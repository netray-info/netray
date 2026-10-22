//! Check endpoint: run DNS lookups and lint analysis for a domain, streaming results as SSE.
//!
//! - `POST /api/check` — accept a structured JSON body with a domain and optional servers.
//!
//! Phase 1 streams batch events (one per record type queried).
//! Phase 2 streams lint events (one per lint category, synchronous pure checks).
//! Phase 3 sends a done event with summary counts.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::http::Method;
use futures::stream::{FuturesUnordered, StreamExt};
use mhost::RecordType;
use mhost::lints::{
    CheckResult, check_caa, check_cname_apex, check_dmarc_records, check_dnssec,
    check_https_svcb_mode, check_mx_sync, check_ns_count, check_spf, check_ttl, is_dmarc,
};
use mhost::resolver::lookup::Uniquify;
use mhost::resolver::{Lookups, MultiQuery, Resolver};
use mhost::resources::rdata::{DnssecAlgorithm, TXT};
use netray_common::fetch::{
    AtLimit, ClientSettings, FetchError, FetchOptions, Resolve, SystemResolver, fetch,
};
use netray_common::target_policy::is_allowed_target;
use serde::{Deserialize, Serialize};
use tokio::sync::{Semaphore, mpsc};

use crate::api::query::{
    extract_ips_from_cached_events, parse_server_spec, record_breaker_outcomes,
};
use crate::api::{BatchEvent, STREAM_TIMEOUT_SECS};
use crate::circuit_breaker::{BreakerState, CircuitBreakerRegistry};
use crate::dns_raw::{self, NOT_PUBLIC, RawOutbound};
use crate::error::ApiError;
use crate::jobs::{DnsBackend, ResolverLookup};
use crate::parser::ParsedQuery;
use crate::result_cache::CachedEvent;
use netray_common::enrichment::{EnrichmentClient, IpInfo};

// ---------------------------------------------------------------------------
// Record types queried for the base domain (15 types)
// ---------------------------------------------------------------------------

pub(crate) const CHECK_RECORD_TYPES: [RecordType; 15] = [
    RecordType::A,
    RecordType::AAAA,
    RecordType::CAA,
    RecordType::CNAME,
    RecordType::DNSKEY,
    RecordType::DS,
    RecordType::HTTPS,
    RecordType::MX,
    RecordType::NS,
    RecordType::NSEC,
    RecordType::NSEC3,
    RecordType::RRSIG,
    RecordType::SOA,
    RecordType::SVCB,
    RecordType::TXT,
];

// Total SSE batch steps = 15 base types + 4 subdomain TXT lookups (DMARC, BIMI, MTA-STS, TLSRPT).
pub(crate) const CHECK_TOTAL_STEPS: u32 = 19;

// ---------------------------------------------------------------------------
// SSE event payloads
// ---------------------------------------------------------------------------

/// The lint categories the pipeline emits; a decoded [`LintEvent`] names one of them.
const LINT_CATEGORIES: &[&str] = &[
    "caa",
    "cname_apex",
    "dnssec",
    "dnskey_algorithm",
    "dnssec_rollover",
    "https_svcb",
    "mx",
    "ns",
    "ns_lame",
    "ns_delegation",
    "spf",
    "ttl",
    "dmarc",
    "bimi",
    "mta_sts",
    "tlsrpt",
    "infrastructure",
];

#[derive(Serialize)]
pub struct LintEvent {
    pub request_id: String,
    pub category: &'static str,
    pub results: Vec<CheckResult>,
}

