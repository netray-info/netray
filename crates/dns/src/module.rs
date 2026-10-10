use std::collections::HashSet;
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use governor::{Quota, RateLimiter};
use mhost::RecordType;
use mhost::lints::CheckResult as LintResult;
use mhost::resolver::{Lookups, MultiQuery};
use mhost::resources::{RData, Record};
use netray_common::enrichment::EnrichmentClient;
use netray_common::rate_limit::{KeyedLimiter, check_keyed_cost};
use netray_engine::{
    BoxFuture, Domain, EvidencePath, Facts, FactsProvider, Module, ResolveError, RunContext,
    SectionOutcome,
};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use serde_json::json;
use tokio::sync::{Semaphore, mpsc};

use crate::api::check::{
    CHECK_TOTAL_STEPS, CheckEvent, CheckRun, LintEvent, check_query, run_check,
};
use crate::api::query::{
    build_resolver_group, effective_server_specs, record_breaker_outcomes, target_keys_from_servers,
};
use crate::api::{BatchEvent, QUERY_SEMAPHORE_PERMITS};
use crate::circuit_breaker::{BreakerState, CircuitBreakerRegistry};
use crate::config::{Config, ConfigError, ModuleConfig};
use crate::error::ApiError;
use crate::parser::ParsedQuery;
use crate::security::QueryPolicy;

static CHECKS: LazyLock<Vec<CheckId>> = LazyLock::new(|| {
    [
        "caa",
        "cname_apex",
        "dnssec",
        "dnskey_algorithm",
        "dnssec_rollover",
        "https_svcb",
        "ns",
        "ns_lame",
        "ns_delegation",
        "ttl",
        "infrastructure",
    ]
    .into_iter()
    .map(|name| CheckId::parse(&format!("dns.{name}")).expect("static check id"))
    .collect()
});

pub(crate) fn dns_checks() -> &'static [CheckId] {
    CHECKS.as_slice()
}

/// The record types of the shared DNS facts.
const FACT_RECORD_TYPES: [RecordType; 6] = [
    RecordType::A,
    RecordType::AAAA,
    RecordType::MX,
    RecordType::CAA,
    RecordType::NS,
    RecordType::HTTPS,
];

/// The DNS check as an engine module: prism's check-route input policy (domain, servers,
/// `QueryPolicy::validate_for_check`), its per-target limit, circuit breakers and query
/// semaphore, then the check pipeline over the configured servers.
pub struct DnsModule {
    config: Config,
    servers: Vec<String>,
    circuit_breakers: Arc<CircuitBreakerRegistry>,
    query_semaphore: Arc<Semaphore>,
    ip_enrichment: Option<Arc<EnrichmentClient>>,
    per_target: KeyedLimiter<String>,
}

impl DnsModule {
    pub async fn new(mut config: ModuleConfig) -> Result<Self, ConfigError> {
        let servers = std::mem::take(&mut config.servers);
        let config = config.into_config()?;
        let per_target = RateLimiter::keyed(
            Quota::per_minute(
                NonZeroU32::new(config.limits.per_target_per_minute).expect("validated non-zero"),
            )
            .allow_burst(
                NonZeroU32::new(config.limits.per_target_burst).expect("validated non-zero"),
            ),
        );
        Ok(Self {
            circuit_breakers: Arc::new(CircuitBreakerRegistry::new(&config.circuit_breaker)),
            query_semaphore: Arc::new(Semaphore::new(QUERY_SEMAPHORE_PERMITS)),
            ip_enrichment: crate::ip_enrichment(&config.backends),
            per_target,
            servers,
            config,
        })
    }

    fn query(&self, domain: &str) -> Result<ParsedQuery, ApiError> {
        let parsed = check_query(domain, &self.servers)?;
        QueryPolicy::new(&self.config).validate_for_check(&parsed)?;
        Ok(parsed)
    }

    /// The per-target part of a route's cost: each server pays `cost` (the check's steps, or the
    /// record types of a query).
    fn charge_targets(&self, parsed: &ParsedQuery, cost: u32) -> Result<(), ApiError> {
        let servers = effective_server_specs(parsed, &self.config);
        let cost = NonZeroU32::new(cost).expect("non-zero cost");
        for key in target_keys_from_servers(&servers) {
            check_keyed_cost(&self.per_target, &key, cost, "per_target", "prism").map_err(|r| {
                ApiError::RateLimited {
                    retry_after_secs: r.retry_after_secs,
                    scope: r.scope,
                }
            })?;
        }
        Ok(())
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(self.config.limits.max_timeout_secs)
    }

