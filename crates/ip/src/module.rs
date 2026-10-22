use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, LazyLock};

use arc_swap::ArcSwap;
use netray_common::enrichment::{EnrichmentClient, EnrichmentSource, IpInfo};
use netray_common::target_policy::is_allowed_target;
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::evidence::{Block, BlockId, Node, Register, Row, Shape, ShapeKind, Value as EvValue, ValueType};
use netray_model::{CheckId, CheckResult, Protocol, Status};

use crate::backend::{DnsCache, Ifconfig, UserAgent, new_dns_cache};
use crate::config::{Config, ModuleConfig};
use crate::enrichment::{EnrichmentContext, LoadError};
use crate::lookup::{
    AsnLookupResponse, IpLookup, IpRefusal, LookupOptions, RangeResponse, asn_lookup, enrich, range_lookup,
};

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

/// The IP register: one table row per looked-up public address.
pub const REGISTER: Register = Register {
    blocks: &[
        ("ip.addresses", ShapeKind::Table),
        ("ip.lookup", ShapeKind::Fields),
        ("ip.headers", ShapeKind::Tree),
    ],
    keys: &[
        ("ip.addresses", "address", ValueType::Addr),
        ("ip.addresses", "asn", ValueType::Text),
        ("ip.addresses", "org", ValueType::Text),
        ("ip.addresses", "prefix", ValueType::Text),
        ("ip.addresses", "network_type", ValueType::Text),
        ("ip.addresses", "country", ValueType::Text),
        ("ip.addresses", "city", ValueType::Text),
        ("ip.addresses", "spamhaus", ValueType::Bool),
        ("ip.addresses", "c2", ValueType::Bool),
        ("ip.addresses", "tor", ValueType::Bool),
        ("ip.addresses", "vpn", ValueType::Bool),
        ("ip.lookup", "address", ValueType::Addr),
        ("ip.lookup", "hostname", ValueType::Text),
        ("ip.lookup", "asn", ValueType::Text),
        ("ip.lookup", "org", ValueType::Text),
        ("ip.lookup", "prefix", ValueType::Text),
        ("ip.lookup", "network_type", ValueType::Text),
        ("ip.lookup", "infra_type", ValueType::Text),
        ("ip.lookup", "asn_category", ValueType::Text),
        ("ip.lookup", "network_role", ValueType::Text),
        ("ip.lookup", "asn_registered", ValueType::Text),
        ("ip.lookup", "iana_label", ValueType::Text),
        ("ip.lookup", "cloud_provider", ValueType::Text),
        ("ip.lookup", "cloud_service", ValueType::Text),
        ("ip.lookup", "cloud_region", ValueType::Text),
        ("ip.lookup", "vpn_provider", ValueType::Text),
        ("ip.lookup", "bot_provider", ValueType::Text),
        ("ip.lookup", "city", ValueType::Text),
        ("ip.lookup", "region", ValueType::Text),
        ("ip.lookup", "region_code", ValueType::Text),
        ("ip.lookup", "country", ValueType::Text),
        ("ip.lookup", "country_iso", ValueType::Text),
        ("ip.lookup", "postal_code", ValueType::Text),
        ("ip.lookup", "latitude", ValueType::Text),
        ("ip.lookup", "longitude", ValueType::Text),
        ("ip.lookup", "timezone", ValueType::Text),
        ("ip.lookup", "continent", ValueType::Text),
        ("ip.lookup", "continent_code", ValueType::Text),
        ("ip.lookup", "registered_country", ValueType::Text),
        ("ip.lookup", "registered_country_iso", ValueType::Text),
        ("ip.lookup", "ua_raw", ValueType::Text),
        ("ip.lookup", "ua_browser", ValueType::Text),
        ("ip.lookup", "ua_browser_version", ValueType::Text),
        ("ip.lookup", "ua_os", ValueType::Text),
        ("ip.lookup", "ua_os_version", ValueType::Text),
        ("ip.lookup", "ua_device", ValueType::Text),
        ("ip.lookup", "tcp_port", ValueType::Int),
        ("ip.lookup", "accuracy_km", ValueType::Int),
        ("ip.lookup", "internal", ValueType::Bool),
        ("ip.lookup", "datacenter", ValueType::Bool),
        ("ip.lookup", "vpn", ValueType::Bool),
        ("ip.lookup", "tor", ValueType::Bool),
        ("ip.lookup", "bot", ValueType::Bool),
        ("ip.lookup", "c2", ValueType::Bool),
        ("ip.lookup", "spamhaus", ValueType::Bool),
        ("ip.lookup", "anycast", ValueType::Bool),
        ("ip.lookup", "cins", ValueType::Bool),
        ("ip.lookup", "eu", ValueType::Bool),
        ("ip.headers", "forwarded_for", ValueType::Text),
        ("ip.headers", "name", ValueType::Text),
        ("ip.headers", "value", ValueType::Text),
    ],
};

