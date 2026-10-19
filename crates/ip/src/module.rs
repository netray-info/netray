use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, LazyLock};

use arc_swap::ArcSwap;
use netray_common::target_policy::is_allowed_target;
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::evidence::{Block, BlockId, Register, Row, Shape, ShapeKind, Value as EvValue, ValueType};
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
    blocks: &[("ip.addresses", ShapeKind::Table)],
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
    config: Arc<Config>,
    enrichment: Arc<ArcSwap<EnrichmentContext>>,
    dns_cache: DnsCache,
}

impl IpModule {
    pub async fn new(module: ModuleConfig) -> Result<Self, LoadError> {
        let config = Config::from_module(module);
        let ctx = EnrichmentContext::load_for_module(&config).await?;
        Ok(Self {
            config: Arc::new(config),
            enrichment: Arc::new(ArcSwap::from_pointee(ctx)),
            dns_cache: new_dns_cache(),
        })
    }

    /// Loads the data again; a failed load keeps the previous data.
    pub async fn reload(&self) {
        match EnrichmentContext::load_for_module(&self.config).await {
            Ok(ctx) => {
                self.enrichment.store(Arc::new(ctx));
                tracing::info!("Enrichment data reloaded successfully");
            }
            Err(e) => {
                tracing::warn!("Failed to reload enrichment data: {}; keeping previous context", e);
            }
        }
    }
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
            let ctx = self.enrichment.load_full();
            let skip_dns = !options.reverse_dns;
            Ok(enrich(
                &ctx,
                &self.dns_cache,
                SocketAddr::new(ip, 0),
                None,
                skip_dns,
                options.lang,
            )
            .await)
        })
    }

    fn lookup_caller_with<'a>(
        &'a self,
        remote: SocketAddr,
        user_agent: Option<&'a str>,
        options: LookupOptions,
    ) -> BoxFuture<'a, Result<Ifconfig, IpRefusal>> {
        Box::pin(async move {
            let ctx = self.enrichment.load_full();
            let skip_dns = !options.reverse_dns;
            let mut ifconfig = enrich(&ctx, &self.dns_cache, remote, user_agent, skip_dns, options.lang).await;
            if ifconfig.user_agent.is_none() {
                ifconfig.user_agent = user_agent.map(UserAgent::unparsed);
            }
            Ok(ifconfig)
        })
    }

    fn geoip_build_epoch(&self) -> Option<u64> {
        self.enrichment.load().geoip_city_build_epoch
    }

    fn range<'a>(&'a self, cidr: &'a str) -> BoxFuture<'a, Result<RangeResponse, IpRefusal>> {
        Box::pin(async move { range_lookup(&self.enrichment.load(), cidr) })
    }

    fn asn(&self, n: u32) -> BoxFuture<'_, Result<AsnLookupResponse, IpRefusal>> {
        Box::pin(async move { Ok(asn_lookup(&self.enrichment.load(), n)) })
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
