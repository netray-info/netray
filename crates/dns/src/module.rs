use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use governor::{Quota, RateLimiter};
use mhost::RecordType;
use mhost::lints::CheckResult as LintResult;
use mhost::resolver::lookup::LookupResult;
use mhost::resolver::{Lookup, Lookups, MultiQuery};
use mhost::resources::rdata::{CAA, DigestType, DnssecAlgorithm, SVCB};
use mhost::resources::{RData, Record};
use netray_common::rate_limit::{KeyedLimiter, check_keyed_cost};
use netray_engine::{
    BoxFuture, Domain, EvidencePath, Facts, FactsProvider, Module, ResolveError, RunContext,
    SectionOutcome,
};
use netray_model::evidence::{
    Block, BlockId, Record as EvRecord, Register, Shape, ShapeKind, Value as EvValue, ValueType,
};
use netray_model::{CheckId, CheckResult, Protocol, Status};
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
    ]
    .into_iter()
    .map(|name| CheckId::parse(&format!("dns.{name}")).expect("static check id"))
    .collect()
});

pub(crate) fn dns_checks() -> &'static [CheckId] {
    CHECKS.as_slice()
}

macro_rules! records_blocks {
    ($($t:literal),+ $(,)?) => {
        /// The DNS register: one records block `dns.<type>` per record type, keys `owner`,
        /// `type`, `ttl` and `data`.
        pub const REGISTER: Register = Register {
            blocks: &[$((concat!("dns.", $t), ShapeKind::Records)),+],
            keys: &[$(
                (concat!("dns.", $t), "owner", ValueType::Text),
                (concat!("dns.", $t), "type", ValueType::Text),
                (concat!("dns.", $t), "ttl", ValueType::Int),
                (concat!("dns.", $t), "data", ValueType::Text),
            )+],
        };

        const VOLATILE: &[EvidencePath] = &[$(EvidencePath(concat!("dns.", $t, "/ttl"))),+];
    };
}

records_blocks!(
    "a", "aaaa", "mx", "caa", "ns", "https", "cname", "dnskey", "ds", "nsec", "nsec3", "rrsig",
    "soa", "svcb", "txt",
);

/// The record types of the check run's base-name batches besides the facts types.
const MODULE_RECORD_TYPES: [RecordType; 9] = [
    RecordType::CNAME,
    RecordType::DNSKEY,
    RecordType::DS,
    RecordType::NSEC,
    RecordType::NSEC3,
    RecordType::RRSIG,
    RecordType::SOA,
    RecordType::SVCB,
    RecordType::TXT,
];

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
            ip_enrichment: None,
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
        VOLATILE
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

/// The A, AAAA, MX (exchange), CAA (zone-file form), NS and HTTPS (zone-file form)
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
                .map(caa_presentation),
        ),
        ns: first_seen(
            rdata_of(lookups, RecordType::NS)
                .filter_map(RData::ns)
                .map(ToString::to_string),
        ),
        https: first_seen(
            rdata_of(lookups, RecordType::HTTPS)
                .filter_map(RData::https)
                .map(svcb_presentation),
        ),
        no_address_definite: definite_negative(lookups, RecordType::A)
            && definite_negative(lookups, RecordType::AAAA),
        blocks: FACT_RECORD_TYPES
            .iter()
            .filter_map(|&record_type| {
                records_block(
                    record_type,
                    lookups
                        .iter()
                        .filter(|lookup| lookup.query().record_type() == record_type),
                )
            })
            .collect(),
    }
}

/// The id of the records block of `record_type`, `dns.<type>`.
fn block_id(record_type: &str) -> BlockId {
    BlockId::parse(&format!("dns.{}", record_type.to_lowercase())).expect("static block id")
}

/// Whether `lookup` is an answer: a response or a definite negative.
fn answered(lookup: &Lookup) -> bool {
    match lookup.result() {
        LookupResult::Response(_) => true,
        LookupResult::NxDomain(nx) => nx.is_definite(),
        LookupResult::Error(_) => false,
    }
}

