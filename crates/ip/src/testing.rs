use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};

use futures::future::BoxFuture;
use netray_engine::{EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::evidence::{Block, Shape, Value};
use netray_model::{CheckId, Protocol};

use crate::backend::{Ifconfig, Ip, Location, Network, Tcp, UserAgent};
use crate::lookup::{AsnLookupResponse, IpLookup, IpRefusal, LookupOptions, RangeResponse, parse_range};
use crate::module::{ip_checks, sample, translate};

struct GoldenModule(String);

impl Module for GoldenModule {
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
            let lookups: Vec<_> = sample.into_iter().map(|ip| (ip, Ok(decode(&self.0)))).collect();
            translate(&lookups, total)
        })
    }
}

fn decode(contract_json: &str) -> Ifconfig {
    serde_json::from_str(contract_json).expect("golden contract decodes as an ifconfig response")
}

/// An IP module that answers every sampled address with the decoded `contract_json`, an
/// ifconfig `/json` response.
pub fn golden_module(contract_json: &str) -> Box<dyn Module> {
    decode(contract_json);
    Box::new(GoldenModule(contract_json.to_string()))
}

const IFCONFIG_FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/contracts/ifconfig-json.json"
));

/// The GeoIP build epoch [`StubIpLookup`] reports.
pub const STUB_GEOIP_EPOCH: u64 = 1_700_000_000;

/// An [`IpLookup`] that answers from the committed ifconfig `/json` fixture, with no data files and
/// no network.
#[derive(Debug, Default)]
pub struct StubIpLookup;

impl StubIpLookup {
    pub fn new() -> Self {
        Self
    }

    fn answer(ip: IpAddr) -> Ifconfig {
        let mut ifconfig = decode(IFCONFIG_FIXTURE);
        ifconfig.ip.addr = ip.to_string();
        ifconfig.ip.version = if ip.is_ipv4() { "4" } else { "6" }.to_string();
        ifconfig.tcp = None;
        ifconfig.user_agent = None;
        ifconfig
    }

    fn caller_answer(remote: SocketAddr, user_agent: Option<&str>) -> Ifconfig {
        let mut ifconfig = Self::answer(remote.ip());
        if remote.port() != 0 {
            ifconfig.tcp = Some(Tcp { port: remote.port() });
        }
        ifconfig.user_agent = user_agent.map(UserAgent::unparsed);
        ifconfig
    }

    fn with_options(mut ifconfig: Ifconfig, options: LookupOptions) -> Ifconfig {
        if options.reverse_dns {
            ifconfig.ip.hostname = Some("stub.example.com".to_string());
        }
        if options.lang.as_deref() == Some("de") {
            ifconfig.location.city_localized = Some("Berlin".to_string());
            ifconfig.location.country_localized = Some("Deutschland".to_string());
        }
        ifconfig
    }
}

impl IpLookup for StubIpLookup {
    fn lookup(&self, ip: IpAddr) -> BoxFuture<'_, Result<Ifconfig, IpRefusal>> {
        Box::pin(async move { Ok(Self::answer(ip)) })
    }

    fn lookup_caller<'a>(
        &'a self,
        remote: SocketAddr,
        user_agent: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Ifconfig, IpRefusal>> {
        Box::pin(async move { Ok(Self::caller_answer(remote, user_agent)) })
    }

    fn lookup_with(&self, ip: IpAddr, options: LookupOptions) -> BoxFuture<'_, Result<Ifconfig, IpRefusal>> {
        Box::pin(async move { Ok(Self::with_options(Self::answer(ip), options)) })
    }

    fn lookup_caller_with<'a>(
        &'a self,
        remote: SocketAddr,
        user_agent: Option<&'a str>,
        options: LookupOptions,
    ) -> BoxFuture<'a, Result<Ifconfig, IpRefusal>> {
        Box::pin(async move { Ok(Self::with_options(Self::caller_answer(remote, user_agent), options)) })
    }

    fn geoip_build_epoch(&self) -> Option<u64> {
        Some(STUB_GEOIP_EPOCH)
    }

    fn range<'a>(&'a self, cidr: &'a str) -> BoxFuture<'a, Result<RangeResponse, IpRefusal>> {
        Box::pin(async move {
            parse_range(cidr)?;
            Ok(RangeResponse {
                cidr: cidr.to_string(),
                asn: Some(64496),
                org: Some("Example Cloud GmbH".to_string()),
                network_type: "cloud".to_string(),
                is_cloud: true,
                is_vpn: false,
                is_datacenter: true,
                is_spamhaus: false,
                is_c2: false,
            })
        })
    }

    fn asn(&self, n: u32) -> BoxFuture<'_, Result<AsnLookupResponse, IpRefusal>> {
        Box::pin(async move {
            Ok(AsnLookupResponse {
                asn: n,
                org: None,
                category: Some("hosting".to_string()),
                network_role: Some("stub".to_string()),
                is_anycast: false,
            })
        })
    }
}