impl<'de> Deserialize<'de> for LintEvent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        fn ignore_content<'de, D: serde::Deserializer<'de>>(d: D) -> Result<(), D::Error> {
            serde::de::IgnoredAny::deserialize(d).map(drop)
        }

        #[derive(Deserialize)]
        enum WireResult {
            #[serde(deserialize_with = "ignore_content")]
            NotFound,
            Ok(String),
            Warning(String),
            Failed(String),
        }

        #[derive(Deserialize)]
        struct Wire {
            request_id: String,
            category: String,
            results: Vec<WireResult>,
        }

        let wire = Wire::deserialize(deserializer)?;
        let category = LINT_CATEGORIES
            .iter()
            .copied()
            .find(|c| *c == wire.category)
            .ok_or_else(|| {
                serde::de::Error::custom(format!("unknown lint category `{}`", wire.category))
            })?;
        let results = wire
            .results
            .into_iter()
            .map(|r| match r {
                WireResult::NotFound => CheckResult::NotFound(),
                WireResult::Ok(m) => CheckResult::Ok(m),
                WireResult::Warning(m) => CheckResult::Warning(m),
                WireResult::Failed(m) => CheckResult::Failed(m),
            })
            .collect();
        Ok(LintEvent {
            request_id: wire.request_id,
            category,
            results,
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct CheckDoneEvent {
    pub request_id: String,
    pub duration_ms: u64,
    pub total_checks: u32,
    pub passed: u32,
    pub warnings: u32,
    pub failed: u32,
    pub not_found: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_key: Option<String>,
}

/// One event of the check pipeline, in stream order.
pub enum CheckEvent {
    Batch(BatchEvent),
    /// IP enrichment by address, before the lints.
    Enrichment(HashMap<String, IpInfo>),
    Lint(LintEvent),
    /// A non-fatal error; the stream goes on unless it is the stream timeout.
    Error {
        code: &'static str,
        message: String,
    },
    /// Findings of a check whose input did not answer, after the lints: the nameservers of an
    /// NS check (`ns_lame`, `ns_delegation`) that did not answer, or the failed ancestor lookup
    /// of a zone or CAA climb; not part of the `/api/check` stream.
    Unanswered {
        category: &'static str,
        silent: Vec<String>,
        none_answered: bool,
    },
    Done(CheckDoneEvent),
}

/// What one check run needs: the domain, the resolvers and their circuit breaker keys (one per
/// resolver), the breakers, the query semaphore and the optional IP enrichment.
pub struct CheckRun {
    pub request_id: String,
    pub domain: String,
    pub resolvers: Vec<Resolver>,
    pub breaker_keys: Vec<String>,
    pub circuit_breakers: Arc<CircuitBreakerRegistry>,
    pub query_semaphore: Arc<Semaphore>,
    pub ip_enrichment: Option<Arc<EnrichmentClient>>,
}

// ---------------------------------------------------------------------------
// POST handler
// ---------------------------------------------------------------------------

/// The check's query for `domain` and `servers`: the domain lowercased and refused when empty,
/// each server parsed. `record_types` is empty — `validate_for_check` skips the type count.
pub(crate) fn check_query(domain: &str, servers: &[String]) -> Result<ParsedQuery, ApiError> {
    let domain = domain.to_ascii_lowercase();
    if domain.is_empty() {
        return Err(ApiError::InvalidDomain("empty domain".into()));
    }

    let servers = servers
        .iter()
        .map(|name| parse_server_spec(name))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ParsedQuery {
        domain,
        record_types: Vec::new(),
        servers,
        transport: None,
        dnssec: false,
        short: false,
        recursive: true,
        truncated_servers: false,
        warnings: Vec::new(),
    })
}

/// A batch as prism's stream and result cache carry it: the internal [`ZONE_SOURCE`] marker is
/// left out, so the V1 wire is unchanged.
fn wire_batch(batch: &BatchEvent) -> serde_json::Value {
    let mut v = serde_json::to_value(batch).unwrap_or(serde_json::Value::Null);
    if batch.source.as_deref() == Some(ZONE_SOURCE)
        && let Some(map) = v.as_object_mut()
    {
        map.remove("source");
    }
    v
}

fn push_cached(cached: &mut Vec<CachedEvent>, event_type: &str, payload: &impl Serialize) {
    if let Ok(data) = serde_json::to_value(payload) {
        cached.push(CachedEvent {
            event_type: event_type.to_owned(),
            data,
        });
    }
}

// ---------------------------------------------------------------------------
// Check pipeline
// ---------------------------------------------------------------------------

/// The check: the lookups of every record type and subdomain, the IP enrichment, the NS and
/// MTA-STS checks and the lints, sent on `tx` in stream order and closed by a `done` event
/// (no `done` after the stream deadline). Stops when `tx` is closed.
pub async fn run_check(run: CheckRun, tx: mpsc::Sender<CheckEvent>) {
    run_check_on(run, tx, DnsBackend::production(), Outbound::production()).await
}

/// [`run_check`] over the given lookup and raw-query backend and MTA-STS fetch settings: the
/// seam tests drive without network.
pub(crate) async fn run_check_on(
    run: CheckRun,
    tx: mpsc::Sender<CheckEvent>,
    backend: DnsBackend,
    outbound: Outbound,
) {
    let CheckRun {
        request_id: rid,
        domain,
        resolvers,
        breaker_keys,
        circuit_breakers,
        query_semaphore,
        ip_enrichment: enrichment_svc,
    } = run;
    let DnsBackend {
        lookup,
        raw: raw_outbound,
    } = backend;

    let _active = ActiveCheck::new();
    let start = Instant::now();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(STREAM_TIMEOUT_SECS);
    let mut batch_events: Vec<CachedEvent> = Vec::new();

    // ------------------------------------------------------------------
    // Phase 1: DNS lookups — all 16 types in parallel (FuturesUnordered)
    // ------------------------------------------------------------------
    let dmarc_domain = format!("_dmarc.{domain}");
    let bimi_domain = format!("default._bimi.{domain}");
    let mta_sts_domain = format!("_mta-sts.{domain}");
    let tlsrpt_domain = format!("_smtp._tls.{domain}");

    #[derive(Clone, Copy)]
    enum LookupKind {
        Base,
        Dmarc,
        Bimi,
        MtaSts,
        Tlsrpt,
    }

    type LookupFut =
        std::pin::Pin<Box<dyn std::future::Future<Output = (String, Lookups, LookupKind)> + Send>>;
    let futs: FuturesUnordered<LookupFut> = FuturesUnordered::new();
    for rt in CHECK_RECORD_TYPES.iter() {
        let rt = *rt;
        let domain = domain.clone();
        let resolvers = resolvers.clone();
        let breaker_keys = breaker_keys.clone();
        let circuit_breakers = Arc::clone(&circuit_breakers);
        let tx_err = tx.clone();
        let semaphore = Arc::clone(&query_semaphore);
        let lookup = Arc::clone(&lookup);
        futs.push(Box::pin(async move {
            let lookups = fan_out_lookup(
                &lookup,
                &domain,
                rt,
                &resolvers,
                &breaker_keys,
                &circuit_breakers,
                &tx_err,
                &semaphore,
            )
            .await;
            (rt.to_string(), lookups, LookupKind::Base)
        }));
    }
    for (sub_domain, label, kind) in [
        (dmarc_domain, "_dmarc", LookupKind::Dmarc),
        (bimi_domain, "_bimi", LookupKind::Bimi),
        (mta_sts_domain, "_mta-sts", LookupKind::MtaSts),
        (tlsrpt_domain, "_tlsrpt", LookupKind::Tlsrpt),
    ] {
        let resolvers = resolvers.clone();
        let breaker_keys = breaker_keys.clone();
        let circuit_breakers = Arc::clone(&circuit_breakers);
        let tx_err = tx.clone();
        let semaphore = Arc::clone(&query_semaphore);
        let lookup = Arc::clone(&lookup);
        futs.push(Box::pin(async move {
            let lookups = fan_out_lookup(
                &lookup,
                &sub_domain,
                RecordType::TXT,
                &resolvers,
                &breaker_keys,
                &circuit_breakers,
                &tx_err,
                &semaphore,
            )
            .await;
            (label.to_string(), lookups, kind)
        }));
    }

    tokio::pin!(futs);
    let mut all_lookups = Lookups::empty();
    let mut dmarc_lookups = Lookups::empty();
    let mut bimi_lookups = Lookups::empty();
    let mut mta_sts_lookups = Lookups::empty();
    let mut tlsrpt_lookups = Lookups::empty();
    let mut completed: u32 = 0;

    loop {
        tokio::select! {
            maybe = futs.next() => {
                match maybe {
                    None => break,
                    Some((label, lookups, kind)) => {
                        completed += 1;
                        let cache = enrichment_svc.is_some().then_some(&mut batch_events);
                        if !send_batch(&tx, &rid, &label, &lookups, completed, cache, None).await {
                            metrics::counter!("netray_dns_queries_total", "operation" => "check", "status" => "error").increment(1);
                            return;
                        }
                        match kind {
                            LookupKind::Dmarc => dmarc_lookups = lookups,
                            LookupKind::Bimi => bimi_lookups = lookups,
                            LookupKind::MtaSts => mta_sts_lookups = lookups,
                            LookupKind::Tlsrpt => tlsrpt_lookups = lookups,
                            LookupKind::Base => all_lookups = all_lookups.merge(lookups),
                        }
                    }
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                let _ = tx
                    .send(CheckEvent::Error {
                        code: "STREAM_TIMEOUT",
                        message: "stream deadline exceeded".to_owned(),
                    })
                    .await;
                metrics::counter!("netray_dns_queries_total", "operation" => "check", "status" => "error").increment(1);
                return;
            }
        }
    }

    // ------------------------------------------------------------------
    // Phase 1.25: the zone of the queried name and the CAA climb
    // ------------------------------------------------------------------
    let scope = LookupScope {
        lookup: &lookup,
        resolvers: &resolvers,
        breaker_keys: &breaker_keys,
        circuit_breakers: &circuit_breakers,
        tx: &tx,
        semaphore: &query_semaphore,
    };
    let zone = read_zone(&scope, &domain, &all_lookups, deadline).await;
    for (rt, lookups) in zone.batches {
        completed += 1;
        let cache = enrichment_svc.is_some().then_some(&mut batch_events);
        if !send_batch(
            &tx,
            &rid,
            &rt.to_string(),
            &lookups,
            completed,
            cache,
            Some(ZONE_SOURCE),
        )
        .await
        {
            metrics::counter!("netray_dns_queries_total", "operation" => "check", "status" => "error")
                .increment(1);
            return;
        }
        let others: Vec<_> = all_lookups
            .into_iter()
            .filter(|l| l.query().record_type() != rt)
            .collect();
        all_lookups = Lookups::from(others).merge(lookups);
    }

    // Extract DMARC TXT strings for the dmarc lint.
    let dmarc_txt_vec = dmarc_lookups.txt();
    let unique_dmarc = dmarc_txt_vec.unique();
    let dmarc_txts: Vec<String> = unique_dmarc
        .iter()
        .map(TXT::as_string)
        .filter(|s| is_dmarc(s))
        .collect();

    // ------------------------------------------------------------------
    // Phase 1.5: IP enrichment (non-blocking, before lint)
    // ------------------------------------------------------------------
    let enrichment_map = if let Some(ref svc) = enrichment_svc {
        let ips = extract_ips_from_cached_events(&batch_events);
        let map = svc.lookup_batch(&ips, Some(&rid)).await;
        if !map.is_empty() {
            let enrichments = map
                .iter()
                .map(|(ip, info)| (ip.to_string(), info.clone()))
                .collect();
            let _ = tx.send(CheckEvent::Enrichment(enrichments)).await;
        }
        map
    } else {
        HashMap::new()
    };

    // ------------------------------------------------------------------
    // Phase 1.75: Async NS checks (lame delegation + delegation consistency)
    //             + MTA-STS policy file fetch
    // ------------------------------------------------------------------
    let query_timeout = Duration::from_secs(3);
    let unique = unique_records(&all_lookups);
    let (lame, delegation, mta_sts_results, ttl_batches) = tokio::join!(
        check_ns_lame_delegation(&unique, &zone.judged, query_timeout, &raw_outbound),
        check_ns_delegation_consistency(&unique, &zone.judged, query_timeout, &raw_outbound),
        check_mta_sts(&mta_sts_lookups, &domain, &unique, &outbound),
        async {
            tokio::time::timeout_at(
                deadline,
                authoritative_ttls(&unique, &zone.judged, query_timeout, &raw_outbound),
            )
            .await
            .unwrap_or_default()
        },
    );
    for (rt, lookups) in ttl_batches {
        completed += 1;
        let cache = enrichment_svc.is_some().then_some(&mut batch_events);
        if !send_batch(&tx, &rid, &rt.to_string(), &lookups, completed, cache, None).await {
            metrics::counter!("netray_dns_queries_total", "operation" => "check", "status" => "error")
                .increment(1);
            return;
        }
        all_lookups = all_lookups.merge(lookups);
    }

    // ------------------------------------------------------------------
    // Phase 2: Lint checks (synchronous, pure)
    // ------------------------------------------------------------------
    let mut lint_checks: Vec<(&'static str, Vec<CheckResult>)> = Vec::new();
    for (category, results) in lint_lookups(&all_lookups) {
        lint_checks.push((category, results));
        if category == "ns" {
            lint_checks.push(("ns_lame", lame.results.clone()));
            lint_checks.push(("ns_delegation", delegation.results.clone()));
        }
    }
    lint_checks.push(("dmarc", check_dmarc_records(&dmarc_txts)));
    let bimi_results = check_bimi(&bimi_lookups);
    let tlsrpt_results = check_tlsrpt(&tlsrpt_lookups, &mta_sts_lookups);
    lint_checks.push(("bimi", bimi_results));
    lint_checks.push(("mta_sts", mta_sts_results));
    lint_checks.push(("tlsrpt", tlsrpt_results));
    if !enrichment_map.is_empty() {
        lint_checks.push(("infrastructure", check_infrastructure(&enrichment_map)));
    }

    let mut total_checks: u32 = 0;
    let mut passed: u32 = 0;
    let mut lint_warnings: u32 = 0;
    let mut failed: u32 = 0;
    let mut not_found: u32 = 0;

    for (category, results) in lint_checks {
        let results = unique_lines(results);
        for r in &results {
            total_checks += 1;
            match r {
                CheckResult::Ok(_) => passed += 1,
                CheckResult::Warning(_) => lint_warnings += 1,
                CheckResult::Failed(_) => failed += 1,
                CheckResult::NotFound() => not_found += 1,
            }
        }

        let lint_event = LintEvent {
            request_id: rid.clone(),
            category,
            results,
        };
        if tx.send(CheckEvent::Lint(lint_event)).await.is_err() {
            metrics::counter!("netray_dns_queries_total", "operation" => "check", "status" => "error")
                .increment(1);
            return;
        }
    }
    for event in zone
        .unmeasured
        .into_iter()
        .chain(unanswered_events(&lame, &delegation))
    {
        if tx.send(event).await.is_err() {
            metrics::counter!("netray_dns_queries_total", "operation" => "check", "status" => "error")
                .increment(1);
            return;
        }
    }

    // ------------------------------------------------------------------
    // Phase 3: Done event
    // ------------------------------------------------------------------
    let elapsed = start.elapsed();
    tracing::info!(
        request_id = %rid,
        domain = %domain,
        duration_ms = elapsed.as_millis(),
        "check completed"
    );
    let done = CheckDoneEvent {
        request_id: rid,
        duration_ms: elapsed.as_millis() as u64,
        total_checks,
        passed,
        warnings: lint_warnings,
        failed,
        not_found,
        cache_key: None,
    };
    let _ = tx.send(CheckEvent::Done(done)).await;

    metrics::counter!("netray_dns_queries_total", "operation" => "check", "status" => "ok")
        .increment(1);
    metrics::histogram!("netray_dns_check_duration_seconds").record(elapsed.as_secs_f64());
}

/// Counts a running check in `netray_dns_active_checks` until dropped, so a check cancelled mid-way
/// (stream deadline, closed client) does not leak its increment.
struct ActiveCheck;

impl ActiveCheck {
    fn new() -> Self {
        metrics::gauge!("netray_dns_active_checks").increment(1.0);
        Self
    }
}

impl Drop for ActiveCheck {
    fn drop(&mut self) {
        metrics::gauge!("netray_dns_active_checks").decrement(1.0);
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Fan out a single-type DNS query across all resolvers concurrently,
/// applying circuit breaker pre-checks. Returns merged lookups.
// One argument per seam and shared limiter, as `dispatch_mechanism` in netray-email does; a
// struct for one private call site would only move the list.
#[allow(clippy::too_many_arguments)]
async fn fan_out_lookup(
    lookup: &Arc<dyn ResolverLookup>,
    domain: &str,
    rt: RecordType,
    resolvers: &[Resolver],
    breaker_keys: &[String],
    circuit_breakers: &Arc<CircuitBreakerRegistry>,
    tx: &mpsc::Sender<CheckEvent>,
    semaphore: &Arc<Semaphore>,
) -> Lookups {
    let query = match MultiQuery::single(domain, rt) {
        Ok(q) => q,
        Err(e) => {
            tracing::warn!(domain = %domain, record_type = %rt, error = %e, "check query build failed");
            let _ = tx
                .send(CheckEvent::Error {
                    code: "RESOLVER_ERROR",
                    message: e.to_string(),
                })
                .await;
            return Lookups::empty();
        }
    };

    let mut handles = Vec::with_capacity(resolvers.len());
    for (idx, resolver) in resolvers.iter().enumerate() {
        let breaker_key = &breaker_keys[idx];
        if let Err(BreakerState::Open) = circuit_breakers.check(breaker_key) {
            tracing::warn!(domain = %domain, record_type = %rt, provider = %breaker_key, "circuit breaker open, skipping provider");
            let _ = tx
                .send(CheckEvent::Error {
                    code: "PROVIDER_DEGRADED",
                    message: format!("circuit breaker open for {breaker_key} ({rt}), skipping"),
                })
                .await;
            continue;
        }
        let r = resolver.clone();
        let q = query.clone();
        let sem = Arc::clone(semaphore);
        let lookup = Arc::clone(lookup);
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await;
            lookup.lookup(&r, q).await
        }));
    }

    let mut merged = Lookups::empty();
    for handle in handles {
        match handle.await {
            Ok(Ok(lookups)) => {
                record_breaker_outcomes(circuit_breakers, &lookups);
                merged = merged.merge(lookups);
            }
            Ok(Err(e)) => {
                tracing::warn!(domain = %domain, record_type = %rt, error = %e, "resolver lookup failed");
                let _ = tx
                    .send(CheckEvent::Error {
                        code: "RESOLVER_ERROR",
                        message: e.to_string(),
                    })
                    .await;
            }
            Err(e) => {
                tracing::error!(domain = %domain, record_type = %rt, error = %e, "resolver task panicked");
                let _ = tx
                    .send(CheckEvent::Error {
                        code: "INTERNAL_ERROR",
                        message: e.to_string(),
                    })
                    .await;
            }
        }
    }

    merged
}

/// Sends `lookups` as the batch of `record_type`, numbered `completed`, cached for the IP
/// enrichment when `cache` is given. False when `tx` is closed.
async fn send_batch(
    tx: &mpsc::Sender<CheckEvent>,
    rid: &str,
    record_type: &str,
    lookups: &Lookups,
    completed: u32,
    cache: Option<&mut Vec<CachedEvent>>,
    source: Option<&str>,
) -> bool {
    let batch = BatchEvent {
        request_id: rid.to_owned(),
        record_type: record_type.to_owned(),
        lookups: lookups.clone(),
        completed,
        total: CHECK_TOTAL_STEPS.max(completed),
        transport: None,
        source: source.map(str::to_owned),
    };
    if let Some(cache) = cache {
        push_cached(cache, "batch", &wire_batch(&batch));
    }
    tx.send(CheckEvent::Batch(batch)).await.is_ok()
}

/// The lookup seam, resolvers and limits every check lookup goes through.
struct LookupScope<'a> {
    lookup: &'a Arc<dyn ResolverLookup>,
    resolvers: &'a [Resolver],
    breaker_keys: &'a [String],
    circuit_breakers: &'a Arc<CircuitBreakerRegistry>,
    tx: &'a mpsc::Sender<CheckEvent>,
    semaphore: &'a Arc<Semaphore>,
}

impl LookupScope<'_> {
    /// [`fan_out_lookup`] of `rt` at `name`.
    async fn of(&self, name: &str, rt: RecordType) -> Lookups {
        fan_out_lookup(
            self.lookup,
            name,
            rt,
            self.resolvers,
            self.breaker_keys,
            self.circuit_breakers,
            self.tx,
            self.semaphore,
        )
        .await
    }
}

