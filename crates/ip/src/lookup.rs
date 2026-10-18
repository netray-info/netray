//! The lookup logic of the ifconfig routes as functions over an `EnrichmentContext`, shared by those
//! routes, the engine module and the `/api/v1` ip functions.
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use futures::StreamExt;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};

use crate::backend::{
    DnsCache, Ifconfig, IfconfigParam, Ip, Location, Network, NetworkFlags, Tcp, UserAgent, classify_network_type,
    get_ifconfig, is_global_ip,
};
use crate::enrichment::EnrichmentContext;

const BATCH_CONCURRENCY: usize = 10;
const BATCH_LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);

/// The IP lookups an embedder can call without the HTTP layer.
pub trait IpLookup: Send + Sync + 'static {
    /// One address: no reverse DNS, no User-Agent, port 0.
    fn lookup(&self, ip: IpAddr) -> BoxFuture<'_, Result<Ifconfig, IpRefusal>>;

    /// The caller: reverse DNS on, the User-Agent header parsed, `tcp` the remote port unless it is 0.
    fn lookup_caller<'a>(
        &'a self,
        remote: SocketAddr,
        user_agent: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Ifconfig, IpRefusal>>;

    /// A CIDR prefix, already percent-decoded; `RangeResponse::cidr` echoes it.
    fn range<'a>(&'a self, cidr: &'a str) -> BoxFuture<'a, Result<RangeResponse, IpRefusal>>;

    /// What is known about an autonomous system number.
    fn asn(&self, n: u32) -> BoxFuture<'_, Result<AsnLookupResponse, IpRefusal>>;
}

/// Why a lookup was not answered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IpRefusal {
    /// The input is not acceptable.
    #[error("{0}")]
    InvalidInput(String),
    /// The lookup could not be completed.
    #[error("{0}")]
    Unavailable(String),
}

fn invalid(message: &str) -> IpRefusal {
    IpRefusal::InvalidInput(message.to_string())
}

/// Builds the answer for `remote`; the PTR lookup runs unless `skip_dns`.
pub async fn enrich(
    ctx: &EnrichmentContext,
    dns_cache: &DnsCache,
    remote: SocketAddr,
    user_agent: Option<&str>,
    skip_dns: bool,
    lang: Option<String>,
) -> Ifconfig {
    let param = IfconfigParam {
        remote: &remote,
        user_agent_header: &user_agent,
        user_agent_parser: ctx.user_agent_parser.as_deref(),
        geoip_city_db: ctx.geoip_city_db.as_deref(),
        geoip_asn_db: ctx.geoip_asn_db.as_deref(),
        tor_exit_nodes: &ctx.tor_exit_nodes,
        feodo_botnet_ips: ctx.feodo_botnet_ips.as_deref(),
        cins_army_ips: ctx.cins_army_ips.as_deref(),
        vpn_ranges: ctx.vpn_ranges.as_deref(),
        cloud_provider_db: ctx.cloud_provider_db.as_deref(),
        datacenter_ranges: ctx.datacenter_ranges.as_deref(),
        bot_db: ctx.bot_db.as_deref(),
        spamhaus_drop: ctx.spamhaus_drop.as_deref(),
        asn_patterns: &ctx.asn_patterns,
        asn_info: ctx.asn_info.as_deref(),
        dns_resolver: &ctx.dns_resolver,
        dns_cache,
        skip_dns,
        lang,
    };
    get_ifconfig(&param).await
}

/// Refuses a private, loopback or link-local address unless `internal_mode`.
pub fn check_address(ip: IpAddr, internal_mode: bool) -> Result<(), IpRefusal> {
    if !internal_mode && !is_global_ip(ip) {
        return Err(invalid("private/loopback IP not allowed"));
    }
    Ok(())
}

/// Parses `s` as an address and applies [`check_address`].
pub fn parse_address(s: &str, internal_mode: bool) -> Result<IpAddr, IpRefusal> {
    let ip: IpAddr = s.parse().map_err(|_| invalid("invalid IP address"))?;
    check_address(ip, internal_mode)?;
    Ok(ip)
}

/// Parses both addresses of a diff request.
pub fn parse_diff_pair(req: &DiffRequest, internal_mode: bool) -> Result<(IpAddr, IpAddr), IpRefusal> {
    let a: IpAddr = req
        .a
        .parse()
        .map_err(|_| invalid("invalid IP address in field \"a\""))?;
    let b: IpAddr = req
        .b
        .parse()
        .map_err(|_| invalid("invalid IP address in field \"b\""))?;
    if !internal_mode && !is_global_ip(a) {
        return Err(invalid("IP \"a\" is not a globally routable address"));
    }
    if !internal_mode && !is_global_ip(b) {
        return Err(invalid("IP \"b\" is not a globally routable address"));
    }
    Ok((a, b))
}

fn is_special_prefix(ip: IpAddr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() {
        return true;
    }
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            let segs = v6.segments();
            // ULA fc00::/7
            if segs[0] & 0xfe00 == 0xfc00 {
                return true;
            }
            // Link-local fe80::/10
            if segs[0] & 0xffc0 == 0xfe80 {
                return true;
            }
            false
        }
    }
}