    async fn measure(&self, ctx: &RunContext) -> Result<Vec<CheckEvent>, ApiError> {
        let parsed = self.query(ctx.domain.as_str())?;
        self.charge_targets(&parsed, CHECK_TOTAL_STEPS)?;
        let (group, breaker_keys) =
            build_resolver_group(&parsed, &self.config, self.timeout()).await?;
        let run = CheckRun {
            request_id: String::new(),
            domain: parsed.domain,
            resolvers: group.resolvers().to_vec(),
            breaker_keys,
            circuit_breakers: self.circuit_breakers.clone(),
            query_semaphore: self.query_semaphore.clone(),
            ip_enrichment: self.ip_enrichment.clone(),
        };
        let (tx, mut rx) = mpsc::channel::<CheckEvent>(32);
        let collect = async move {
            let mut events = Vec::new();
            while let Some(event) = rx.recv().await {
                events.push(event);
            }
            events
        };
        let ((), events) = tokio::join!(run_check(run, tx), collect);
        Ok(events)
    }

    /// The fact records through the resolver group, as prism's query route asks them: the
    /// per-target charge for the record types, a server whose circuit breaker is open skipped,
    /// each lookup under the query semaphore, its outcome recorded on the breakers.
    async fn fact_lookups(&self, domain: &str) -> Result<Lookups, ApiError> {
        let parsed = self.query(domain)?;
        self.charge_targets(&parsed, FACT_RECORD_TYPES.len() as u32)?;
        let (group, breaker_keys) =
            build_resolver_group(&parsed, &self.config, self.timeout()).await?;
        let query = MultiQuery::multi_record(parsed.domain.as_str(), FACT_RECORD_TYPES)
            .map_err(|e| ApiError::ResolverError(e.to_string()))?;
        let mut error = None;
        let mut lookups = Vec::new();
        for (resolver, key) in group.resolvers().iter().zip(&breaker_keys) {
            if let Err(BreakerState::Open) = self.circuit_breakers.check(key) {
                error = Some(format!("circuit breaker open for {key}, skipping"));
                continue;
            }
            let query = query.clone();
            lookups.push(async move {
                let _permit = self.query_semaphore.acquire().await;
                resolver.lookup(query).await
            });
        }
        // Per resolver: the group's own lookup future is not `Send`.
        let results = futures::future::join_all(lookups).await;
        let mut merged: Option<Lookups> = None;
        for result in results {
            match result {
                Ok(lookups) => {
                    record_breaker_outcomes(&self.circuit_breakers, &lookups);
                    merged = Some(match merged {
                        Some(m) => m.merge(lookups),
                        None => lookups,
                    });
                }
                Err(e) => error = Some(e.to_string()),
            }
        }
        match (merged, error) {
            (Some(lookups), _) => Ok(lookups),
            (None, Some(e)) => Err(ApiError::ResolverError(e)),
            (None, None) => Ok(Lookups::empty()),
        }
    }
}

impl Module for DnsModule {
    fn protocol(&self) -> Protocol {
        Protocol::Dns
    }

    fn checks(&self) -> &'static [CheckId] {
        dns_checks()
    }

    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }

    fn run<'a>(&'a self, ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async move {
            match tokio::time::timeout_at(ctx.deadline.into(), self.measure(ctx)).await {
                Ok(Ok(events)) => translate(&events),
                Ok(Err(e)) => SectionOutcome::Incomplete {
                    reason: e.to_string(),
                },
                Err(_) => SectionOutcome::TimedOut,
            }
        })
    }
}

impl FactsProvider for DnsModule {
    fn resolve<'a>(
        &'a self,
        ctx: &'a RunContext,
        domain: &'a Domain,
    ) -> BoxFuture<'a, Result<Facts, ResolveError>> {
        Box::pin(async move {
            let lookups =
                tokio::time::timeout_at(ctx.deadline.into(), self.fact_lookups(domain.as_str()))
                    .await
                    .map_err(|_| ResolveError("dns facts: deadline exceeded".to_string()))?
                    .map_err(|e| ResolveError(e.to_string()))?;
            Ok(facts_from_lookups(&lookups))
        })
    }
}