/// The checks that judge the zone of the queried name.
const ZONE_CHECKS: [&str; 6] = [
    "ns",
    "ns_lame",
    "ns_delegation",
    "dnssec",
    "dnskey_algorithm",
    "dnssec_rollover",
];

/// What the zone of the queried name adds to a check: the name the lame-delegation and
/// delegation-consistency checks ask for, the batches sent that replace the name's lookups of
/// their type in the linted lookups (the zone's NS, DNSKEY and DS, an ancestor's CAA), and an
/// `Unanswered` event per check whose zone or CAA climb ended on a failed lookup.
struct ZoneReading {
    judged: String,
    batches: Vec<(RecordType, Lookups)>,
    unmeasured: Vec<CheckEvent>,
}

/// The finding of a zone reading cut off by the stream deadline.
const ZONE_DEADLINE: &str = "zone lookup exceeded the stream deadline";

/// The finding of a CAA climb cut off by the stream deadline.
const CAA_DEADLINE: &str = "CAA lookup of the name's ancestors exceeded the stream deadline";

/// The `source` of a batch read at the zone or an ancestor instead of the queried name: it replaces
/// the name's batch of its type, whose failure then decides nothing (`module::translate`).
pub(crate) const ZONE_SOURCE: &str = "zone";

/// Most ancestors a zone or CAA climb asks.
const MAX_ANCESTORS: usize = 8;

/// The zone of `domain` and its CAA climb, read after the lookups at `domain` itself and bounded
/// by `deadline`. An apex owning its SOA, a name whose SOA lookup got no definite answer and a
/// name holding CAA records add no lookup.
async fn read_zone(
    scope: &LookupScope<'_>,
    domain: &str,
    lookups: &Lookups,
    deadline: tokio::time::Instant,
) -> ZoneReading {
    let zone = async {
        if owns(lookups, domain, RecordType::SOA)
            || lookup_failure(lookups, RecordType::SOA, domain).is_some()
        {
            return Ok(None);
        }
        let Some((zone, _)) = climb(scope, domain, RecordType::SOA, |l, ancestor| {
            owns(l, ancestor, RecordType::SOA)
        })
        .await?
        else {
            return Ok(None);
        };
        let (ns, dnskey, ds) = tokio::join!(
            scope.of(&zone, RecordType::NS),
            scope.of(&zone, RecordType::DNSKEY),
            scope.of(&zone, RecordType::DS),
        );
        Ok(Some((zone, ns, dnskey, ds)))
    };
    let caa = async {
        if !lookups.records_by_type(RecordType::CAA).is_empty()
            || lookup_failure(lookups, RecordType::CAA, domain).is_some()
        {
            return Ok(None);
        }
        climb(scope, domain, RecordType::CAA, |l, _| {
            !l.records_by_type(RecordType::CAA).is_empty()
        })
        .await
    };
    let (zone, caa) = tokio::join!(
        tokio::time::timeout_at(deadline, zone),
        tokio::time::timeout_at(deadline, caa),
    );

    let mut reading = ZoneReading {
        judged: domain.to_owned(),
        batches: Vec::new(),
        unmeasured: Vec::new(),
    };
    match zone.unwrap_or_else(|_| Err(ZONE_DEADLINE.to_owned())) {
        Ok(None) => {}
        Ok(Some((zone, ns, dnskey, ds))) => {
            reading.batches.extend([
                (RecordType::NS, ns),
                (RecordType::DNSKEY, dnskey),
                (RecordType::DS, ds),
            ]);
            reading.judged = zone;
        }
        Err(finding) => reading
            .unmeasured
            .extend(ZONE_CHECKS.iter().map(|check| unmeasured(check, &finding))),
    }
    // A timed-out future was climbing: one that needed no climb is ready at its first poll.
    match caa.unwrap_or_else(|_| Err(CAA_DEADLINE.to_owned())) {
        Ok(None) => {}
        Ok(Some((_, lookups))) => reading.batches.push((RecordType::CAA, lookups)),
        Err(finding) => reading.unmeasured.push(unmeasured("caa", &finding)),
    }
    reading
}

/// The closest ancestor of `domain` whose `rt` lookup `found` accepts, with that lookup; `None`
/// when none of [`ancestors`] does. An ancestor lookup with no definite answer before then ends
/// the climb with its finding.
async fn climb(
    scope: &LookupScope<'_>,
    domain: &str,
    rt: RecordType,
    found: impl Fn(&Lookups, &str) -> bool,
) -> Result<Option<(String, Lookups)>, String> {
    for ancestor in ancestors(domain) {
        let lookups = scope.of(&ancestor, rt).await;
        if found(&lookups, &ancestor) {
            return Ok(Some((ancestor, lookups)));
        }
        if let Some(finding) = lookup_failure(&lookups, rt, &ancestor) {
            return Err(finding);
        }
    }
    Ok(None)
}

/// The ancestors of `domain`, closest first, at most [`MAX_ANCESTORS`]: never above its
/// registrable domain, or a single-label name when the public suffix list gives none.
fn ancestors(domain: &str) -> Vec<String> {
    let domain = domain.trim_end_matches('.').to_ascii_lowercase();
    let floor = psl::domain_str(&domain).map_or(2, |d| d.split('.').count());
    let labels: Vec<&str> = domain.split('.').collect();
    (1..=labels.len().saturating_sub(floor))
        .take(MAX_ANCESTORS)
        .map(|first| labels[first..].join("."))
        .collect()
}

/// Whether `lookups` hold an `rt` record owned by `name`, compared as A-labels.
fn owns(lookups: &Lookups, name: &str, rt: RecordType) -> bool {
    let name = name.trim_end_matches('.');
    lookups.records_by_type(rt).iter().any(|record| {
        record
            .name()
            .to_ascii()
            .trim_end_matches('.')
            .eq_ignore_ascii_case(name)
    })
}

/// The finding of the `rt` lookups of `name` in `lookups` when none got a definite answer.
fn lookup_failure(lookups: &Lookups, rt: RecordType, name: &str) -> Option<String> {
    let of_type = lookups.iter().filter(|l| l.query().record_type() == rt);
    crate::module::lookup_error(&rt.to_string(), of_type, Some(name))
}

/// The event that makes `category` unmeasured with `finding`.
fn unmeasured(category: &'static str, finding: &str) -> CheckEvent {
    CheckEvent::Unanswered {
        category,
        silent: vec![finding.to_owned()],
        none_answered: true,
    }
}