fn column(columns: &[String], name: &str) -> Option<usize> {
    columns.iter().position(|c| c == name)
}

fn cell_text(row: &[Value], at: Option<usize>) -> Option<String> {
    match at.and_then(|i| row.get(i)) {
        Some(Value::Text(t)) if !t.is_empty() => Some(t.clone()),
        _ => None,
    }
}

fn cell_flag(row: &[Value], at: Option<usize>) -> bool {
    matches!(at.and_then(|i| row.get(i)), Some(Value::Bool(true)))
}

fn row_lookup(columns: &[String], row: &[Value]) -> Option<(IpAddr, Ifconfig)> {
    let ip = match row.get(column(columns, "address")?)? {
        Value::Addr(ip) => *ip,
        _ => return None,
    };
    let at = |name| column(columns, name);
    let asn = cell_text(row, at("asn")).and_then(|t| t.strip_prefix("AS")?.parse().ok());
    let mut location = Location::unknown();
    location.country = cell_text(row, at("country"));
    location.city = cell_text(row, at("city"));
    let network = Network {
        asn,
        org: cell_text(row, at("org")),
        prefix: cell_text(row, at("prefix")),
        asn_category: None,
        network_role: None,
        asn_registered: None,
        network_type: cell_text(row, at("network_type")).unwrap_or_default(),
        infra_type: String::new(),
        is_internal: false,
        is_datacenter: false,
        is_vpn: cell_flag(row, at("vpn")),
        is_tor: cell_flag(row, at("tor")),
        is_bot: false,
        is_c2: cell_flag(row, at("c2")),
        is_spamhaus: cell_flag(row, at("spamhaus")),
        cloud: None,
        vpn: None,
        bot: None,
        is_anycast: false,
        is_cins: false,
        iana_label: None,
    };
    let ifconfig = Ifconfig {
        ip: Ip {
            addr: ip.to_string(),
            version: if ip.is_ipv4() { "4" } else { "6" }.to_string(),
            hostname: None,
        },
        tcp: None,
        location,
        network,
        user_agent: None,
    };
    Some((ip, ifconfig))
}

/// The IP section rebuilt from its own `ip.addresses` block; `total` counts the distinct A
/// addresses of `dns.a` and AAAA addresses of `dns.aaaa`, or the rows when neither block is
/// present.
pub async fn from_evidence(_domain: &str, blocks: &[Block]) -> SectionOutcome {
    let block = |id: &str| blocks.iter().find(|b| b.id.to_string() == id);
    let lookups: Vec<(IpAddr, Result<Ifconfig, String>)> = match block("ip.addresses").map(|b| &b.shape) {
        Some(Shape::Table { columns, rows, .. }) => rows
            .iter()
            .filter_map(|row| row_lookup(columns, row))
            .map(|(ip, ifconfig)| (ip, Ok(ifconfig)))
            .collect(),
        _ => Vec::new(),
    };
    let addresses = |id: &str, rtype: &str| match block(id).map(|b| &b.shape) {
        Some(Shape::Records(records)) => Some(
            records
                .iter()
                .filter(|r| matches!(&r.rtype, Value::Text(t) if t == rtype))
                .map(|r| &r.data)
                .collect::<HashSet<_>>()
                .len(),
        ),
        _ => None,
    };
    let total = match (addresses("dns.a", "A"), addresses("dns.aaaa", "AAAA")) {
        (None, None) => lookups.len(),
        (a, aaaa) => a.unwrap_or(0) + aaaa.unwrap_or(0),
    };
    translate(&lookups, total)
}