/// The A, AAAA, MX (exchange), CAA (presentation form), NS and HTTPS (presentation form)
/// records of `lookups`, each once, in lookup order. A field reads only the lookups of its own
/// record type, as the A and AAAA batches of `translate` do: an A or AAAA lookup keeps every
/// address it answers (the CNAME chain target included), the others keep the records owned by
/// the queried name, so the glue of an NS answer is not an address of the domain.
pub fn facts_from_lookups(lookups: &Lookups) -> Facts {
    Facts {
        a: first_seen(
            rdata_of(lookups, RecordType::A)
                .filter_map(RData::a)
                .copied(),
        ),
        aaaa: first_seen(
            rdata_of(lookups, RecordType::AAAA)
                .filter_map(RData::aaaa)
                .copied(),
        ),
        mx: first_seen(
            rdata_of(lookups, RecordType::MX)
                .filter_map(RData::mx)
                .map(|mx| mx.exchange().to_string()),
        ),
        caa: first_seen(
            rdata_of(lookups, RecordType::CAA)
                .filter_map(RData::caa)
                .map(|caa| {
                    let flags = if caa.issuer_critical() { 128 } else { 0 };
                    format!("{flags} {} \"{}\"", caa.tag(), caa.value())
                }),
        ),
        ns: first_seen(
            rdata_of(lookups, RecordType::NS)
                .filter_map(RData::ns)
                .map(ToString::to_string),
        ),
        https: first_seen(
            rdata_of(lookups, RecordType::HTTPS)
                .filter_map(RData::https)
                .map(|svcb| {
                    let mut rr = format!("{} {}", svcb.svc_priority(), svcb.target_name());
                    for param in svcb.svc_params() {
                        rr.push_str(&format!(
                            " {}={}",
                            param.key(),
                            param.value().trim_end_matches(',')
                        ));
                    }
                    rr
                }),
        ),
    }
}

/// The record data of the lookups that queried `record_type`, CNAME chain targets included.
/// Only the query type selects: an NS answer's glue sits in the NS lookup and never reaches
/// `a`/`aaaa`, and mhost already keeps authority/additional records only when they are owned
/// by the queried name. No owner comparison here: a live query name is not fully qualified
/// while wire record names are, and hickory's `Name` equality tells the two apart.
fn rdata_of(lookups: &Lookups, record_type: RecordType) -> impl Iterator<Item = &RData> {
    lookups
        .iter()
        .filter(move |lookup| lookup.query().record_type() == record_type)
        .flat_map(|lookup| lookup.records().into_iter().map(Record::data))
}