/// Maps a lint result onto the shared model status (a function: `CheckResult` is foreign).
pub fn lint_status(r: &CheckResult) -> netray_model::Status {
    match r {
        CheckResult::NotFound() => netray_model::Status::Fail,
        CheckResult::Ok(_) => netray_model::Status::Pass,
        CheckResult::Warning(_) => netray_model::Status::Warn,
        CheckResult::Failed(_) => netray_model::Status::Fail,
    }
}

/// Drop repeated identical results; the first occurrence and the order are kept.
pub fn unique_lines(results: Vec<CheckResult>) -> Vec<CheckResult> {
    let mut out: Vec<CheckResult> = Vec::with_capacity(results.len());
    for r in results {
        if !out.contains(&r) {
            out.push(r);
        }
    }
    out
}

/// One record per name, type and data across every lookup of every resolver.
///
/// Of duplicates the copy with the highest TTL is kept, at the position of the first
/// occurrence (mhost's `Record` equality ignores the TTL, and `check_ttl` keeps the highest
/// TTL the same way). mhost's `Record` constructors are crate-private, so a lookup that has to
/// change is rewritten through its public serde form. That happens per lookup: a lookup that
/// needs no change is kept as is, a lookup whose records all repeat earlier ones is dropped,
/// and a lookup that does not survive the serde round trip (e.g. a name with an escaped
/// `\000` label) is kept as is, so it can only affect its own deduplication.
pub(crate) fn unique_records(lookups: &Lookups) -> Lookups {
    use mhost::resolver::Lookup;
    use serde_json::Value;
    use std::collections::HashSet;

    fn record_key(rec: &Value) -> String {
        format!("{}|{}|{}", rec["name"], rec["type"], rec["data"])
    }

    fn records_of(lookup: &mut Value) -> Option<&mut Vec<Value>> {
        lookup
            .pointer_mut("/result/Response/records")
            .and_then(Value::as_array_mut)
    }

    let values: Vec<Option<Value>> = lookups
        .iter()
        .map(|l| serde_json::to_value(l).ok())
        .collect();

    let mut max_ttl: HashMap<String, u64> = HashMap::new();
    for value in values.iter().flatten() {
        let mut value = value.clone();
        if let Some(records) = records_of(&mut value) {
            for rec in records.iter() {
                let ttl = rec["ttl"].as_u64().unwrap_or(0);
                let entry = max_ttl.entry(record_key(rec)).or_insert(ttl);
                *entry = (*entry).max(ttl);
            }
        }
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<Lookup> = Vec::with_capacity(values.len());
    for (lookup, value) in lookups.iter().zip(values) {
        let Some(mut value) = value else {
            out.push(lookup.clone());
            continue;
        };
        let Some(records) = records_of(&mut value) else {
            out.push(lookup.clone());
            continue;
        };
        if records.is_empty() {
            out.push(lookup.clone());
            continue;
        }

        let before = records.len();
        let mut changed = false;
        records.retain_mut(|rec| {
            let key = record_key(rec);
            if !seen.insert(key.clone()) {
                return false;
            }
            if let Some(&ttl) = max_ttl.get(&key)
                && rec["ttl"].as_u64() != Some(ttl)
            {
                rec["ttl"] = Value::from(ttl);
                changed = true;
            }
            true
        });

        if records.is_empty() {
            continue;
        }
        if !changed && records.len() == before {
            out.push(lookup.clone());
            continue;
        }
        match serde_json::from_value::<Lookup>(value) {
            Ok(rewritten) => out.push(rewritten),
            Err(err) => {
                tracing::debug!(%err, "lookup does not round-trip; keeping it unchanged");
                out.push(lookup.clone());
            }
        }
    }

    Lookups::new(out)
}

/// The TTL below which `check_ttl` calls a record low.
const LOW_TTL: u32 = 60;

/// Two names compared as A-labels: hickory serialises and displays `xn--` labels as Unicode.
fn same_name(a: &str, b: &str) -> bool {
    a_label(a).eq_ignore_ascii_case(&a_label(b))
}

/// `name` as an A-label without the trailing dot; unparsable names stay as given.
fn a_label(name: &str) -> String {
    let ascii = hickory_proto::rr::Name::from_utf8(name)
        .map(|n| n.to_ascii())
        .unwrap_or_else(|_| name.to_owned());
    ascii.trim_end_matches('.').to_owned()
}

/// Whether `name` is `zone` or below it.
fn in_zone(name: &str, zone: &str) -> bool {
    let (name, zone) = (
        a_label(name).to_ascii_lowercase(),
        a_label(zone).to_ascii_lowercase(),
    );
    name == zone || name.ends_with(&format!(".{zone}"))
}

/// One batch per (name, type) at or below `zone` whose highest TTL in `unique` is below
/// [`LOW_TTL`]: the pair's records with the highest TTL an authoritative (AA) answer of the
/// zone's nameservers carries for that name and type, from the nameserver that served it. A pair
/// no nameserver answers authoritatively, or owned outside the zone (a CNAME target elsewhere),
/// gets no batch.
async fn authoritative_ttls(
    unique: &Lookups,
    zone: &str,
    timeout: Duration,
    raw: &RawOutbound,
) -> Vec<(RecordType, Lookups)> {
    let mut pairs: Vec<(String, RecordType)> = Vec::new();
    for record in unique.records() {
        let pair = (record.name().to_ascii(), record.record_type());
        // An RRSIG carries the TTL of the RRset it covers (RFC 4034 §3), not one per name.
        if record.record_type() != RecordType::RRSIG
            && record.ttl() > 0
            && record.ttl() < LOW_TTL
            && in_zone(&pair.0, zone)
            && !pairs.contains(&pair)
        {
            pairs.push(pair);
        }
    }
    if pairs.is_empty() {
        return Vec::new();
    }

    let mut ns_ips: HashMap<String, Vec<IpAddr>> = unique
        .ns()
        .into_iter()
        .map(|n| (n.to_ascii(), Vec::new()))
        .collect();
    if ns_ips.is_empty() {
        return Vec::new();
    }
    raw.resolve_missing_glue(&mut ns_ips).await;
    let servers: Vec<SocketAddr> = raw
        .build_server_list(&ns_ips)
        .servers
        .into_iter()
        .map(|(addr, _)| addr)
        .collect();
    if servers.is_empty() {
        return Vec::new();
    }

    let answers = futures::future::join_all(pairs.iter().map(|(name, rt)| {
        let servers = &servers;
        async move {
            let qname =
                hickory_proto::rr::Name::from_ascii(format!("{}.", name.trim_end_matches('.')))
                    .ok()?;
            let qtype = hickory_proto::rr::RecordType::from(u16::from(*rt));
            raw.parallel_queries(servers, &qname, qtype, timeout)
                .await
                .into_iter()
                .filter_map(|qr| {
                    let resp = qr.result.ok().filter(|r| r.is_authoritative())?;
                    resp.answers()
                        .iter()
                        .filter(|rec| {
                            rec.record_type() == qtype && same_name(&rec.name.to_ascii(), name)
                        })
                        .map(|rec| rec.ttl)
                        .max()
                        .map(|ttl| (ttl, qr.server))
                })
                .max_by_key(|(ttl, _)| *ttl)
        }
    }))
    .await;

    pairs
        .iter()
        .zip(answers)
        .filter_map(|((name, rt), answer)| {
            let (ttl, server) = answer?;
            let lookups = with_ttl(unique, name, *rt, ttl, server);
            (!lookups.is_empty()).then(|| (*rt, Lookups::new(lookups)))
        })
        .collect()
}

/// Every lookup in `lookups` holding records of (`name`, `rt`), reduced to those records, each
/// carrying `ttl`, as answered by `server`. The TTL belongs to the RRset (RFC 2181 §5.2), so it
/// covers records the authoritative answer did not repeat, such as a rotating address set.
fn with_ttl(
    lookups: &Lookups,
    name: &str,
    rt: RecordType,
    ttl: u32,
    server: SocketAddr,
) -> Vec<mhost::resolver::Lookup> {
    use serde_json::Value;

    let rtype = rt.to_string();
    let owned = |rec: &Value| {
        rec["type"].as_str() == Some(rtype.as_str())
            && rec["name"].as_str().is_some_and(|n| same_name(n, name))
    };
    // Every lookup, whatever its query type: `unique_records` keeps a record in the first
    // lookup that carried it (a CNAME in the A lookup, glue in the NS lookup).
    lookups
        .iter()
        .filter_map(|l| serde_json::to_value(l).ok())
        .filter_map(|mut value| {
            let records = value
                .pointer_mut("/result/Response/records")?
                .as_array_mut()?;
            records.retain(|rec| owned(rec));
            if records.is_empty() {
                return None;
            }
            for rec in records.iter_mut() {
                rec["ttl"] = Value::from(ttl);
            }
            value["name_server"] =
                Value::from(mhost::nameserver::NameServerConfig::udp(server).to_string());
            serde_json::from_value(value).ok()
        })
        .collect()
}

/// `lookups` without RRSIG records, for the TTL lint: an RRSIG carries the TTL of the RRset it
/// covers (RFC 4034 §3), which the lint judges on that RRset, while the signature's own cached
/// remainder would add a finding no answer stands behind.
fn without_rrsigs(lookups: &Lookups) -> Lookups {
    use serde_json::Value;

    let rrsig = |rec: &Value| rec["type"].as_str() == Some("RRSIG");
    let kept: Vec<mhost::resolver::Lookup> = lookups
        .iter()
        .map(|lookup| {
            let Ok(mut value) = serde_json::to_value(lookup) else {
                return lookup.clone();
            };
            let Some(records) = value
                .pointer_mut("/result/Response/records")
                .and_then(Value::as_array_mut)
            else {
                return lookup.clone();
            };
            if !records.iter().any(rrsig) {
                return lookup.clone();
            }
            records.retain(|rec| !rrsig(rec));
            serde_json::from_value(value).unwrap_or_else(|_| lookup.clone())
        })
        .collect();
    Lookups::new(kept)
}

/// The synchronous record lints in emit order, run over the unique records of all resolvers,
/// each category's lines deduplicated.
pub fn lint_lookups(lookups: &Lookups) -> Vec<(&'static str, Vec<CheckResult>)> {
    let unique = unique_records(lookups);
    let lookups = &unique;
    vec![
        ("caa", check_caa(lookups)),
        ("cname_apex", check_cname_apex(lookups)),
        ("dnssec", check_dnssec(lookups)),
        ("dnskey_algorithm", check_dnskey_algorithms(lookups)),
        ("dnssec_rollover", check_dnssec_rollover(lookups)),
        ("https_svcb", check_https_svcb_mode(lookups)),
        ("mx", check_mx_sync(lookups)),
        ("ns", check_ns_count(lookups)),
        ("spf", check_spf(lookups)),
        ("ttl", check_ttl(&without_rrsigs(lookups))),
    ]
    .into_iter()
    .map(|(category, results)| (category, unique_lines(results)))
    .collect()
}

/// DNSKEY algorithm lint: warn on deprecated DNSSEC signing algorithms (RFC 8624).
fn check_dnskey_algorithms(lookups: &mhost::resolver::Lookups) -> Vec<CheckResult> {
    let keys = lookups.dnskey();
    if keys.is_empty() {
        return vec![CheckResult::NotFound()];
    }

    let mut results = Vec::new();
    let mut any_deprecated = false;

    for key in &keys {
        match key.algorithm() {
            DnssecAlgorithm::RsaMd5 => {
                results.push(CheckResult::Failed(
                    "DNSKEY uses RSAMD5 (algorithm 1) — deprecated, must not be used (RFC 8624)"
                        .to_string(),
                ));
                any_deprecated = true;
            }
            DnssecAlgorithm::Dsa => {
                results.push(CheckResult::Failed(
                    "DNSKEY uses DSA (algorithm 3) — deprecated, must not be used (RFC 8624)"
                        .to_string(),
                ));
                any_deprecated = true;
            }
            DnssecAlgorithm::RsaSha1 => {
                results.push(CheckResult::Warning(
                    "DNSKEY uses RSASHA1 (algorithm 5) — deprecated, should not be used (RFC 8624)"
                        .to_string(),
                ));
                any_deprecated = true;
            }
            DnssecAlgorithm::RsaSha1Nsec3Sha1 => {
                results.push(CheckResult::Warning("DNSKEY uses RSASHA1-NSEC3-SHA1 (algorithm 7) — deprecated, should not be used (RFC 8624)".to_string()));
                any_deprecated = true;
            }
            _ => {}
        }
    }

    if !any_deprecated {
        results.push(CheckResult::Ok(
            "All DNSKEY algorithms are current (RFC 8624)".to_owned(),
        ));
    }

    results
}

/// Infrastructure lint: flag threat indicators and unusual hosting from enrichment data.
fn check_infrastructure(
    enrichments: &std::collections::HashMap<std::net::IpAddr, IpInfo>,
) -> Vec<CheckResult> {
    let mut results = Vec::new();
    let mut has_concern = false;

    for (ip, info) in enrichments {
        if info.is_spamhaus {
            results.push(CheckResult::Failed(format!(
                "{ip} is listed on Spamhaus blocklists"
            )));
            has_concern = true;
        }
        if info.is_c2 {
            results.push(CheckResult::Failed(format!(
                "{ip} is associated with command-and-control infrastructure"
            )));
            has_concern = true;
        }
        if info.is_tor {
            results.push(CheckResult::Warning(format!(
                "{ip} is a Tor exit node — unusual for DNS infrastructure"
            )));
            has_concern = true;
        }
        if let Some(ref ip_type) = info.ip_type
            && ip_type.eq_ignore_ascii_case("residential")
        {
            results.push(CheckResult::Warning(format!(
                "{ip} is on a residential IP — unusual for MX/NS records"
            )));
            has_concern = true;
        }
    }

    if !has_concern {
        results.push(CheckResult::Ok(
            "No infrastructure concerns detected".to_owned(),
        ));
    }

    results
}

/// The lint results of an NS check and the nameservers that did not answer it.
pub(crate) struct NsCheck {
    pub(crate) results: Vec<CheckResult>,
    /// One finding per server that timed out, naming it.
    pub(crate) silent: Vec<String>,
    /// No server answered the query.
    pub(crate) none_answered: bool,
}

impl NsCheck {
    fn measured(results: Vec<CheckResult>) -> Self {
        Self {
            results,
            silent: Vec::new(),
            none_answered: false,
        }
    }
}

/// The `Unanswered` events of the lame-delegation and delegation-consistency checks, one per
/// check with a silent server.
pub(crate) fn unanswered_events(lame: &NsCheck, delegation: &NsCheck) -> Vec<CheckEvent> {
    [("ns_lame", lame), ("ns_delegation", delegation)]
        .into_iter()
        .filter(|(_, check)| !check.silent.is_empty())
        .map(|(category, check)| CheckEvent::Unanswered {
            category,
            silent: check.silent.clone(),
            none_answered: check.none_answered,
        })
        .collect()
}

/// The finding for a server that did not answer, named by its NS host when known.
fn silent_server(
    server: SocketAddr,
    servers: &[(SocketAddr, Option<String>)],
    error: &dns_raw::RawError,
) -> String {
    let ip = server.ip();
    match servers
        .iter()
        .find(|(addr, _)| *addr == server)
        .and_then(|(_, name)| name.as_deref())
    {
        Some(name) => format!(
            "{} ({ip}) did not answer: {error}",
            name.trim_end_matches('.')
        ),
        None => format!("{ip} did not answer: {error}"),
    }
}

/// NS lame delegation check: query each authoritative NS server with RD=0
/// and check whether it answers authoritatively (AA=1) for the domain.
/// A lame server answers but with AA=0 — indicating misconfiguration.
pub(crate) async fn check_ns_lame_delegation(
    lookups: &Lookups,
    domain: &str,
    timeout: Duration,
    raw: &RawOutbound,
) -> NsCheck {
    let ns_names: Vec<String> = lookups.ns().into_iter().map(|n| n.to_ascii()).collect();

    if ns_names.is_empty() {
        return NsCheck::measured(vec![CheckResult::NotFound()]);
    }

    // Resolve NS hostnames to IPs.
    let mut ns_ips: HashMap<String, Vec<IpAddr>> =
        ns_names.iter().map(|n| (n.clone(), Vec::new())).collect();
    raw.resolve_missing_glue(&mut ns_ips).await;

    let domain_name = match hickory_proto::rr::Name::from_ascii(domain) {
        Ok(n) => n,
        Err(_) => return NsCheck::measured(vec![CheckResult::NotFound()]),
    };

    let list = raw.build_server_list(&ns_ips);
    let servers: Vec<SocketAddr> = list.servers.iter().map(|(addr, _)| *addr).collect();

    if servers.is_empty() {
        if list.refused > 0 {
            return NsCheck::measured(vec![CheckResult::Warning(NOT_PUBLIC.to_owned())]);
        }
        return NsCheck::measured(vec![CheckResult::Warning(
            "NS lame delegation: could not resolve any NS server IPs for direct query".to_owned(),
        )]);
    }

    let results_raw = raw
        .parallel_queries(
            &servers,
            &domain_name,
            hickory_proto::rr::RecordType::SOA,
            timeout,
        )
        .await;

    let mut check = judge_lame_delegation(&results_raw, &list.servers, domain);
    if list.refused > 0 {
        check
            .results
            .push(CheckResult::Warning(NOT_PUBLIC.to_owned()));
        check.none_answered = false;
    }
    check
}

fn judge_lame_delegation(
    results_raw: &[dns_raw::RawQueryResult],
    servers: &[(SocketAddr, Option<String>)],
    domain: &str,
) -> NsCheck {
    let mut lame_servers: Vec<String> = Vec::new();
    let mut ok_count = 0usize;
    let mut silent: Vec<String> = Vec::new();

    for qr in results_raw {
        match &qr.result {
            Ok(resp) => {
                if resp.is_authoritative() {
                    ok_count += 1;
                } else {
                    lame_servers.push(qr.server.ip().to_string());
                }
            }
            Err(e) => silent.push(silent_server(qr.server, servers, e)),
        }
    }

    if lame_servers.is_empty() && ok_count == 0 {
        return NsCheck {
            results: vec![CheckResult::Warning(
                "NS lame delegation: no servers responded to SOA query".to_owned(),
            )],
            silent,
            none_answered: true,
        };
    }

    let mut results = Vec::new();
    if lame_servers.is_empty() {
        results.push(CheckResult::Ok(format!(
            "All {ok_count} NS server(s) answered authoritatively (AA=1)"
        )));
    } else {
        for server in &lame_servers {
            results.push(CheckResult::Failed(format!(
                "NS server {server} is lame: answered with AA=0 (not authoritative for {domain})"
            )));
        }
        if ok_count > 0 {
            results.push(CheckResult::Warning(format!(
                "{ok_count} NS server(s) answered correctly; {} server(s) lame",
                lame_servers.len()
            )));
        }
    }
    NsCheck {
        results,
        silent,
        none_answered: false,
    }
}

/// NS delegation consistency check: compare NS names from recursive resolution
/// with NS names obtained by querying the zone's authoritative servers directly.
/// Inconsistency indicates stale delegation or incomplete NS synchronization.
pub(crate) async fn check_ns_delegation_consistency(
    lookups: &Lookups,
    domain: &str,
    timeout: Duration,
    raw: &RawOutbound,
) -> NsCheck {
    // Recursive NS names (already collected).
    let mut recursive_ns: Vec<String> = lookups
        .ns()
        .into_iter()
        .map(|n| n.to_ascii().to_ascii_lowercase())
        .collect();
    recursive_ns.sort();
    recursive_ns.dedup();

    if recursive_ns.is_empty() {
        return NsCheck::measured(vec![CheckResult::NotFound()]);
    }

    // Query the authoritative NS servers directly for NS records.
    let mut ns_ips: HashMap<String, Vec<IpAddr>> = recursive_ns
        .iter()
        .map(|n| (n.clone(), Vec::new()))
        .collect();
    raw.resolve_missing_glue(&mut ns_ips).await;

    let list = raw.build_server_list(&ns_ips);
    let servers: Vec<SocketAddr> = list.servers.iter().map(|(addr, _)| *addr).collect();

    if servers.is_empty() {
        if list.refused > 0 {
            return NsCheck::measured(vec![CheckResult::Warning(NOT_PUBLIC.to_owned())]);
        }
        return NsCheck::measured(vec![CheckResult::Warning(
            "NS delegation consistency: could not resolve NS server IPs for direct query"
                .to_owned(),
        )]);
    }

    let domain_name = match hickory_proto::rr::Name::from_ascii(domain) {
        Ok(n) => n,
        Err(_) => return NsCheck::measured(vec![CheckResult::NotFound()]),
    };

    let results_raw = raw
        .parallel_queries(
            &servers,
            &domain_name,
            hickory_proto::rr::RecordType::NS,
            timeout,
        )
        .await;

    let mut check = judge_ns_consistency(&recursive_ns, &results_raw, &list.servers);
    if list.refused > 0 {
        check
            .results
            .push(CheckResult::Warning(NOT_PUBLIC.to_owned()));
        check.none_answered = false;
    }
    check
}

fn judge_ns_consistency(
    recursive_ns: &[String],
    results_raw: &[dns_raw::RawQueryResult],
    servers: &[(SocketAddr, Option<String>)],
) -> NsCheck {
    // Collect NS names from direct (authoritative) responses.
    let mut auth_ns: Vec<String> = Vec::new();
    let mut silent: Vec<String> = Vec::new();
    let mut answered = false;
    for qr in results_raw {
        match &qr.result {
            Ok(resp) => {
                answered = true;
                for record in resp.answers() {
                    if let hickory_proto::rr::RData::NS(ns) = &record.data {
                        let name = ns.0.to_ascii().to_ascii_lowercase();
                        if !auth_ns.contains(&name) {
                            auth_ns.push(name);
                        }
                    }
                }
            }
            Err(e) => silent.push(silent_server(qr.server, servers, e)),
        }
    }
    let check = |results: Vec<CheckResult>| NsCheck {
        results,
        silent: silent.clone(),
        none_answered: !answered,
    };

    if auth_ns.is_empty() {
        return check(vec![CheckResult::Warning(
            "NS delegation consistency: no NS records returned from direct authoritative query"
                .to_owned(),
        )]);
    }

    auth_ns.sort();
    auth_ns.dedup();

    if recursive_ns == auth_ns.as_slice() {
        check(vec![CheckResult::Ok(format!(
            "NS delegation is consistent: {} name server(s) match between parent and child",
            recursive_ns.len()
        ))])
    } else {
        let only_recursive: Vec<&String> = recursive_ns
            .iter()
            .filter(|n| !auth_ns.contains(n))
            .collect();
        let only_auth: Vec<&String> = auth_ns
            .iter()
            .filter(|n| !recursive_ns.contains(n))
            .collect();

        let mut results = Vec::new();
        for ns in &only_recursive {
            results.push(CheckResult::Warning(format!(
                "NS '{ns}' is in parent delegation but not in child zone NS records"
            )));
        }
        for ns in &only_auth {
            results.push(CheckResult::Warning(format!(
                "NS '{ns}' is in child zone but not in parent delegation"
            )));
        }
        check(results)
    }
}

/// DNSSEC rollover detection: check for multiple KSKs, orphaned DS records (DS
/// with no matching DNSKEY key tag), and new KSKs that have no DS record yet.
fn check_dnssec_rollover(lookups: &Lookups) -> Vec<CheckResult> {
    let dnskeys = lookups.dnskey();
    let ds_records = lookups.ds();

    if dnskeys.is_empty() && ds_records.is_empty() {
        return vec![CheckResult::NotFound()];
    }

    if dnskeys.is_empty() {
        return vec![CheckResult::Warning(
            "DS record(s) present but no DNSKEY records found — possible rollover in progress or lame DNSSEC delegation".to_owned(),
        )];
    }

    // Collect KSK key tags (SEP bit set, zone key bit set).
    let ksk_tags: Vec<u16> = dnskeys
        .iter()
        .filter(|k| k.is_zone_key() && k.is_secure_entry_point())
        .filter_map(|k| k.key_tag())
        .collect();

    let ds_tags: Vec<u16> = ds_records.iter().map(|d| d.key_tag()).collect();

    let mut results = Vec::new();

    // Multiple KSKs — potential double-KSK rollover window.
    if ksk_tags.len() > 1 {
        results.push(CheckResult::Warning(format!(
            "Multiple KSKs present ({} KSKs) — DNSSEC key rollover may be in progress",
            ksk_tags.len()
        )));
    }

    // DS records with no matching DNSKEY (orphaned DS).
    for &ds_tag in &ds_tags {
        if !ksk_tags.contains(&ds_tag)
            && !dnskeys
                .iter()
                .filter_map(|k| k.key_tag())
                .any(|t| t == ds_tag)
        {
            results.push(CheckResult::Failed(format!(
                "DS key tag {ds_tag} has no matching DNSKEY — orphaned DS record (old key removed before DS was removed)"
            )));
        }
    }

    // KSKs with no matching DS record.
    for &ksk_tag in &ksk_tags {
        if !ds_tags.contains(&ksk_tag) {
            results.push(CheckResult::Warning(format!(
                "KSK with key tag {ksk_tag} has no matching DS record — new KSK not yet published in parent zone"
            )));
        }
    }

    if results.is_empty() {
        if ksk_tags.is_empty() {
            results.push(CheckResult::Warning(
                "DNSKEY records present but no KSK (SEP bit) found".to_owned(),
            ));
        } else {
            results.push(CheckResult::Ok(format!(
                "DNSSEC rollover state is clean: {} KSK(s) each matched by a DS record",
                ksk_tags.len()
            )));
        }
    }

    results
}

/// Parse TXT record tags: split on `;`, trim each segment, split on first `=`.
/// Returns (tag_name, tag_value) pairs.
fn parse_txt_tags(txt: &str) -> Vec<(&str, &str)> {
    txt.split(';')
        .map(|seg| seg.trim())
        .filter(|seg| !seg.is_empty())
        .map(|seg| {
            if let Some(pos) = seg.find('=') {
                (seg[..pos].trim(), seg[pos + 1..].trim())
            } else {
                (seg.trim(), "")
            }
        })
        .collect()
}

/// BIMI record check: validate `default._bimi.<domain>` TXT record.
fn check_bimi(lookups: &Lookups) -> Vec<CheckResult> {
    let txt_vec = lookups.txt();
    if txt_vec.is_empty() {
        return vec![CheckResult::NotFound()];
    }

    let unique = txt_vec.unique();
    let bimi_txts: Vec<String> = unique
        .iter()
        .map(TXT::as_string)
        .filter(|s| s.contains("v=BIMI1"))
        .collect();

    if bimi_txts.is_empty() {
        return vec![CheckResult::Warning(
            "TXT record found but no v=BIMI1 tag".into(),
        )];
    }

    if bimi_txts.len() > 1 {
        return vec![CheckResult::Failed(
            "Multiple BIMI records found; only one is permitted".into(),
        )];
    }

    let record = &bimi_txts[0];
    let tags = parse_txt_tags(record);

    let l_tag = tags.iter().find(|(name, _)| *name == "l");
    let a_tag = tags.iter().find(|(name, _)| *name == "a");

    let Some((_, l_value)) = l_tag else {
        return vec![CheckResult::Failed("Missing l= tag in BIMI record".into())];
    };

    if l_value.is_empty() {
        return vec![CheckResult::Ok("BIMI self-assertion (no logo)".into())];
    }

    if l_value.starts_with("http://") {
        return vec![CheckResult::Warning("BIMI logo URL must use HTTPS".into())];
    }

    let mut results = vec![CheckResult::Ok("BIMI record valid".into())];

    if let Some((_, a_value)) = a_tag
        && a_value.starts_with("http://")
    {
        results.push(CheckResult::Warning(
            "BIMI authority URL must use HTTPS".into(),
        ));
    }

    results
}

// ---------------------------------------------------------------------------
// MTA-STS helpers
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum MtaStsMode {
    Enforce,
    Testing,
    Disabled,
}

#[derive(Debug)]
struct MtaStsPolicy {
    mode: MtaStsMode,
    max_age: u32,
    mx_patterns: Vec<String>,
}

/// Parse an MTA-STS policy file (RFC 8461 §3.2).
/// Returns `Err(message)` on any structural/semantic error.
fn parse_mta_sts_policy(text: &str) -> Result<MtaStsPolicy, String> {
    let mut version_ok = false;
    let mut mode: Option<MtaStsMode> = None;
    let mut max_age: Option<u32> = None;
    let mut mx_patterns: Vec<String> = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();

        match key {
            "version" => {
                if value != "STSv1" {
                    return Err(format!("Invalid MTA-STS policy version: {value}"));
                }
                version_ok = true;
            }
            "mode" => {
                mode = Some(match value {
                    "enforce" => MtaStsMode::Enforce,
                    "testing" => MtaStsMode::Testing,
                    "none" => MtaStsMode::Disabled,
                    other => return Err(format!("Invalid MTA-STS mode: {other}")),
                });
            }
            "max_age" => {
                let n: u32 = value
                    .parse()
                    .map_err(|_| format!("Invalid MTA-STS max_age: {value}"))?;
                if n == 0 || n > 31_557_600 {
                    return Err(format!(
                        "MTA-STS max_age out of range: {n} (must be 1-31557600)"
                    ));
                }
                max_age = Some(n);
            }
            "mx" if !value.is_empty() => {
                mx_patterns.push(value.trim_end_matches('.').to_lowercase());
            }
            _ => {} // ignore extension fields per RFC 8461 §3.2
        }
    }

    if !version_ok {
        return Err("MTA-STS policy missing version field".into());
    }
    let mode = mode.ok_or_else(|| "MTA-STS policy missing mode field".to_string())?;
    let max_age = max_age.ok_or_else(|| "MTA-STS policy missing max_age field".to_string())?;
    if mx_patterns.is_empty() {
        return Err("MTA-STS policy has no mx entries".into());
    }

    Ok(MtaStsPolicy {
        mode,
        max_age,
        mx_patterns,
    })
}