/// Parses a CIDR prefix and refuses private, loopback and link-local ones.
pub fn parse_range(cidr: &str) -> Result<ip_network::IpNetwork, IpRefusal> {
    let network: ip_network::IpNetwork = cidr.parse().map_err(|_| invalid("invalid CIDR notation"))?;
    if is_special_prefix(network.network_address()) {
        return Err(invalid("private, loopback, or link-local prefixes are not allowed"));
    }
    Ok(network)
}

/// What is known about the network address of `cidr`.
pub fn range_lookup(ctx: &EnrichmentContext, cidr: &str) -> Result<RangeResponse, IpRefusal> {
    let network = parse_range(cidr)?;
    let network_addr = network.network_address();

    let (asn_number, asn_org) = ctx
        .geoip_asn_db
        .as_deref()
        .and_then(|db| db.lookup(network_addr))
        .map(|(isp, _prefix)| {
            (
                isp.autonomous_system_number,
                isp.autonomous_system_organization.map(|s| s.to_owned()),
            )
        })
        .unwrap_or((None, None));

    let is_cloud = ctx
        .cloud_provider_db
        .as_deref()
        .and_then(|db| db.lookup(network_addr))
        .is_some();

    let is_vpn = ctx
        .vpn_ranges
        .as_deref()
        .map(|db| db.lookup(network_addr))
        .unwrap_or(false);

    let is_botnet_c2 = ctx
        .feodo_botnet_ips
        .as_deref()
        .and_then(|db| db.lookup(&network_addr))
        .unwrap_or(false);

    let is_threat = ctx
        .spamhaus_drop
        .as_deref()
        .map(|db| db.lookup(network_addr))
        .unwrap_or(false);

    let asn_class =
        crate::backend::asn_heuristic::classify_asn(asn_number, asn_org.as_deref(), ctx.asn_patterns.as_ref());
    let asn_hosting = asn_number
        .and_then(|n| ctx.asn_info.as_deref().and_then(|db| db.lookup(n)))
        .is_some_and(|m| m.category == crate::backend::asn_info::AsnCategory::Hosting);
    let is_datacenter = is_cloud
        || ctx
            .datacenter_ranges
            .as_deref()
            .map(|db| db.lookup(network_addr))
            .unwrap_or(false)
        || matches!(
            asn_class,
            crate::backend::asn_heuristic::AsnClassification::Hosting { .. }
        )
        || asn_hosting;

    let is_tor = ctx.tor_exit_nodes.lookup(&network_addr).unwrap_or(false);
    let is_bot = ctx.bot_db.as_deref().and_then(|db| db.lookup(network_addr)).is_some();
    let is_internal = !is_global_ip(network_addr);
    let is_vpn = is_vpn || matches!(asn_class, crate::backend::asn_heuristic::AsnClassification::Vpn { .. });

    let network_type = classify_network_type(NetworkFlags {
        is_internal,
        is_c2: is_botnet_c2,
        is_bot,
        is_cloud,
        is_vpn,
        is_tor,
        is_spamhaus: is_threat,
        is_datacenter,
    })
    .to_string();

    Ok(RangeResponse {
        cidr: cidr.to_string(),
        asn: asn_number,
        org: asn_org,
        network_type,
        is_cloud,
        is_vpn,
        is_datacenter,
        is_spamhaus: is_threat,
        is_c2: is_botnet_c2,
    })
}

/// What is known about the autonomous system `n`.
pub fn asn_lookup(ctx: &EnrichmentContext, n: u32) -> AsnLookupResponse {
    let asn_meta = ctx.asn_info.as_deref().and_then(|db| db.lookup(n));

    let category = asn_meta.and_then(|m| match m.category {
        crate::backend::asn_info::AsnCategory::Hosting => Some("hosting".to_string()),
        crate::backend::asn_info::AsnCategory::Isp => Some("isp".to_string()),
        crate::backend::asn_info::AsnCategory::Business => Some("business".to_string()),
        crate::backend::asn_info::AsnCategory::EducationResearch => Some("education_research".to_string()),
        crate::backend::asn_info::AsnCategory::GovernmentAdmin => Some("government_admin".to_string()),
        crate::backend::asn_info::AsnCategory::Unknown => None,
    });

    let network_role = asn_meta.and_then(|m| m.network_role.clone());

    // Use an arbitrary IP from the ASN to get org name from MaxMind — not possible without IP.
    // Instead, check the asn_info for the org; MaxMind ASN DB is IP-keyed not ASN-keyed.
    // We return the info we have from asn_info only; org from MaxMind requires an IP.
    // TODO("MaxMind ASN DB is IP-keyed; org name not available for pure ASN lookups without a representative IP")
    let org: Option<String> = asn_meta.and(None); // MaxMind ASN DB is not ASN-keyed

    let is_anycast = crate::backend::ANYCAST_ASNS.contains(&n);

    AsnLookupResponse {
        asn: n,
        org,
        category,
        network_role,
        is_anycast,
    }
}

