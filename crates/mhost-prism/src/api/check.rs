//! Check endpoint: run DNS lookups and lint analysis for a domain, streaming results as SSE.
//!
//! - `POST /api/check` — accept a structured JSON body with a domain and optional servers.
//!
//! Phase 1 streams batch events (one per record type queried).
//! Phase 2 streams lint events (one per lint category, synchronous pure checks).
//! Phase 3 sends a done event with summary counts.

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::Response;
use futures::stream::{FuturesUnordered, StreamExt};
use mhost::RecordType;
use mhost::lints::{
    CheckResult, check_caa, check_cname_apex, check_dmarc_records, check_dnssec,
    check_https_svcb_mode, check_mx_sync, check_ns_count, check_spf, check_ttl, is_dmarc,
};
use mhost::resolver::lookup::Uniquify;
use mhost::resolver::{Lookups, MultiQuery, Resolver};
use mhost::resources::rdata::{DnssecAlgorithm, TXT};
use serde::{Deserialize, Serialize};
use tokio::sync::{Semaphore, mpsc};
use tokio_stream::wrappers::ReceiverStream;

use crate::RequestId;
use crate::dns_raw;
use crate::api::query::{
    StreamParams, build_resolver_group, effective_server_specs, extract_ips_from_cached_events,
    make_error_event, parse_server_spec, record_breaker_outcomes, target_keys_from_servers,
};
use crate::api::{AppState, BatchEvent, CollectedResponse, STREAM_TIMEOUT_SECS};
use crate::circuit_breaker::{BreakerState, CircuitBreakerRegistry};
use crate::error::{ApiError, ErrorResponse};
use crate::parser::ParsedQuery;
use crate::record_format;
use crate::result_cache::{CachedEvent, CachedResult, ResultCache};
use crate::security::QueryPolicy;
use netray_common::enrichment::IpInfo;

// ---------------------------------------------------------------------------
// Record types queried for the base domain (15 types)
// ---------------------------------------------------------------------------