/// Returns true if `mx_host` matches the MTA-STS `pattern` (RFC 8461 §4.1).
/// Wildcards (`*.example.com`) match exactly one label.
fn mx_matches_pattern(mx_host: &str, pattern: &str) -> bool {
    let mx = mx_host.trim_end_matches('.').to_lowercase();
    let pat = pattern.trim_end_matches('.').to_lowercase();

    if let Some(suffix) = pat.strip_prefix("*.") {
        mx.ends_with(&format!(".{suffix}")) && mx[..mx.len() - suffix.len() - 1].find('.').is_none()
    } else {
        mx == pat
    }
}

/// Validate the MTA-STS DNS TXT record at `_mta-sts.<domain>`.
/// Returns `Ok(())` if valid, `Err(results)` with error results on failure.
fn validate_mta_sts_dns(lookups: &Lookups) -> Result<(), Vec<CheckResult>> {
    let txt_vec = lookups.txt();
    if txt_vec.is_empty() {
        return Err(vec![CheckResult::NotFound()]);
    }

    let unique = txt_vec.unique();
    let sts_txts: Vec<String> = unique
        .iter()
        .map(TXT::as_string)
        .filter(|s| s.contains("v=STSv1"))
        .collect();

    if sts_txts.len() > 1 {
        return Err(vec![CheckResult::Failed(
            "Multiple MTA-STS records found; only one is permitted".into(),
        )]);
    }

    if sts_txts.is_empty() {
        return Err(vec![CheckResult::Failed(
            "Unrecognized MTA-STS version tag".into(),
        )]);
    }

    let record = &sts_txts[0];
    let tags = parse_txt_tags(record);

    let id_tag = tags.iter().find(|(name, _)| *name == "id");
    let Some((_, id_value)) = id_tag else {
        return Err(vec![CheckResult::Failed(
            "id= is required in MTA-STS record".into(),
        )]);
    };

    if id_value.is_empty()
        || id_value.len() > 32
        || !id_value.chars().all(|c| c.is_ascii_alphanumeric())
    {
        return Err(vec![CheckResult::Failed(
            "Invalid id= value: must be alphanumeric, max 32 chars".into(),
        )]);
    }

    Ok(())
}