const ADDRESSES_COLUMNS: [&str; 11] = [
    "address",
    "asn",
    "org",
    "prefix",
    "network_type",
    "country",
    "city",
    "spamhaus",
    "c2",
    "tor",
    "vpn",
];

/// The IP enrichment as an engine module: ifconfig's lookups run in-process.
pub struct IpModule {
    source: Source,
}

enum Source {
    Live {
        config: Arc<Config>,
        enrichment: Arc<ArcSwap<EnrichmentContext>>,
        dns_cache: DnsCache,
    },
    /// Answers every address with the decoded ifconfig response; no GeoIP data.
    #[cfg(feature = "testing")]
    Stub(String),
}

impl IpModule {
    pub async fn new(module: ModuleConfig) -> Result<Self, LoadError> {
        let config = Config::from_module(module);
        let ctx = EnrichmentContext::load_for_module(&config).await?;
        Ok(Self {
            source: Source::Live {
                config: Arc::new(config),
                enrichment: Arc::new(ArcSwap::from_pointee(ctx)),
                dns_cache: new_dns_cache(),
            },
        })
    }

    #[cfg(feature = "testing")]
    pub(crate) fn stub(contract_json: &str) -> Self {
        serde_json::from_str::<Ifconfig>(contract_json).expect("stub contract decodes as an ifconfig response");
        Self {
            source: Source::Stub(contract_json.to_string()),
        }
    }

    /// Loads the data again; a failed load keeps the previous data.
    pub async fn reload(&self) {
        let (config, enrichment) = match &self.source {
            Source::Live { config, enrichment, .. } => (config, enrichment),
            #[cfg(feature = "testing")]
            Source::Stub(_) => return,
        };
        match EnrichmentContext::load_for_module(config).await {
            Ok(ctx) => {
                enrichment.store(Arc::new(ctx));
                tracing::info!("Enrichment data reloaded successfully");
            }
            Err(e) => {
                tracing::warn!("Failed to reload enrichment data: {}; keeping previous context", e);
            }
        }
    }
}

#[cfg(feature = "testing")]
fn stub_answer(json: &str) -> Result<Ifconfig, IpRefusal> {
    serde_json::from_str(json).map_err(|e| IpRefusal::Unavailable(e.to_string()))
}

impl IpLookup for IpModule {
    fn lookup(&self, ip: IpAddr) -> BoxFuture<'_, Result<Ifconfig, IpRefusal>> {
        self.lookup_with(ip, LookupOptions::default())
    }

    fn lookup_caller<'a>(
        &'a self,
        remote: SocketAddr,
        user_agent: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Ifconfig, IpRefusal>> {
        self.lookup_caller_with(
            remote,
            user_agent,
            LookupOptions {
                reverse_dns: true,
                lang: None,
            },
        )
    }

    fn lookup_with(&self, ip: IpAddr, options: LookupOptions) -> BoxFuture<'_, Result<Ifconfig, IpRefusal>> {
        Box::pin(async move {
            match &self.source {
                Source::Live {
                    enrichment, dns_cache, ..
                } => {
                    let ctx = enrichment.load_full();
                    let skip_dns = !options.reverse_dns;
                    Ok(enrich(&ctx, dns_cache, SocketAddr::new(ip, 0), None, skip_dns, options.lang).await)
                }
                #[cfg(feature = "testing")]
                Source::Stub(json) => stub_answer(json),
            }
        })
    }

    fn lookup_caller_with<'a>(
        &'a self,
        remote: SocketAddr,
        user_agent: Option<&'a str>,
        options: LookupOptions,
    ) -> BoxFuture<'a, Result<Ifconfig, IpRefusal>> {
        Box::pin(async move {
            match &self.source {
                Source::Live {
                    enrichment, dns_cache, ..
                } => {
                    let ctx = enrichment.load_full();
                    let skip_dns = !options.reverse_dns;
                    let mut ifconfig = enrich(&ctx, dns_cache, remote, user_agent, skip_dns, options.lang).await;
                    if ifconfig.user_agent.is_none() {
                        ifconfig.user_agent = user_agent.map(UserAgent::unparsed);
                    }
                    Ok(ifconfig)
                }
                #[cfg(feature = "testing")]
                Source::Stub(json) => stub_answer(json),
            }
        })
    }

    fn geoip_build_epoch(&self) -> Option<u64> {
        match &self.source {
            Source::Live { enrichment, .. } => enrichment.load().geoip_city_build_epoch,
            #[cfg(feature = "testing")]
            Source::Stub(_) => None,
        }
    }

    fn range<'a>(&'a self, cidr: &'a str) -> BoxFuture<'a, Result<RangeResponse, IpRefusal>> {
        Box::pin(async move {
            match &self.source {
                Source::Live { enrichment, .. } => range_lookup(&enrichment.load(), cidr),
                #[cfg(feature = "testing")]
                Source::Stub(_) => Err(IpRefusal::Unavailable("stub module holds no range data".into())),
            }
        })
    }

    fn asn(&self, n: u32) -> BoxFuture<'_, Result<AsnLookupResponse, IpRefusal>> {
        Box::pin(async move {
            match &self.source {
                Source::Live { enrichment, .. } => Ok(asn_lookup(&enrichment.load(), n)),
                #[cfg(feature = "testing")]
                Source::Stub(_) => Err(IpRefusal::Unavailable("stub module holds no ASN data".into())),
            }
        })
    }
}