/// The records block `dns.<type>` of `lookups`: their records, each (owner, type, data) once
/// with the highest TTL seen, in (owner, type, data) order. `None` when no lookup answered
/// (Rule M).
fn records_block<'a>(
    record_type: RecordType,
    lookups: impl IntoIterator<Item = &'a Lookup>,
) -> Option<Block> {
    let mut any_answered = false;
    let mut ttls: BTreeMap<(String, String, String), u32> = BTreeMap::new();
    for lookup in lookups.into_iter().filter(|lookup| answered(lookup)) {
        any_answered = true;
        for record in lookup.records() {
            let key = (
                record.name().to_string(),
                record.record_type().to_string(),
                rdata_text(record.data()),
            );
            let ttl = ttls.entry(key).or_default();
            *ttl = (*ttl).max(record.ttl());
        }
    }
    let records = ttls
        .into_iter()
        .map(|((owner, rtype, data), ttl)| EvRecord {
            owner: EvValue::Text(owner),
            rtype: EvValue::Text(rtype),
            ttl: EvValue::Int(i64::from(ttl)),
            data: EvValue::Text(data),
        })
        .collect();
    let title = record_type.to_string();
    any_answered.then(|| Block {
        id: block_id(&title),
        title,
        shape: Shape::Records(records),
    })
}

/// A CAA record in zone-file form, e.g. `0 issue "ca.example.com"`.
fn caa_presentation(caa: &CAA) -> String {
    let flags = if caa.issuer_critical() { 128 } else { 0 };
    format!("{flags} {} \"{}\"", caa.tag(), caa.value())
}

/// An SVCB or HTTPS record in zone-file form, e.g. `1 . alpn=h2`.
fn svcb_presentation(svcb: &SVCB) -> String {
    let mut rr = format!("{} {}", svcb.svc_priority(), svcb.target_name());
    for param in svcb.svc_params() {
        rr.push_str(&format!(
            " {}={}",
            param.key(),
            param.value().trim_end_matches(',')
        ));
    }
    rr
}

fn algorithm_number(algorithm: DnssecAlgorithm) -> u8 {
    match algorithm {
        DnssecAlgorithm::RsaMd5 => 1,
        DnssecAlgorithm::Dsa => 3,
        DnssecAlgorithm::RsaSha1 => 5,
        DnssecAlgorithm::RsaSha1Nsec3Sha1 => 7,
        DnssecAlgorithm::RsaSha256 => 8,
        DnssecAlgorithm::RsaSha512 => 10,
        DnssecAlgorithm::EcdsaP256Sha256 => 13,
        DnssecAlgorithm::EcdsaP384Sha384 => 14,
        DnssecAlgorithm::Ed25519 => 15,
        DnssecAlgorithm::Ed448 => 16,
        DnssecAlgorithm::Unassigned(n) => n,
    }
}

fn digest_type_number(digest_type: DigestType) -> u8 {
    match digest_type {
        DigestType::Sha1 => 1,
        DigestType::Sha256 => 2,
        DigestType::Sha384 => 4,
        DigestType::Unassigned(n) => n,
    }
}