/// Compares two answers part by part; a part is `equal` when its JSON is.
pub fn diff(a: Ifconfig, b: Ifconfig) -> DiffResponse {
    fn same<T: Serialize>(a: &T, b: &T) -> bool {
        let value = |v: &T| serde_json::to_value(v).unwrap_or(serde_json::Value::Null);
        value(a) == value(b)
    }
    let Ifconfig {
        ip: ip_a,
        tcp: tcp_a,
        location: location_a,
        network: network_a,
        user_agent: user_agent_a,
    } = a;
    let Ifconfig {
        ip: ip_b,
        tcp: tcp_b,
        location: location_b,
        network: network_b,
        user_agent: user_agent_b,
    } = b;
    DiffResponse {
        ip: IpDiff {
            equal: same(&ip_a, &ip_b),
            a: ip_a,
            b: ip_b,
        },
        location: LocationDiff {
            equal: same(&location_a, &location_b),
            a: location_a,
            b: location_b,
        },
        network: NetworkDiff {
            equal: same(&network_a, &network_b),
            a: network_a,
            b: network_b,
        },
        tcp: TcpDiff {
            equal: same(&tcp_a, &tcp_b),
            a: tcp_a,
            b: tcp_b,
        },
        user_agent: UserAgentDiff {
            equal: same(&user_agent_a, &user_agent_b),
            a: user_agent_a,
            b: user_agent_b,
        },
    }
}

/// Runs `f` for every address, at most ten at a time and each for at most five seconds; the answers
/// are in input order.
pub async fn lookup_batch<F, Fut>(ips: &[IpAddr], f: F) -> Vec<Result<Ifconfig, IpRefusal>>
where
    F: Fn(IpAddr) -> Fut,
    Fut: Future<Output = Result<Ifconfig, IpRefusal>>,
{
    futures::stream::iter(ips.iter().copied())
        .map(|ip| {
            let lookup = f(ip);
            async move {
                tokio::time::timeout(BATCH_LOOKUP_TIMEOUT, lookup)
                    .await
                    .unwrap_or_else(|_| Err(IpRefusal::Unavailable("lookup timed out".to_string())))
            }
        })
        .buffered(BATCH_CONCURRENCY)
        .collect()
        .await
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AsnLookupResponse {
    /// ASN number.
    #[schema(example = 15169)]
    pub asn: u32,
    /// Organization name from MaxMind ASN database.
    #[schema(example = "Google LLC")]
    pub org: Option<String>,
    /// ASN category from ipverse/as-metadata.
    #[schema(example = "hosting")]
    pub category: Option<String>,
    /// Network role from ipverse/as-metadata.
    #[schema(example = "tier1_transit")]
    pub network_role: Option<String>,
    /// Whether this ASN is known to be anycast.
    #[schema(example = false)]
    pub is_anycast: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RangeResponse {
    /// The CIDR prefix as provided.
    #[schema(example = "8.8.8.0/24")]
    pub cidr: String,
    /// ASN that originates this prefix (from MaxMind lookup on the network address).
    #[schema(example = 15169)]
    pub asn: Option<u32>,
    /// Organization name for the ASN.
    #[schema(example = "Google LLC")]
    pub org: Option<String>,
    /// Network type classification of the network address.
    #[schema(example = "cloud")]
    pub network_type: String,
    /// True if the network address is a cloud CIDR.
    #[schema(example = false)]
    pub is_cloud: bool,
    /// True if the network address matches VPN ranges.
    #[schema(example = false)]
    pub is_vpn: bool,
    /// True if the network address is a datacenter.
    #[schema(example = false)]
    pub is_datacenter: bool,
    /// True if the network address matches Spamhaus DROP.
    #[schema(example = false)]
    pub is_spamhaus: bool,
    /// True if the network address is a Feodo C2 node.
    #[schema(example = false)]
    pub is_c2: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DiffRequest {
    /// First IP address to compare.
    #[schema(example = "8.8.8.8")]
    pub a: String,
    /// Second IP address to compare.
    #[schema(example = "1.1.1.1")]
    pub b: String,
}

/// The two answers of a diff, one entry per top-level field of `Ifconfig`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DiffResponse {
    pub ip: IpDiff,
    pub location: LocationDiff,
    pub network: NetworkDiff,
    pub tcp: TcpDiff,
    pub user_agent: UserAgentDiff,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct IpDiff {
    pub a: Ip,
    pub b: Ip,
    pub equal: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct LocationDiff {
    pub a: Location,
    pub b: Location,
    pub equal: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct NetworkDiff {
    pub a: Network,
    pub b: Network,
    pub equal: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TcpDiff {
    pub a: Option<Tcp>,
    pub b: Option<Tcp>,
    pub equal: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct UserAgentDiff {
    pub a: Option<UserAgent>,
    pub b: Option<UserAgent>,
    pub equal: bool,
}