impl EnrichmentSource for IpModule {
    fn lookup(&self, ip: IpAddr) -> futures::future::BoxFuture<'_, Option<IpInfo>> {
        Box::pin(async move {
            let ifconfig = IpLookup::lookup(self, ip).await.ok()?;
            let network = serde_json::to_value(&ifconfig.network).ok()?;
            serde_json::from_value(network).ok()
        })
    }
}

/// The client the DNS, TLS, HTTP and email modules enrich through: `None` without an IP module.
pub fn enrichment_client(ip: Option<&Arc<IpModule>>) -> Option<Arc<EnrichmentClient>> {
    ip.map(|ip| Arc::new(EnrichmentClient::in_process(ip.clone())))
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
                let result = IpLookup::lookup(self, ip).await.map_err(|refusal| refusal.to_string());
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

    let mut rows: Vec<Row> = Vec::new();
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

        let text = |v: &Option<String>| EvValue::Text(v.clone().unwrap_or_default());
        rows.push(vec![
            EvValue::Addr(*ip),
            EvValue::Text(n.asn.map(|asn| format!("AS{asn}")).unwrap_or_default()),
            text(&n.org),
            text(&n.prefix),
            EvValue::Text(n.network_type.clone()),
            text(&e.location.country),
            text(&e.location.city),
            EvValue::Bool(n.is_spamhaus),
            EvValue::Bool(n.is_c2),
            EvValue::Bool(n.is_tor),
            EvValue::Bool(n.is_vpn),
        ]);
    }

    if lookups.len() < total {
        reputation_messages.push(format!("checked {} of {total} addresses", lookups.len()));
    }

    let reputation_check = CheckResult {
        id: ip_checks()[0].clone(),
        status: worst,
        findings: reputation_messages,
        evidence: vec![BlockId::parse("ip.addresses").expect("static block id")],
    };

    SectionOutcome::Measured {
        checks: vec![reputation_check],
        evidence: vec![Block {
            id: BlockId::parse("ip.addresses").expect("static block id"),
            title: "Addresses".into(),
            shape: Shape::Table {
                key: "address".into(),
                columns: ADDRESSES_COLUMNS.map(String::from).into(),
                rows,
            },
        }],
    }
}

fn push_text(fields: &mut Vec<(String, EvValue)>, key: &str, value: Option<&str>) {
    if let Some(v) = value.filter(|v| !v.is_empty()) {
        fields.push((key.into(), EvValue::Text(v.into())));
    }
}