const CHECK_RECORD_TYPES: [RecordType; 15] = [
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

// Total SSE batch steps = 15 base types + 1 DMARC lookup.
const CHECK_TOTAL_STEPS: u32 = 16;

// ---------------------------------------------------------------------------
// SSE event payloads
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct LintEvent {
    request_id: String,
    category: &'static str,
    results: Vec<CheckResult>,
}

#[derive(Serialize)]
struct CheckDoneEvent {
    request_id: String,
    duration_ms: u64,
    total_checks: u32,
    passed: u32,
    warnings: u32,
    failed: u32,
    not_found: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_key: Option<String>,
}

// ---------------------------------------------------------------------------
// POST handler
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CheckRequest {
    /// Domain name to check (e.g. `"example.com"`).
    domain: String,
    /// DNS servers to use. Defaults to config default_servers.
    #[serde(default)]
    servers: Vec<String>,
    /// Query timeout in seconds. Clamped to config max (default 10).
    #[serde(default)]
    timeout_secs: Option<u64>,
}

/// Run a comprehensive DNS health check for a domain.
///
/// Queries 15 record types plus `_dmarc.<domain>` TXT, then runs 9 lint categories
/// (CAA, CNAME apex, DNSSEC, HTTPS/SVCB, MX, NS count, SPF, TTL consistency, DMARC).
/// Results stream as Server-Sent Events.
///
/// ## SSE Events
///
/// - `batch` — DNS results per step: `{"request_id","record_type","lookups","completed","total"}`
/// - `lint` — lint category results: `{"request_id","category","results"}`
/// - `done` — summary: `{"request_id","duration_ms","total_checks","passed","warnings","failed","not_found"}`
/// - `error` — non-fatal error: `{"code","message"}`
#[utoipa::path(
    post, path = "/api/check",
    tag = "Check",
    request_body = CheckRequest,
    responses(
        (status = 200, description = "SSE stream of DNS batch events, lint results, and done summary", content_type = "text/event-stream"),
        (status = 400, description = "Bad request (invalid domain or server)", body = ErrorResponse),
        (status = 422, description = "Query rejected by policy (private IP, limits exceeded)", body = ErrorResponse),
        (status = 429, description = "Rate limit exceeded", body = ErrorResponse),
    )
)]
pub async fn post_handler(
    State(state): State<AppState>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Query(stream_params): Query<StreamParams>,
    Json(body): Json<CheckRequest>,
) -> Result<Response, ApiError> {
    let domain = body.domain.to_ascii_lowercase();
    if domain.is_empty() {
        return Err(ApiError::InvalidDomain("empty domain".into()));
    }

    let mut servers = Vec::new();
    for name in &body.servers {
        let server = parse_server_spec(name)?;
        servers.push(server);
    }

    // Build a ParsedQuery for policy validation and resolver construction.
    // record_types is empty — validate_for_check skips the type-count check.
    let parsed = ParsedQuery {
        domain: domain.clone(),
        record_types: Vec::new(),
        servers,
        transport: None,
        dnssec: false,
        short: false,
        recursive: true,
        truncated_servers: false,
        warnings: Vec::new(),
    };

    let client_ip = state.ip_extractor.extract(&headers, peer_addr);
    tracing::debug!(%client_ip, %peer_addr, "check POST");

    let policy = QueryPolicy::new(&state.config);
    policy.validate_for_check(&parsed)?;

    // Clamp user-supplied timeout to config max; default to config max.
    let timeout_secs = body
        .timeout_secs
        .map(|t| t.min(state.config.limits.max_timeout_secs))
        .unwrap_or(state.config.limits.max_timeout_secs);
    let timeout = Duration::from_secs(timeout_secs);

    // Rate limiting: cost = 16 (15 base types + 1 DMARC) × server_count.
    let effective_servers = effective_server_specs(&parsed, &state.config);
    let target_keys = target_keys_from_servers(&effective_servers);
    let num_servers = effective_servers.len().max(1) as u32;
    let total_cost = CHECK_TOTAL_STEPS * num_servers;
    let stream_guard = state.hot_state.rate_limiter.load().check_query_cost(
        client_ip,
        &target_keys,
        total_cost,
        CHECK_TOTAL_STEPS,
    )?;

    let (resolver_group, breaker_keys) =
        build_resolver_group(&parsed, &state.config, timeout).await?;

    let resolvers = resolver_group.resolvers().to_vec();
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(32);
    let (done_tx, done_rx) = if !stream_params.stream {
        let (s, r) = tokio::sync::oneshot::channel::<Vec<crate::result_cache::CachedEvent>>();
        (Some(s), Some(r))
    } else {
        (None, None)
    };

    let rid = request_id.0;
    let circuit_breakers = state.circuit_breakers.clone();
    let result_cache = state.result_cache.clone();
    let query_string = domain.clone();
    let enrichment_svc = state.ip_enrichment.clone();
    let query_semaphore = state.query_semaphore.clone();

    tokio::spawn(async move {
        let _stream_guard = stream_guard;
        metrics::gauge!("prism_active_checks").increment(1.0);
        let start = Instant::now();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(STREAM_TIMEOUT_SECS);
        let mut cached_events: Vec<CachedEvent> = Vec::new();

        // ------------------------------------------------------------------
        // Phase 1: DNS lookups — all 16 types in parallel (FuturesUnordered)
        // ------------------------------------------------------------------
        let dmarc_domain = format!("_dmarc.{domain}");

        // Each future yields (record_type_label, Lookups, is_dmarc_lookup).
        type LookupFut =
            std::pin::Pin<Box<dyn std::future::Future<Output = (String, Lookups, bool)> + Send>>;
        let futs: FuturesUnordered<LookupFut> = FuturesUnordered::new();
        for rt in CHECK_RECORD_TYPES.iter() {
            let rt = *rt;
            let domain = domain.clone();
            let resolvers = resolvers.clone();
            let breaker_keys = breaker_keys.clone();
            let circuit_breakers = Arc::clone(&circuit_breakers);
            let tx_err = tx.clone();
            let semaphore = Arc::clone(&query_semaphore);
            futs.push(Box::pin(async move {
                let lookups = fan_out_lookup(
                    &domain,
                    rt,
                    &resolvers,
                    &breaker_keys,
                    &circuit_breakers,
                    &tx_err,
                    &semaphore,
                )
                .await;
                (rt.to_string(), lookups, false)
            }));
        }
        {
            let resolvers = resolvers.clone();
            let breaker_keys = breaker_keys.clone();
            let circuit_breakers = Arc::clone(&circuit_breakers);
            let tx_err = tx.clone();
            let semaphore = Arc::clone(&query_semaphore);
            futs.push(Box::pin(async move {
                let lookups = fan_out_lookup(
                    &dmarc_domain,
                    RecordType::TXT,
                    &resolvers,
                    &breaker_keys,
                    &circuit_breakers,
                    &tx_err,
                    &semaphore,
                )
                .await;
                ("_dmarc".to_string(), lookups, true)
            }));
        }

        tokio::pin!(futs);
        let mut all_lookups = Lookups::empty();
        let mut dmarc_lookups = Lookups::empty();
        let mut completed: u32 = 0;

        loop {
            tokio::select! {
                maybe = futs.next() => {
                    match maybe {
                        None => break,
                        Some((label, lookups, is_dmarc_lookup)) => {
                            completed += 1;
                            let batch = BatchEvent {
                                request_id: rid.clone(),
                                record_type: label,
                                lookups: lookups.clone(),
                                completed,
                                total: CHECK_TOTAL_STEPS,
                                transport: None,
                                source: None,
                            };
                            if let Ok(json_val) = serde_json::to_value(&batch) {
                                cached_events.push(CachedEvent {
                                    event_type: "batch".to_owned(),
                                    data: json_val,
                                });
                            }
                            let event = {
                                let mut v = serde_json::to_value(&batch)
                                    .unwrap_or(serde_json::Value::Null);
                                record_format::enrich_lookups_json(&mut v, &batch.record_type);
                                Event::default()
                                    .event("batch")
                                    .json_data(&v)
                                    .unwrap_or_else(|_| Event::default().event("batch").data("{}"))
                            };
                            if tx.send(Ok(event)).await.is_err() {
                                metrics::counter!("prism_queries_total", "endpoint" => "check", "status" => "error").increment(1);
                                metrics::gauge!("prism_active_checks").decrement(1.0);
                                return;
                            }
                            if is_dmarc_lookup {
                                dmarc_lookups = lookups;
                            } else {
                                all_lookups = all_lookups.merge(lookups);
                            }
                        }
                    }
                }
                _ = tokio::time::sleep_until(deadline) => {
                    let _ = tx
                        .send(Ok(make_error_event(
                            "STREAM_TIMEOUT",
                            "stream deadline exceeded",
                        )))
                        .await;
                    metrics::counter!("prism_queries_total", "endpoint" => "check", "status" => "error").increment(1);
                    metrics::gauge!("prism_active_checks").decrement(1.0);
                    return;
                }
            }
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
            let ips = extract_ips_from_cached_events(&cached_events);
            let map = svc.lookup_batch(&ips).await;
            if !map.is_empty() {
                use crate::api::query::EnrichmentEvent;
                let enrichment_event = EnrichmentEvent {
                    request_id: rid.clone(),
                    enrichments: map
                        .iter()
                        .map(|(ip, info)| (ip.to_string(), info.clone()))
                        .collect(),
                };
                if let Ok(json_val) = serde_json::to_value(&enrichment_event) {
                    cached_events.push(CachedEvent {
                        event_type: "enrichment".to_owned(),
                        data: json_val,
                    });
                }
                let event = Event::default()
                    .event("enrichment")
                    .json_data(&enrichment_event)
                    .unwrap_or_else(|_| Event::default().event("enrichment").data("{}"));
                let _ = tx.send(Ok(event)).await;
            }
            map
        } else {
            std::collections::HashMap::new()
        };

        // ------------------------------------------------------------------
        // Phase 1.75: Async NS checks (lame delegation + delegation consistency)
        // ------------------------------------------------------------------
        let query_timeout = Duration::from_secs(3);
        let lame_results = check_ns_lame_delegation(&all_lookups, &domain, query_timeout).await;
        let delegation_results = check_ns_delegation_consistency(&all_lookups, &domain, query_timeout).await;
        let dnssec_rollover_results = check_dnssec_rollover(&all_lookups);

        // ------------------------------------------------------------------
        // Phase 2: Lint checks (synchronous, pure)
        // ------------------------------------------------------------------
        let mut lint_checks: Vec<(&'static str, Vec<CheckResult>)> = vec![
            ("caa", check_caa(&all_lookups)),
            ("cname_apex", check_cname_apex(&all_lookups)),
            ("dnssec", check_dnssec(&all_lookups)),
            ("dnskey_algorithm", check_dnskey_algorithms(&all_lookups)),
            ("dnssec_rollover", dnssec_rollover_results),
            ("https_svcb", check_https_svcb_mode(&all_lookups)),
            ("mx", check_mx_sync(&all_lookups)),
            ("ns", check_ns_count(&all_lookups)),
            ("ns_lame", lame_results),
            ("ns_delegation", delegation_results),
            ("spf", check_spf(&all_lookups)),
            ("ttl", check_ttl(&all_lookups)),
            ("dmarc", check_dmarc_records(&dmarc_txts)),
        ];
        if !enrichment_map.is_empty() {
            lint_checks.push(("infrastructure", check_infrastructure(&enrichment_map)));
        }

        let mut total_checks: u32 = 0;
        let mut passed: u32 = 0;
        let mut lint_warnings: u32 = 0;
        let mut failed: u32 = 0;
        let mut not_found: u32 = 0;

        for (category, results) in lint_checks {
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
            if let Ok(json_val) = serde_json::to_value(&lint_event) {
                cached_events.push(CachedEvent {
                    event_type: "lint".to_owned(),
                    data: json_val,
                });
            }
            let event = Event::default()
                .event("lint")
                .json_data(&lint_event)
                .unwrap_or_else(|_| Event::default().event("lint").data("{}"));
            if tx.send(Ok(event)).await.is_err() {
                metrics::counter!("prism_queries_total", "endpoint" => "check", "status" => "error").increment(1);
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
        let cache_key = ResultCache::generate_key();
        let done = CheckDoneEvent {
            request_id: rid,
            duration_ms: elapsed.as_millis() as u64,
            total_checks,
            passed,
            warnings: lint_warnings,
            failed,
            not_found,
            cache_key: Some(cache_key.clone()),
        };
        if let Ok(done_val) = serde_json::to_value(&done) {
            cached_events.push(CachedEvent {
                event_type: "done".to_owned(),
                data: done_val,
            });
        }
        result_cache
            .insert(
                cache_key,
                CachedResult {
                    query: query_string,
                    mode: "check".to_owned(),
                    events: cached_events.clone(),
                },
            )
            .await;

        if let Some(dtx) = done_tx {
            let _ = dtx.send(cached_events);
        }

        let event = Event::default()
            .event("done")
            .json_data(&done)
            .unwrap_or_else(|_| Event::default().event("done").data("{}"));
        let _ = tx.send(Ok(event)).await;

        metrics::counter!("prism_queries_total", "endpoint" => "check", "status" => "ok")
            .increment(1);
        metrics::histogram!("prism_check_duration_seconds").record(elapsed.as_secs_f64());
        metrics::gauge!("prism_active_checks").decrement(1.0);
    });

    if let Some(drx) = done_rx {
        let timeout = Duration::from_secs(STREAM_TIMEOUT_SECS + 5);
        let (events, truncated) = match tokio::time::timeout(timeout, drx).await {
            Ok(Ok(cached)) => {
                let vals = cached
                    .into_iter()
                    .map(|e| serde_json::json!({"type": e.event_type, "data": e.data}))
                    .collect();
                (vals, false)
            }
            _ => (Vec::new(), true),
        };
        return Ok(axum::Json(CollectedResponse { events, truncated }).into_response());
    }

    let sse_stream = ReceiverStream::new(rx);

    Ok(Sse::new(sse_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ).into_response())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Fan out a single-type DNS query across all resolvers concurrently,
/// applying circuit breaker pre-checks. Returns merged lookups.
async fn fan_out_lookup(
    domain: &str,
    rt: RecordType,
    resolvers: &[Resolver],
    breaker_keys: &[String],
    circuit_breakers: &Arc<CircuitBreakerRegistry>,
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    semaphore: &Arc<Semaphore>,
) -> Lookups {
    let query = match MultiQuery::single(domain, rt) {
        Ok(q) => q,
        Err(e) => {
            tracing::warn!(domain = %domain, record_type = %rt, error = %e, "check query build failed");
            let _ = tx
                .send(Ok(make_error_event("RESOLVER_ERROR", &e.to_string())))
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
                .send(Ok(make_error_event(
                    "PROVIDER_DEGRADED",
                    &format!("circuit breaker open for {breaker_key} ({rt}), skipping"),
                )))
                .await;
            continue;
        }
        let r = resolver.clone();
        let q = query.clone();
        let sem = Arc::clone(semaphore);
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await;
            r.lookup(q).await
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
                    .send(Ok(make_error_event("RESOLVER_ERROR", &e.to_string())))
                    .await;
            }
            Err(e) => {
                tracing::error!(domain = %domain, record_type = %rt, error = %e, "resolver task panicked");
                let _ = tx
                    .send(Ok(make_error_event("INTERNAL_ERROR", &e.to_string())))
                    .await;
            }
        }
    }

    merged
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
                results.push(CheckResult::Failed(format!(
                    "DNSKEY uses RSAMD5 (algorithm 1) — deprecated, must not be used (RFC 8624)"
                )));
                any_deprecated = true;
            }
            DnssecAlgorithm::Dsa => {
                results.push(CheckResult::Failed(format!(
                    "DNSKEY uses DSA (algorithm 3) — deprecated, must not be used (RFC 8624)"
                )));
                any_deprecated = true;
            }
            DnssecAlgorithm::RsaSha1 => {
                results.push(CheckResult::Warning(format!(
                    "DNSKEY uses RSASHA1 (algorithm 5) — deprecated, should not be used (RFC 8624)"
                )));
                any_deprecated = true;
            }
            DnssecAlgorithm::RsaSha1Nsec3Sha1 => {
                results.push(CheckResult::Warning(format!(
                    "DNSKEY uses RSASHA1-NSEC3-SHA1 (algorithm 7) — deprecated, should not be used (RFC 8624)"
                )));
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

/// NS lame delegation check: query each authoritative NS server with RD=0
/// and check whether it answers authoritatively (AA=1) for the domain.
/// A lame server answers but with AA=0 — indicating misconfiguration.
async fn check_ns_lame_delegation(
    lookups: &Lookups,
    domain: &str,
    timeout: Duration,
) -> Vec<CheckResult> {
    let ns_names: Vec<String> = lookups
        .ns()
        .into_iter()
        .map(|n| n.to_ascii())
        .collect();

    if ns_names.is_empty() {
        return vec![CheckResult::NotFound()];
    }

    // Resolve NS hostnames to IPs.
    let mut ns_ips: HashMap<String, Vec<IpAddr>> = ns_names
        .iter()
        .map(|n| (n.clone(), Vec::new()))
        .collect();
    dns_raw::resolve_missing_glue(&mut ns_ips).await;

    let domain_name = match hickory_proto::rr::Name::from_ascii(domain) {
        Ok(n) => n,
        Err(_) => return vec![CheckResult::NotFound()],
    };

    let servers: Vec<SocketAddr> = ns_ips
        .values()
        .flat_map(|ips| ips.iter().filter(|ip| ip.is_ipv4()).copied())
        .map(|ip| SocketAddr::new(ip, 53))
        .collect();

    if servers.is_empty() {
        return vec![CheckResult::Warning(
            "NS lame delegation: could not resolve any NS server IPs for direct query".to_owned(),
        )];
    }

    let results_raw = dns_raw::parallel_queries(&servers, &domain_name, hickory_proto::rr::RecordType::SOA, timeout).await;

    let mut lame_servers: Vec<String> = Vec::new();
    let mut ok_count = 0usize;

    for qr in &results_raw {
        match &qr.result {
            Ok(resp) => {
                if resp.is_authoritative() {
                    ok_count += 1;
                } else {
                    lame_servers.push(qr.server.ip().to_string());
                }
            }
            Err(e) => {
                tracing::debug!(server = %qr.server, error = %e, "lame delegation check query failed");
            }
        }
    }

    if lame_servers.is_empty() && ok_count == 0 {
        return vec![CheckResult::Warning(
            "NS lame delegation: no servers responded to SOA query".to_owned(),
        )];
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
    results
}

/// NS delegation consistency check: compare NS names from recursive resolution
/// with NS names obtained by querying the zone's authoritative servers directly.
/// Inconsistency indicates stale delegation or incomplete NS synchronization.
async fn check_ns_delegation_consistency(
    lookups: &Lookups,
    domain: &str,
    timeout: Duration,
) -> Vec<CheckResult> {
    // Recursive NS names (already collected).
    let mut recursive_ns: Vec<String> = lookups
        .ns()
        .into_iter()
        .map(|n| n.to_ascii().to_ascii_lowercase())
        .collect();
    recursive_ns.sort();
    recursive_ns.dedup();

    if recursive_ns.is_empty() {
        return vec![CheckResult::NotFound()];
    }

    // Query the authoritative NS servers directly for NS records.
    let mut ns_ips: HashMap<String, Vec<IpAddr>> = recursive_ns
        .iter()
        .map(|n| (n.clone(), Vec::new()))
        .collect();
    dns_raw::resolve_missing_glue(&mut ns_ips).await;

    let servers: Vec<SocketAddr> = ns_ips
        .values()
        .flat_map(|ips| ips.iter().filter(|ip| ip.is_ipv4()).copied())
        .map(|ip| SocketAddr::new(ip, 53))
        .collect();

    if servers.is_empty() {
        return vec![CheckResult::Warning(
            "NS delegation consistency: could not resolve NS server IPs for direct query".to_owned(),
        )];
    }

    let domain_name = match hickory_proto::rr::Name::from_ascii(domain) {
        Ok(n) => n,
        Err(_) => return vec![CheckResult::NotFound()],
    };

    let results_raw = dns_raw::parallel_queries(&servers, &domain_name, hickory_proto::rr::RecordType::NS, timeout).await;

    // Collect NS names from direct (authoritative) responses.
    let mut auth_ns: Vec<String> = Vec::new();
    for qr in &results_raw {
        if let Ok(resp) = &qr.result {
            for record in resp.answers() {
                if let hickory_proto::rr::RData::NS(ns) = record.data() {
                    let name = ns.0.to_ascii().to_ascii_lowercase();
                    if !auth_ns.contains(&name) {
                        auth_ns.push(name);
                    }
                }
            }
        }
    }

    if auth_ns.is_empty() {
        return vec![CheckResult::Warning(
            "NS delegation consistency: no NS records returned from direct authoritative query".to_owned(),
        )];
    }

    auth_ns.sort();
    auth_ns.dedup();

    if recursive_ns == auth_ns {
        vec![CheckResult::Ok(format!(
            "NS delegation is consistent: {} name server(s) match between parent and child",
            recursive_ns.len()
        ))]
    } else {
        let only_recursive: Vec<&String> = recursive_ns.iter().filter(|n| !auth_ns.contains(n)).collect();
        let only_auth: Vec<&String> = auth_ns.iter().filter(|n| !recursive_ns.contains(n)).collect();

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
        results
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
        if !ksk_tags.contains(&ds_tag) && !dnskeys.iter().filter_map(|k| k.key_tag()).any(|t| t == ds_tag) {
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
