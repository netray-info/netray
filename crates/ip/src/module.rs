use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, LazyLock};

use arc_swap::ArcSwap;
use netray_common::target_policy::is_allowed_target;
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use serde_json::json;

use crate::backend::{DnsCache, Ifconfig, IfconfigParam, get_ifconfig, new_dns_cache};
use crate::config::{Config, ModuleConfig};
use crate::enrichment::{EnrichmentContext, LoadError};

// ---------------------------------------------------------------------------
// Module
// ---------------------------------------------------------------------------

/// Addresses queried per family; the rest of a larger answer is not looked at.
const MAX_PER_FAMILY: usize = 4;

static CHECKS: LazyLock<Vec<CheckId>> =
    LazyLock::new(|| vec![CheckId::parse("ip.reputation").expect("static check id")]);

pub(crate) fn ip_checks() -> &'static [CheckId] {
    CHECKS.as_slice()
}

/// The IP enrichment as an engine module: ifconfig's lookups run in-process.
pub struct IpModule {
    config: Arc<Config>,
    enrichment: Arc<ArcSwap<EnrichmentContext>>,
    dns_cache: DnsCache,
}

impl IpModule {
    pub async fn new(module: ModuleConfig) -> Result<Self, LoadError> {
        let config = Config::from_module(module);
        let ctx = EnrichmentContext::load(&config).await?;
        Ok(Self {
            config: Arc::new(config),
            enrichment: Arc::new(ArcSwap::from_pointee(ctx)),
            dns_cache: new_dns_cache(),
        })
    }

    /// Loads the data again; a failed load keeps the previous data.
    pub async fn reload(&self) {
        crate::reload_enrichment(&self.enrichment, &self.config, "reload").await;
    }

    async fn lookup(&self, ip: IpAddr) -> Result<Ifconfig, String> {
        let ctx = self.enrichment.load();
        let remote = SocketAddr::new(ip, 0);
        let param = IfconfigParam {
            remote: &remote,
            user_agent_header: &None,
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
            dns_cache: &self.dns_cache,
            skip_dns: true,
            lang: None,
        };
        Ok(get_ifconfig(&param).await)
    }
}

/// The public addresses among `facts` (those `is_allowed_target` accepts), at most four IPv4 and
/// four IPv6 of them in ascending order, and the count of all addresses in `facts`, private ones
/// included.
pub(crate) fn sample(facts: &Facts) -> (Vec<IpAddr>, usize) {
    let total = facts.a.len() + facts.aaaa.len();
    let (mut v4, mut v6): (Vec<IpAddr>, Vec<IpAddr>) = facts
        .a
        .iter()
        .copied()
        .map(IpAddr::V4)
        .chain(facts.aaaa.iter().copied().map(IpAddr::V6))
        .filter(|ip| is_allowed_target(*ip))
        .partition(IpAddr::is_ipv4);
    v4.sort();
    v6.sort();
    let sample = v4
        .into_iter()
        .take(MAX_PER_FAMILY)
        .chain(v6.into_iter().take(MAX_PER_FAMILY))
        .collect();
    (sample, total)
}

impl Module for IpModule {
    fn protocol(&self) -> Protocol {
        Protocol::Ip
    }

    fn checks(&self) -> &'static [CheckId] {
        ip_checks()
    }

    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }

    fn needs_addresses(&self) -> bool {
        true
    }

    fn run<'a>(&'a self, _ctx: &'a RunContext, facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async move {
            let (sample, total) = sample(facts);
            let lookups = futures::future::join_all(sample.into_iter().map(|ip| async move {
                let result = self.lookup(ip).await;
                (ip, result)
            }))
            .await;
            translate(&lookups, total)
        })
    }
}

// ---------------------------------------------------------------------------
// Translation
// ---------------------------------------------------------------------------

struct IpInfo {
    ip: IpAddr,
    org: Option<String>,
    geo: Option<String>,
    network_type: String,
}