/// The `ip.lookup` block: one lookup document as fields, each key only when known; the nine flags
/// are always present.
pub fn lookup_block(ifconfig: &Ifconfig) -> Block {
    let n = &ifconfig.network;
    let l = &ifconfig.location;
    let mut f: Vec<(String, EvValue)> = Vec::new();
    if let Ok(addr) = ifconfig.ip.addr.parse::<IpAddr>() {
        f.push(("address".into(), EvValue::Addr(addr)));
    }
    push_text(&mut f, "hostname", ifconfig.ip.hostname.as_deref());
    if let Some(tcp) = &ifconfig.tcp {
        f.push(("tcp_port".into(), EvValue::Int(i64::from(tcp.port))));
    }
    let asn = n.asn.map(|asn| format!("AS{asn}"));
    let cloud = n.cloud.as_ref();
    for (key, value) in [
        ("asn", asn.as_deref()),
        ("org", n.org.as_deref()),
        ("prefix", n.prefix.as_deref()),
        ("network_type", Some(n.network_type.as_str())),
        ("infra_type", Some(n.infra_type.as_str())),
        ("asn_category", n.asn_category.as_deref()),
        ("network_role", n.network_role.as_deref()),
        ("asn_registered", n.asn_registered.as_deref()),
        ("iana_label", n.iana_label.as_deref()),
        ("cloud_provider", cloud.map(|c| c.provider.as_str())),
        ("cloud_service", cloud.and_then(|c| c.service.as_deref())),
        ("cloud_region", cloud.and_then(|c| c.region.as_deref())),
        ("vpn_provider", n.vpn.as_ref().and_then(|v| v.provider.as_deref())),
        ("bot_provider", n.bot.as_ref().map(|b| b.provider.as_str())),
    ] {
        push_text(&mut f, key, value);
    }
    for (key, flag) in [
        ("internal", n.is_internal),
        ("datacenter", n.is_datacenter),
        ("vpn", n.is_vpn),
        ("tor", n.is_tor),
        ("bot", n.is_bot),
        ("c2", n.is_c2),
        ("spamhaus", n.is_spamhaus),
        ("anycast", n.is_anycast),
        ("cins", n.is_cins),
    ] {
        f.push((key.into(), EvValue::Bool(flag)));
    }
    let latitude = l.latitude.map(|v| v.to_string());
    let longitude = l.longitude.map(|v| v.to_string());
    for (key, value) in [
        ("city", l.city.as_deref()),
        ("region", l.region.as_deref()),
        ("region_code", l.region_code.as_deref()),
        ("country", l.country.as_deref()),
        ("country_iso", l.country_iso.as_deref()),
        ("postal_code", l.postal_code.as_deref()),
        ("latitude", latitude.as_deref()),
        ("longitude", longitude.as_deref()),
        ("timezone", l.timezone.as_deref()),
        ("continent", l.continent.as_deref()),
        ("continent_code", l.continent_code.as_deref()),
        ("registered_country", l.registered_country.as_deref()),
        ("registered_country_iso", l.registered_country_iso.as_deref()),
    ] {
        push_text(&mut f, key, value);
    }
    if let Some(km) = l.accuracy_radius_km {
        f.push(("accuracy_km".into(), EvValue::Int(i64::from(km))));
    }
    if let Some(eu) = l.is_eu {
        f.push(("eu".into(), EvValue::Bool(eu)));
    }
    if let Some(ua) = &ifconfig.user_agent {
        fn known(family: &str) -> Option<&str> {
            (family != "Other").then_some(family)
        }
        for (key, value) in [
            ("ua_raw", ua.raw.as_deref()),
            ("ua_browser", known(&ua.browser.family)),
            ("ua_browser_version", Some(ua.browser.version.as_str())),
            ("ua_os", known(&ua.os.family)),
            ("ua_os_version", Some(ua.os.version.as_str())),
            ("ua_device", known(&ua.device.family)),
        ] {
            push_text(&mut f, key, value);
        }
    }

    Block {
        id: BlockId::parse("ip.lookup").expect("static block id"),
        title: "Lookup".into(),
        shape: Shape::Fields(f),
    }
}

/// The IP section of one looked-up address: `translate`'s verdicts, with the `ip.lookup` block in
/// place of `ip.addresses`.
pub fn translate_lookup(ip: IpAddr, result: Result<Ifconfig, String>) -> SectionOutcome {
    let lookups = [(ip, result)];
    match translate(&lookups, 1) {
        SectionOutcome::Measured { mut checks, .. } => {
            let Ok(ifconfig) = &lookups[0].1 else {
                unreachable!("a measured outcome comes from an Ok lookup");
            };
            let block = lookup_block(ifconfig);
            for check in &mut checks {
                check.evidence = vec![block.id.clone()];
            }
            SectionOutcome::Measured {
                checks,
                evidence: vec![block],
            }
        }
        other => other,
    }
}

/// The `ip.headers` block: the caller's X-Forwarded-For hops at the root and one child per
/// request header.
pub fn headers_block(forwarded_for: &[String], headers: &[(String, String)]) -> Block {
    let text = |v: &str| EvValue::Text(v.into());
    Block {
        id: BlockId::parse("ip.headers").expect("static block id"),
        title: "Request headers".into(),
        shape: Shape::Tree(Node {
            // An empty hop (a trailing comma) is no hop; a placeholder is never published.
            fields: forwarded_for
                .iter()
                .filter(|hop| !hop.is_empty())
                .map(|hop| ("forwarded_for".to_string(), text(hop)))
                .collect(),
            children: headers
                .iter()
                .map(|(name, value)| Node {
                    fields: vec![("name".into(), text(name)), ("value".into(), text(value))],
                    children: Vec::new(),
                })
                .collect(),
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