fn first_seen<T: PartialEq>(items: impl Iterator<Item = T>) -> Vec<T> {
    let mut out = Vec::new();
    for item in items {
        if !out.contains(&item) {
            out.push(item);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Translation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Pass,
    Warn,
    Fail,
    NotFound,
    /// Excluded from both earned and possible totals.
    Skip,
}

impl From<Verdict> for Status {
    fn from(verdict: Verdict) -> Self {
        match verdict {
            Verdict::Pass => Status::Pass,
            Verdict::Warn => Status::Warn,
            Verdict::Fail | Verdict::NotFound => Status::Fail,
            Verdict::Skip => Status::NotApplicable,
        }
    }
}

struct LintCheck {
    name: &'static str,
    verdict: Verdict,
    messages: Vec<String>,
}

/// Maps prism's check events onto `dns.<category>` checks (the email categories dropped), the
/// V1 headline and the addresses of the A/AAAA batches.
pub fn translate(events: &[CheckEvent]) -> SectionOutcome {
    let mut checks: Vec<LintCheck> = Vec::new();
    let mut resolved_ips: Vec<IpAddr> = Vec::new();
    let mut seen_ips: HashSet<IpAddr> = HashSet::new();

    for event in events {
        match event {
            CheckEvent::Batch(batch) => {
                collect_ips_from_batch(batch, &mut resolved_ips, &mut seen_ips);
            }
            CheckEvent::Lint(lint) => {
                if let Some(check) = parse_lint_event(lint) {
                    checks.push(check);
                }
            }
            _ => {}
        }
    }

    // If DNSKEY algorithm check is NotFound (no DNSKEY records → DNSSEC not deployed),
    // dnskey_algorithm and dnssec_rollover are N/A — skip them rather than penalising.
    let dnssec_absent = checks
        .iter()
        .any(|c| c.name == "dnskey_algorithm" && c.verdict == Verdict::NotFound);
    if dnssec_absent {
        for check in &mut checks {
            if check.name == "dnskey_algorithm" || check.name == "dnssec_rollover" {
                check.verdict = Verdict::Skip;
                check.messages.clear();
            }
        }
    }

    let headline = build_headline(&checks);

    let mut results = Vec::with_capacity(checks.len());
    for check in checks {
        let id = match CheckId::parse(&format!("dns.{}", check.name)) {
            Ok(id) => id,
            Err(e) => {
                return SectionOutcome::Incomplete {
                    reason: e.to_string(),
                };
            }
        };
        results.push(CheckResult {
            id,
            status: check.verdict.into(),
            findings: check.messages,
            evidence: vec![],
        });
    }

    let resolved_ips: Vec<String> = resolved_ips.iter().map(ToString::to_string).collect();
    SectionOutcome::Measured {
        checks: results,
        presentation: json!({ "headline": headline, "resolved_ips": resolved_ips }),
    }
}

/// The A or AAAA addresses of an A or AAAA batch, each once.
fn collect_ips_from_batch(
    batch: &BatchEvent,
    resolved_ips: &mut Vec<IpAddr>,
    seen: &mut HashSet<IpAddr>,
) {
    let ips: Vec<IpAddr> = match batch.record_type.as_str() {
        "A" => batch
            .lookups
            .a()
            .into_iter()
            .map(|ip| IpAddr::V4(*ip))
            .collect(),
        "AAAA" => batch
            .lookups
            .aaaa()
            .into_iter()
            .map(|ip| IpAddr::V6(*ip))
            .collect(),
        _ => return,
    };

    for ip in ips {
        if seen.insert(ip) {
            resolved_ips.push(ip);
        }
    }
}

/// A lint event as a single check carrying the worst verdict.
///
/// Email-security categories (spf, dmarc, mta_sts, tlsrpt, bimi, mx) belong to the email
/// module; DNS lint events for those categories are ignored here.
fn parse_lint_event(lint: &LintEvent) -> Option<LintCheck> {
    const EMAIL_CATEGORIES: &[&str] = &["spf", "dmarc", "mta_sts", "tlsrpt", "bimi", "mx"];
    if EMAIL_CATEGORIES.contains(&lint.category) {
        return None;
    }

    let mut worst = Verdict::Pass;
    let mut messages: Vec<String> = Vec::new();

    for (verdict, msg) in lint.results.iter().map(classify_lint_result) {
        if verdict_rank(verdict) > verdict_rank(worst) {
            worst = verdict;
        }
        match verdict {
            Verdict::Warn | Verdict::Fail | Verdict::NotFound => {
                messages.push(msg.unwrap_or_else(|| "Not found".to_string()));
            }
            _ => {}
        }
    }

    // If no results at all, treat as not found.
    if lint.results.is_empty() {
        worst = Verdict::NotFound;
        messages.push("Not found".to_string());
    }

    Some(LintCheck {
        name: lint.category,
        verdict: worst,
        messages,
    })
}

/// Rank verdicts so we can find the worst: higher rank = worse.
/// Order: Pass < NotFound < Warn < Fail.
fn verdict_rank(v: Verdict) -> u8 {
    match v {
        Verdict::Pass | Verdict::Skip => 0,
        Verdict::NotFound => 1,
        Verdict::Warn => 2,
        Verdict::Fail => 3,
    }
}

/// Map a single lint result to (verdict, optional message).
fn classify_lint_result(result: &LintResult) -> (Verdict, Option<String>) {
    match result {
        LintResult::Ok(_) => (Verdict::Pass, None),
        LintResult::Warning(msg) => (Verdict::Warn, Some(msg.clone())),
        LintResult::Failed(msg) => (Verdict::Fail, Some(msg.clone())),
        LintResult::NotFound() => (Verdict::NotFound, None),
    }
}

/// Build a summary headline from the collected checks.
fn build_headline(checks: &[LintCheck]) -> String {
    let keys = ["dnssec", "caa", "ns", "cname_apex"];
    let labels = ["DNSSEC", "CAA", "NS", "CNAME-apex"];

    let parts: Vec<String> = keys
        .iter()
        .zip(labels.iter())
        .map(|(key, label)| {
            let symbol = match checks.iter().find(|c| c.name == *key) {
                Some(c) => match c.verdict {
                    Verdict::Pass => "\u{2713}",
                    Verdict::Warn => "~",
                    Verdict::Fail => "\u{2717}",
                    Verdict::NotFound | Verdict::Skip => "\u{2013}",
                },
                None => "\u{2013}",
            };
            format!("{label} {symbol}")
        })
        .collect();

    parts.join("  ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(query_type: &str, records: serde_json::Value) -> serde_json::Value {
        json!({
            "name_server": "udp:192.0.2.53:53",
            "query": {"name": "example.com.", "record_type": query_type},
            "result": {"Response": {
                "records": records,
                "response_time": {"nanos": 0, "secs": 0},
                "valid_until": "2030-01-01T00:00:00Z"
            }}
        })
    }

    #[test]
    fn facts_skip_the_glue_of_an_ns_answer() {
        let lookups: Lookups = serde_json::from_value(json!({"lookups": [
            lookup("NS", json!([
                {"data": {"NS": "ns1.example.com."}, "name": "example.com.", "ttl": 300, "type": "NS"},
                {"data": {"A": "192.0.2.53"}, "name": "ns1.example.com.", "ttl": 300, "type": "A"}
            ])),
            lookup("A", json!([
                {"data": {"A": "192.0.2.10"}, "name": "example.com.", "ttl": 300, "type": "A"}
            ])),
        ]}))
        .expect("lookups decode");

        let facts = facts_from_lookups(&lookups);
        assert_eq!(
            facts.a,
            vec!["192.0.2.10".parse::<std::net::Ipv4Addr>().unwrap()]
        );
        assert_eq!(facts.ns, vec!["ns1.example.com.".to_string()]);
    }
}