/// Outbound fetch settings for the MTA-STS policy fetch.
pub(crate) struct Outbound {
    pub(crate) settings: ClientSettings,
    pub(crate) resolver: Arc<dyn Resolve>,
    pub(crate) allow: fn(IpAddr) -> bool,
}

impl Outbound {
    fn production() -> Self {
        Self {
            settings: ClientSettings {
                user_agent: Some(concat!("prism/", env!("CARGO_PKG_VERSION")).to_string()),
                ..ClientSettings::default()
            },
            resolver: Arc::new(SystemResolver),
            allow: is_allowed_target,
        }
    }
}

/// MTA-STS check: validate DNS record then fetch and validate the policy file.
async fn check_mta_sts(
    lookups: &Lookups,
    domain: &str,
    mx_lookups: &Lookups,
    outbound: &Outbound,
) -> Vec<CheckResult> {
    let policy_url = format!("https://mta-sts.{domain}/.well-known/mta-sts.txt");
    check_mta_sts_at(lookups, domain, mx_lookups, outbound, &policy_url).await
}

/// MTA-STS check against the policy file at `policy_url`.
pub(crate) async fn check_mta_sts_at(
    lookups: &Lookups,
    _domain: &str,
    mx_lookups: &Lookups,
    outbound: &Outbound,
    policy_url: &str,
) -> Vec<CheckResult> {
    if let Err(results) = validate_mta_sts_dns(lookups) {
        return results;
    }

    let mut results = vec![CheckResult::Ok("MTA-STS DNS record valid".into())];

    let opts = FetchOptions {
        max_redirects: 10,
        at_limit: AtLimit::Fail,
        body_cap: 64 * 1024,
        timeout: Duration::from_secs(5),
        allow: outbound.allow,
        ..FetchOptions::new(Method::GET)
    };
    let response = match fetch(
        &outbound.settings,
        outbound.resolver.clone(),
        policy_url,
        &opts,
    )
    .await
    {
        Ok(resp) => resp,
        Err(FetchError::Timeout) => {
            results.push(CheckResult::Warning(
                "MTA-STS policy file fetch timed out".into(),
            ));
            return results;
        }
        Err(_) => {
            results.push(CheckResult::Warning(
                "MTA-STS policy file unreachable".into(),
            ));
            return results;
        }
    };

    if !response.status.is_success() {
        results.push(CheckResult::Failed(format!(
            "MTA-STS policy file fetch failed: HTTP {}",
            response.status.as_u16()
        )));
        return results;
    }

    // Lossy, as reqwest's `text()` decoded it before: a stray non-UTF-8 byte keeps today's result.
    let body = String::from_utf8_lossy(&response.body);

    let policy = match parse_mta_sts_policy(&body) {
        Err(msg) => {
            results.push(CheckResult::Failed(msg));
            return results;
        }
        Ok(p) => p,
    };

    match policy.mode {
        MtaStsMode::Disabled => {
            results.push(CheckResult::Warning(
                "MTA-STS mode is none; policy not enforced".into(),
            ));
        }
        MtaStsMode::Testing => {
            results.push(CheckResult::Warning(format!(
                "MTA-STS mode is testing; policy not yet enforced (max_age: {}s)",
                policy.max_age
            )));
        }
        MtaStsMode::Enforce => {
            results.push(CheckResult::Ok(format!(
                "MTA-STS policy valid (mode: enforce, max_age: {}s)",
                policy.max_age
            )));
        }
    }

    // Cross-check: every MX host in DNS must be covered by a policy mx pattern.
    let mx_hosts: Vec<String> = mx_lookups
        .mx()
        .into_iter()
        .map(|mx| {
            mx.exchange()
                .to_ascii()
                .trim_end_matches('.')
                .to_lowercase()
        })
        .collect();

    let uncovered: Vec<String> = mx_hosts
        .iter()
        .filter(|mx| !policy.mx_patterns.iter().any(|p| mx_matches_pattern(mx, p)))
        .cloned()
        .collect();

    if !uncovered.is_empty() {
        results.push(CheckResult::Warning(format!(
            "MX host(s) not covered by MTA-STS policy: {}",
            uncovered.join(", ")
        )));
    }

    results
}