/// The IP section of the lookups of the sampled public addresses; `total` is how many addresses
/// the domain resolved to, private ones included.
pub fn translate(lookups: &[(IpAddr, Result<Ifconfig, String>)], total: usize) -> SectionOutcome {
    if lookups.is_empty() {
        return SectionOutcome::NotApplicable {
            reason: "no public addresses".into(),
        };
    }

    let mut entries = Vec::with_capacity(lookups.len());
    for (_, result) in lookups {
        match result {
            Ok(e) => entries.push(e),
            Err(reason) => {
                return SectionOutcome::Incomplete { reason: reason.clone() };
            }
        }
    }

    let mut addresses: Vec<IpInfo> = Vec::new();
    let mut worst = Status::Pass;
    let mut reputation_messages: Vec<String> = Vec::new();

    for ((ip, _), e) in lookups.iter().zip(entries) {
        let n = &e.network;
        let mut flags: Vec<&str> = Vec::new();
        if n.is_spamhaus {
            flags.push("spamhaus");
        }
        if n.is_c2 {
            flags.push("c2");
        }
        if n.is_tor {
            flags.push("tor");
        }
        if n.is_vpn {
            flags.push("vpn");
        }
        let verdict = if n.is_spamhaus || n.is_c2 || n.is_tor {
            Status::Fail
        } else if n.is_vpn {
            Status::Warn
        } else {
            Status::Pass
        };
        if verdict_rank(verdict) > verdict_rank(worst) {
            worst = verdict;
        }
        for flag in flags {
            reputation_messages.push(format!("{ip}: {flag}"));
        }

        addresses.push(IpInfo {
            ip: *ip,
            org: n.org.clone(),
            geo: build_geo(&e.location),
            network_type: n.network_type.clone(),
        });
    }

    if lookups.len() < total {
        reputation_messages.push(format!("checked {} of {total} addresses", lookups.len()));
    }

    let reputation_check = CheckResult {
        id: ip_checks()[0].clone(),
        status: worst,
        findings: reputation_messages,
        evidence: vec![],
    };

    let headline = build_headline(&addresses);
    SectionOutcome::Measured {
        checks: vec![reputation_check],
        presentation: json!({
            "headline": headline,
            "addresses": addresses
                .iter()
                .map(|a| json!({
                    "ip": a.ip.to_string(),
                    "org": a.org,
                    "geo": a.geo,
                    "network_type": a.network_type,
                }))
                .collect::<Vec<_>>(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Rank verdicts so we can find the worst: higher rank = worse.
fn verdict_rank(v: Status) -> u8 {
    match v {
        Status::Pass | Status::NotApplicable | Status::NotTested | Status::Unmeasured => 0,
        Status::Warn => 2,
        Status::Fail => 3,
    }
}

fn build_geo(location: &crate::backend::Location) -> Option<String> {
    match (&location.city, &location.country) {
        (Some(city), Some(country)) => Some(format!("{city}, {country}")),
        (None, Some(country)) => Some(country.clone()),
        (Some(city), None) => Some(city.clone()),
        (None, None) => None,
    }
}

/// Build a short headline from org names and geo locations.
fn build_headline(addresses: &[IpInfo]) -> String {
    if addresses.is_empty() {
        return String::new();
    }

    // Collect unique org names (up to 3).
    let orgs: Vec<&str> = addresses
        .iter()
        .filter_map(|a| a.org.as_deref())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .take(3)
        .collect();

    // Collect unique geo locations (up to 2).
    let geos: Vec<&str> = addresses
        .iter()
        .filter_map(|a| a.geo.as_deref())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .take(2)
        .collect();

    match (orgs.is_empty(), geos.is_empty()) {
        (false, false) => format!("{} + {}", orgs.join(", "), geos.join(" + ")),
        (false, true) => orgs.join(", "),
        (true, false) => geos.join(" + "),
        (true, true) => format!("{} address(es)", addresses.len()),
    }
}