/// The record data in zone-file form.
fn rdata_text(data: &RData) -> String {
    match data {
        RData::A(ip) => ip.to_string(),
        RData::AAAA(ip) => ip.to_string(),
        RData::ANAME(name) | RData::CNAME(name) | RData::NS(name) | RData::PTR(name) => {
            name.to_string()
        }
        RData::CAA(caa) => caa_presentation(caa),
        RData::HTTPS(svcb) | RData::SVCB(svcb) => svcb_presentation(svcb),
        RData::MX(mx) => format!("{} {}", mx.preference(), mx.exchange()),
        RData::SOA(soa) => format!(
            "{} {} {} {} {} {} {}",
            soa.mname(),
            soa.rname(),
            soa.serial(),
            soa.refresh(),
            soa.retry(),
            soa.expire(),
            soa.minimum()
        ),
        RData::TXT(txt) => txt
            .iter()
            .map(|s| format!("\"{}\"", String::from_utf8_lossy(s)))
            .collect::<Vec<_>>()
            .join(" "),
        RData::DNSKEY(key) => format!(
            "{} {} {} {}",
            key.flags(),
            key.protocol(),
            algorithm_number(key.algorithm()),
            key.public_key()
        ),
        RData::DS(ds) => format!(
            "{} {} {} {}",
            ds.key_tag(),
            algorithm_number(ds.algorithm()),
            digest_type_number(ds.digest_type()),
            ds.digest()
        ),
        RData::RRSIG(sig) => format!(
            "{} {} {} {} {} {} {} {} {}",
            sig.type_covered(),
            algorithm_number(sig.algorithm()),
            sig.labels(),
            sig.original_ttl(),
            sig.expiration(),
            sig.inception(),
            sig.key_tag(),
            sig.signer_name(),
            sig.signature()
        ),
        RData::NSEC(nsec) => std::iter::once(nsec.next_domain_name().to_string())
            .chain(nsec.types().iter().cloned())
            .collect::<Vec<_>>()
            .join(" "),
        RData::NSEC3(nsec3) => {
            let salt = if nsec3.salt().is_empty() {
                "-"
            } else {
                nsec3.salt()
            };
            [
                nsec3.hash_algorithm().to_string(),
                u8::from(nsec3.opt_out()).to_string(),
                nsec3.iterations().to_string(),
                salt.to_string(),
                nsec3.next_hashed_owner().to_string(),
            ]
            .into_iter()
            .chain(nsec3.types().iter().cloned())
            .collect::<Vec<_>>()
            .join(" ")
        }
        other => format!("{other:?}"),
    }
}

/// Whether `lookups` hold at least one lookup of `record_type` and every one is a definite
/// negative (NXDOMAIN, or NODATA that is not a referral).
fn definite_negative(lookups: &Lookups, record_type: RecordType) -> bool {
    let mut of_type = lookups
        .iter()
        .filter(|lookup| lookup.query().record_type() == record_type)
        .peekable();
    of_type.peek().is_some()
        && of_type
            .all(|lookup| matches!(lookup.result(), LookupResult::NxDomain(nx) if nx.is_definite()))
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
    /// The input of the check could not be obtained.
    Unmeasured,
}

impl From<Verdict> for Status {
    fn from(verdict: Verdict) -> Self {
        match verdict {
            Verdict::Pass => Status::Pass,
            Verdict::Warn => Status::Warn,
            Verdict::Fail | Verdict::NotFound => Status::Fail,
            Verdict::Skip => Status::NotApplicable,
            Verdict::Unmeasured => Status::Unmeasured,
        }
    }
}

struct LintCheck {
    name: &'static str,
    verdict: Verdict,
    messages: Vec<String>,
}

/// The record types whose batches each lint-over-batch check reads.
const CHECK_INPUTS: &[(&str, &[&str])] = &[
    ("caa", &["CAA"]),
    ("cname_apex", &["CNAME", "SOA"]),
    ("dnssec", &["DNSKEY", "DS"]),
    ("dnskey_algorithm", &["DNSKEY"]),
    ("dnssec_rollover", &["DNSKEY", "DS"]),
    ("https_svcb", &["HTTPS", "SVCB"]),
    ("ns", &["NS"]),
    ("ns_lame", &["NS"]),
    ("ns_delegation", &["NS"]),
];

/// Record types a check references as evidence beyond its `CHECK_INPUTS`; a failed batch of
/// one of them does not make the check unmeasured.
const EVIDENCE_ONLY_INPUTS: &[(&str, &[&str])] = &[("dnssec", &["RRSIG", "NSEC", "NSEC3"])];