/// TLSRPT record check: validate `_smtp._tls.<domain>` TXT record.
/// Also checks co-existence with MTA-STS.
fn check_tlsrpt(lookups: &Lookups, mta_sts_lookups: &Lookups) -> Vec<CheckResult> {
    let txt_vec = lookups.txt();
    let mut results = if txt_vec.is_empty() {
        vec![CheckResult::NotFound()]
    } else {
        let unique = txt_vec.unique();
        let rpt_txts: Vec<String> = unique
            .iter()
            .map(TXT::as_string)
            .filter(|s| s.contains("v=TLSRPTv1"))
            .collect();

        if rpt_txts.len() > 1 {
            return vec![CheckResult::Failed(
                "Multiple TLSRPT records found; only one is permitted".into(),
            )];
        } else if rpt_txts.is_empty() {
            return vec![CheckResult::Failed(
                "Unrecognized TLSRPT version tag".into(),
            )];
        } else {
            let record = &rpt_txts[0];
            let tags = parse_txt_tags(record);

            let rua_tag = tags.iter().find(|(name, _)| *name == "rua");

            let Some((_, rua_value)) = rua_tag else {
                return vec![CheckResult::Failed(
                    "rua= is required in TLSRPT record".into(),
                )];
            };

            let uris: Vec<&str> = rua_value.split(',').map(|u| u.trim()).collect();
            let all_valid = uris
                .iter()
                .all(|uri| uri.starts_with("mailto:") || uri.starts_with("https:"));

            if !all_valid {
                return vec![CheckResult::Failed(
                    "rua= must be mailto: or https: URI".into(),
                )];
            }

            vec![CheckResult::Ok("TLSRPT record valid".into())]
        }
    };

    // Co-existence check: warn if MTA-STS is present but TLSRPT is absent.
    let is_not_found = results.len() == 1 && matches!(results[0], CheckResult::NotFound());
    if is_not_found {
        let mta_txt_vec = mta_sts_lookups.txt();
        let mta_unique = mta_txt_vec.unique();
        let has_mta_sts = mta_unique
            .iter()
            .map(TXT::as_string)
            .any(|s| s.contains("v=STSv1"));
        if has_mta_sts {
            results.push(CheckResult::Warning(
                "TLSRPT record missing; TLS reporting recommended when MTA-STS is deployed".into(),
            ));
        }
    }

    results
}

#[cfg(test)]
#[path = "check_mta_sts_tests.rs"]
mod mta_sts_outbound_tests;

#[cfg(test)]
#[path = "check_ns_outbound_tests.rs"]
mod ns_outbound_tests;

#[cfg(test)]
#[path = "check_zone_tests.rs"]
mod zone_tests;

#[cfg(test)]
#[path = "check_ttl_tests.rs"]
mod ttl_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_check_guard_decrements_gauge_when_dropped_early() {
        let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        let _local = metrics::set_default_local_recorder(&recorder);