/// Maps prism's check events onto `dns.<category>` checks (the email categories dropped) and the
/// records blocks of the answered base-name batches. A check references the blocks of its inputs that the section holds.
pub fn translate(events: &[CheckEvent]) -> SectionOutcome {
    let mut checks: Vec<LintCheck> = Vec::new();
    let mut batch_errors: Vec<(&str, Option<String>)> = Vec::new();
    let mut batches: Vec<&BatchEvent> = Vec::new();
    let mut unanswered: Vec<(&'static str, &[String], bool)> = Vec::new();
    let mut stream_timed_out = false;

    for event in events {
        match event {
            CheckEvent::Batch(batch) => {
                batch_errors.push((batch.record_type.as_str(), batch_error(batch)));
                batches.push(batch);
            }
            CheckEvent::Lint(lint) => {
                if let Some(check) = parse_lint_event(lint) {
                    checks.push(check);
                }
            }
            CheckEvent::Unanswered {
                category,
                silent,
                none_answered,
            } => unanswered.push((*category, silent.as_slice(), *none_answered)),
            CheckEvent::Error {
                code: "STREAM_TIMEOUT",
                ..
            } => stream_timed_out = true,
            _ => {}
        }
    }

    for check in &mut checks {
        let findings: Vec<String> = if check.name == "ttl" {
            let base: Vec<_> = batch_errors
                .iter()
                .filter(|(record_type, _)| !record_type.starts_with('_'))
                .collect();
            if !base.is_empty() && base.iter().all(|(_, error)| error.is_some()) {
                base.iter().filter_map(|(_, error)| error.clone()).collect()
            } else {
                Vec::new()
            }
        } else {
            let inputs = CHECK_INPUTS
                .iter()
                .find(|(name, _)| *name == check.name)
                .map_or(&[][..], |(_, inputs)| *inputs);
            batch_errors
                .iter()
                .filter(|(record_type, _)| inputs.contains(record_type))
                .filter_map(|(_, error)| error.clone())
                .collect()
        };
        if !findings.is_empty() {
            check.verdict = Verdict::Unmeasured;
            check.messages = findings;
        }
    }

    for (category, silent, none_answered) in unanswered {
        match checks.iter_mut().find(|c| c.name == category) {
            Some(check) if check.verdict == Verdict::Unmeasured => {}
            Some(check) if none_answered => {
                check.verdict = Verdict::Unmeasured;
                check.messages = silent.to_vec();
            }
            Some(check) => check.messages.extend(silent.iter().cloned()),
            None if none_answered => checks.push(LintCheck {
                name: category,
                verdict: Verdict::Unmeasured,
                messages: silent.to_vec(),
            }),
            None => {}
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
                check.messages.push("DNSSEC not deployed".to_string());
            }
        }
    }

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
    for id in dns_checks() {
        if !results.iter().any(|r| r.id == *id) {
            let (status, finding) = if stream_timed_out {
                (
                    Status::Unmeasured,
                    "DNS check stream timed out before this check was reported",
                )
            } else {
                (Status::NotApplicable, "not reported")
            };
            results.push(CheckResult {
                id: id.clone(),
                status,
                findings: vec![finding.to_string()],
                evidence: vec![],
            });
        }
    }

    let evidence: Vec<Block> = FACT_RECORD_TYPES
        .iter()
        .chain(&MODULE_RECORD_TYPES)
        .filter_map(|&record_type| {
            let label = record_type.to_string();
            records_block(
                record_type,
                batches
                    .iter()
                    .filter(|batch| batch.record_type == label)
                    .flat_map(|batch| batch.lookups.iter()),
            )
        })
        .collect();
    for result in &mut results {
        if result.status != Status::Unmeasured {
            result.evidence = check_evidence(result.id.name(), &evidence);
        }
    }

    SectionOutcome::Measured {
        checks: results,
        evidence,
    }
}