        let guard = ActiveCheck::new();
        assert!(handle.render().contains("netray_dns_active_checks 1"));
        drop(guard);
        assert!(handle.render().contains("netray_dns_active_checks 0"));
    }

    /// Build a `Lookups` containing TXT records via JSON deserialization.
    /// The mhost crate's `new_for_test` constructors are `#[cfg(test)]`-gated
    /// and only available within the mhost crate itself. We use serde round-trip
    /// through the public `Serialize`/`Deserialize` impls instead.
    fn make_txt_lookups(domain: &str, txts: &[&str]) -> Lookups {
        // txt_data is Box<[Box<[u8]>]> — serialized as array of arrays of u8.
        let records: Vec<serde_json::Value> = txts
            .iter()
            .map(|txt| {
                let bytes: Vec<Vec<u8>> = vec![txt.as_bytes().to_vec()];
                serde_json::json!({
                    "name": domain,
                    "type": "TXT",
                    "ttl": 300,
                    "data": { "TXT": { "txt_data": bytes } }
                })
            })
            .collect();

        let lookups_json = serde_json::json!({
            "lookups": [{
                "query": {
                    "name": domain,
                    "record_type": "TXT"
                },
                "name_server": "udp:127.0.0.1:53",
                "result": {
                    "Response": {
                        "records": records,
                        "response_time": { "secs": 0, "nanos": 10000000 },
                        "valid_until": "2099-01-01T00:00:00Z"
                    }
                }
            }]
        });

        serde_json::from_value(lookups_json).expect("failed to deserialize test Lookups")
    }

    #[test]
    fn test_check_total_steps_is_19() {
        assert_eq!(CHECK_TOTAL_STEPS, 19);
    }

    // -- BIMI tests --

    #[test]
    fn test_check_bimi_valid() {
        let lookups = make_txt_lookups(
            "default._bimi.example.com",
            &["v=BIMI1; l=https://example.com/logo.svg"],
        );
        let results = check_bimi(&lookups);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], CheckResult::Ok(msg) if msg.contains("BIMI record valid")));
    }

    #[test]
    fn test_check_bimi_http_logo() {
        let lookups = make_txt_lookups(
            "default._bimi.example.com",
            &["v=BIMI1; l=http://example.com/logo.svg"],
        );
        let results = check_bimi(&lookups);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], CheckResult::Warning(msg) if msg.contains("HTTPS")));
    }

    #[test]
    fn test_check_bimi_missing_l() {
        let lookups = make_txt_lookups("default._bimi.example.com", &["v=BIMI1"]);
        let results = check_bimi(&lookups);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], CheckResult::Failed(msg) if msg.contains("Missing l=")));
    }

    #[test]
    fn test_check_bimi_empty_l() {
        let lookups = make_txt_lookups("default._bimi.example.com", &["v=BIMI1; l="]);
        let results = check_bimi(&lookups);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], CheckResult::Ok(msg) if msg.contains("self-assertion")));
    }

    #[test]
    fn test_check_bimi_http_authority() {
        let lookups = make_txt_lookups(
            "default._bimi.example.com",
            &["v=BIMI1; l=https://example.com/logo.svg; a=http://example.com/auth"],
        );
        let results = check_bimi(&lookups);
        assert_eq!(results.len(), 2);
        assert!(matches!(&results[0], CheckResult::Ok(msg) if msg.contains("BIMI record valid")));
        assert!(
            matches!(&results[1], CheckResult::Warning(msg) if msg.contains("BIMI authority URL must use HTTPS"))
        );
    }

    #[test]
    fn test_check_bimi_multiple_records() {
        let lookups = make_txt_lookups(
            "default._bimi.example.com",
            &[
                "v=BIMI1; l=https://example.com/a.svg",
                "v=BIMI1; l=https://example.com/b.svg",
            ],
        );
        let results = check_bimi(&lookups);
        assert_eq!(results.len(), 1);
        assert!(
            matches!(&results[0], CheckResult::Failed(msg) if msg.contains("Multiple BIMI records"))
        );
    }

    #[test]
    fn test_check_bimi_not_found() {
        let lookups = Lookups::empty();
        let results = check_bimi(&lookups);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], CheckResult::NotFound()));
    }

    // -- MTA-STS DNS record tests --

    #[test]
    fn test_mta_sts_dns_valid() {
        let lookups = make_txt_lookups("_mta-sts.example.com", &["v=STSv1; id=20240101000000"]);
        assert!(validate_mta_sts_dns(&lookups).is_ok());
    }

    #[test]
    fn test_mta_sts_dns_missing_id() {
        let lookups = make_txt_lookups("_mta-sts.example.com", &["v=STSv1"]);
        let err = validate_mta_sts_dns(&lookups).unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(matches!(&err[0], CheckResult::Failed(msg) if msg.contains("id=")));
    }

    #[test]
    fn test_mta_sts_dns_invalid_id() {
        let lookups = make_txt_lookups("_mta-sts.example.com", &["v=STSv1; id=foo bar!"]);
        let err = validate_mta_sts_dns(&lookups).unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(matches!(&err[0], CheckResult::Failed(msg) if msg.contains("Invalid id=")));
    }

    #[test]
    fn test_mta_sts_dns_not_found() {
        let lookups = Lookups::empty();
        let err = validate_mta_sts_dns(&lookups).unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(matches!(&err[0], CheckResult::NotFound()));
    }

    // -- MTA-STS policy parsing tests --

    #[test]
    fn test_parse_mta_sts_policy_enforce() {
        let text = "version: STSv1\nmode: enforce\nmx: mail.example.com\nmx: *.example.com\nmax_age: 604800\n";
        let policy = parse_mta_sts_policy(text).unwrap();
        assert!(matches!(policy.mode, MtaStsMode::Enforce));
        assert_eq!(policy.max_age, 604800);
        assert_eq!(
            policy.mx_patterns,
            vec!["mail.example.com", "*.example.com"]
        );
    }

    #[test]
    fn test_parse_mta_sts_policy_testing() {
        let text = "version: STSv1\nmode: testing\nmx: mail.example.com\nmax_age: 86400\n";
        let policy = parse_mta_sts_policy(text).unwrap();
        assert!(matches!(policy.mode, MtaStsMode::Testing));
    }

    #[test]
    fn test_parse_mta_sts_policy_none_mode() {
        let text = "version: STSv1\nmode: none\nmx: mail.example.com\nmax_age: 86400\n";
        let policy = parse_mta_sts_policy(text).unwrap();
        assert!(matches!(policy.mode, MtaStsMode::Disabled));
    }

    #[test]
    fn test_parse_mta_sts_policy_missing_version() {
        let text = "mode: enforce\nmx: mail.example.com\nmax_age: 604800\n";
        assert!(parse_mta_sts_policy(text).is_err());
    }

    #[test]
    fn test_parse_mta_sts_policy_missing_mode() {
        let text = "version: STSv1\nmx: mail.example.com\nmax_age: 604800\n";
        assert!(parse_mta_sts_policy(text).is_err());
    }

    #[test]
    fn test_parse_mta_sts_policy_missing_max_age() {
        let text = "version: STSv1\nmode: enforce\nmx: mail.example.com\n";
        assert!(parse_mta_sts_policy(text).is_err());
    }

    #[test]
    fn test_parse_mta_sts_policy_no_mx() {
        let text = "version: STSv1\nmode: enforce\nmax_age: 604800\n";
        assert!(parse_mta_sts_policy(text).is_err());
    }

    #[test]
    fn test_parse_mta_sts_policy_invalid_max_age() {
        let text = "version: STSv1\nmode: enforce\nmx: mail.example.com\nmax_age: 0\n";
        assert!(parse_mta_sts_policy(text).is_err());

        let text = "version: STSv1\nmode: enforce\nmx: mail.example.com\nmax_age: 99999999\n";
        assert!(parse_mta_sts_policy(text).is_err());
    }

    #[test]
    fn test_parse_mta_sts_policy_ignores_extension_fields() {
        let text =
            "version: STSv1\nmode: enforce\nmx: mail.example.com\nmax_age: 604800\nfoo: bar\n";
        assert!(parse_mta_sts_policy(text).is_ok());
    }

    // -- MX pattern matching tests --

    #[test]
    fn test_mx_matches_exact() {
        assert!(mx_matches_pattern("mail.example.com", "mail.example.com"));
        assert!(!mx_matches_pattern("other.example.com", "mail.example.com"));
    }

    #[test]
    fn test_mx_matches_wildcard() {
        assert!(mx_matches_pattern("mail.example.com", "*.example.com"));
        assert!(mx_matches_pattern("smtp.example.com", "*.example.com"));
        assert!(!mx_matches_pattern("a.mail.example.com", "*.example.com"));
        assert!(!mx_matches_pattern("example.com", "*.example.com"));
    }

    #[test]
    fn test_mx_matches_case_insensitive() {
        assert!(mx_matches_pattern("Mail.Example.COM", "mail.example.com"));
        assert!(mx_matches_pattern("mail.example.com", "Mail.Example.COM"));
    }

    #[test]
    fn test_mx_matches_trailing_dot_normalized() {
        assert!(mx_matches_pattern("mail.example.com.", "mail.example.com"));
        assert!(mx_matches_pattern("mail.example.com", "mail.example.com."));
    }

    // -- TLSRPT tests --

    #[test]
    fn test_check_tlsrpt_valid_mailto() {
        let lookups = make_txt_lookups(
            "_smtp._tls.example.com",
            &["v=TLSRPTv1; rua=mailto:tls@example.com"],
        );
        let mta_sts = Lookups::empty();
        let results = check_tlsrpt(&lookups, &mta_sts);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], CheckResult::Ok(msg) if msg.contains("TLSRPT record valid")));
    }

    #[test]
    fn test_check_tlsrpt_valid_https() {
        let lookups = make_txt_lookups(
            "_smtp._tls.example.com",
            &["v=TLSRPTv1; rua=https://example.com/report"],
        );
        let mta_sts = Lookups::empty();
        let results = check_tlsrpt(&lookups, &mta_sts);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], CheckResult::Ok(msg) if msg.contains("TLSRPT record valid")));
    }

    #[test]
    fn test_check_tlsrpt_multiple_rua() {
        let lookups = make_txt_lookups(
            "_smtp._tls.example.com",
            &["v=TLSRPTv1; rua=mailto:a@example.com,https://example.com/r"],
        );
        let mta_sts = Lookups::empty();
        let results = check_tlsrpt(&lookups, &mta_sts);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], CheckResult::Ok(msg) if msg.contains("TLSRPT record valid")));
    }

    #[test]
    fn test_check_tlsrpt_invalid_rua() {
        let lookups = make_txt_lookups(
            "_smtp._tls.example.com",
            &["v=TLSRPTv1; rua=ftp://example.com"],
        );
        let mta_sts = Lookups::empty();
        let results = check_tlsrpt(&lookups, &mta_sts);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], CheckResult::Failed(_)));
    }

    #[test]
    fn test_check_tlsrpt_coexistence_warning() {
        let tlsrpt = Lookups::empty();
        let mta_sts = make_txt_lookups("_mta-sts.example.com", &["v=STSv1; id=abc123"]);
        let results = check_tlsrpt(&tlsrpt, &mta_sts);
        assert_eq!(results.len(), 2);
        assert!(matches!(&results[0], CheckResult::NotFound()));
        assert!(
            matches!(&results[1], CheckResult::Warning(msg) if msg.contains("TLSRPT record missing"))
        );
    }

    #[test]
    fn test_check_tlsrpt_both_absent() {
        let tlsrpt = Lookups::empty();
        let mta_sts = Lookups::empty();
        let results = check_tlsrpt(&tlsrpt, &mta_sts);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], CheckResult::NotFound()));
    }
}