/// The blocks of `blocks` a check judged by: those of its inputs and evidence-only inputs;
/// `ttl` references every records block.
fn check_evidence(check: &str, blocks: &[Block]) -> Vec<BlockId> {
    if check == "ttl" {
        return blocks.iter().map(|b| b.id.clone()).collect();
    }
    let inputs = |table: &[(&str, &'static [&'static str])]| {
        table
            .iter()
            .find(|(name, _)| *name == check)
            .map_or(&[][..], |(_, inputs)| *inputs)
    };
    inputs(CHECK_INPUTS)
        .iter()
        .chain(inputs(EVIDENCE_ONLY_INPUTS))
        .map(|input| block_id(input))
        .filter(|id| blocks.iter().any(|b| b.id == *id))
        .collect()
}

/// The finding of a batch that measured nothing: no lookups, or every lookup an error or a
/// negative answer that is not definite. Names the record type, the causes and the resolvers.
fn batch_error(batch: &BatchEvent) -> Option<String> {
    let record_type = &batch.record_type;
    if batch.lookups.is_empty() {
        return Some(format!("{record_type} lookup failed: no resolver answered"));
    }
    let mut causes: Vec<(String, Vec<String>)> = Vec::new();
    for lookup in batch.lookups.iter() {
        let cause = match lookup.result() {
            LookupResult::Error(e) => e.to_string(),
            LookupResult::NxDomain(nx) if !nx.is_definite() => {
                if nx.is_referral() {
                    "referral".to_string()
                } else {
                    nx.response_code()
                        .map_or_else(|| "no response code".to_string(), |rc| rc.to_string())
                }
            }
            _ => return None,
        };
        let server = lookup.name_server().to_string();
        match causes.iter_mut().find(|(c, _)| *c == cause) {
            Some((_, servers)) => servers.push(server),
            None => causes.push((cause, vec![server])),
        }
    }
    let causes: Vec<String> = causes
        .into_iter()
        .map(|(cause, servers)| format!("{cause} from {}", servers.join(", ")))
        .collect();
    Some(format!(
        "{record_type} lookup failed: {}",
        causes.join("; ")
    ))
}

/// A lint event as a single check carrying the worst verdict.
///
/// Email-security categories (spf, dmarc, mta_sts, tlsrpt, bimi, mx) belong to the email
/// module, and `infrastructure` is not a catalogue check; DNS lint events for those categories
/// are ignored here.
fn parse_lint_event(lint: &LintEvent) -> Option<LintCheck> {
    const EMAIL_CATEGORIES: &[&str] = &[
        "spf",
        "dmarc",
        "mta_sts",
        "tlsrpt",
        "bimi",
        "mx",
        "infrastructure",
    ];
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
        Verdict::Pass | Verdict::Skip | Verdict::Unmeasured => 0,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

    #[test]
    fn facts_keep_records_for_an_unqualified_query_name_and_cname_targets() {
        // A live query name comes from lens without the trailing dot; wire records are fully
        // qualified. The MX answer follows a CNAME to another name.
        let unqualified = |query_type: &str, records: serde_json::Value| {
            let mut l = lookup(query_type, records);
            l["query"]["name"] = json!("shop.example.com");
            l
        };
        let lookups: Lookups = serde_json::from_value(json!({"lookups": [
            unqualified("MX", json!([
                {"data": {"CNAME": "shop.example.net."}, "name": "shop.example.com.", "ttl": 300, "type": "CNAME"},
                {"data": {"MX": {"preference": 10, "exchange": "mx.example.net."}}, "name": "shop.example.net.", "ttl": 300, "type": "MX"}
            ])),
            unqualified("NS", json!([
                {"data": {"NS": "ns1.example.com."}, "name": "shop.example.com.", "ttl": 300, "type": "NS"}
            ])),
        ]}))
        .expect("lookups decode");

        let facts = facts_from_lookups(&lookups);
        assert_eq!(
            facts.mx.len(),
            1,
            "the MX behind the CNAME is kept: {:?}",
            facts.mx
        );
        assert_eq!(facts.ns, vec!["ns1.example.com.".to_string()]);
    }
}
