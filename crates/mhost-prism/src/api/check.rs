//! Check endpoint: run DNS lookups and lint analysis for a domain, streaming results as SSE.
//!
//! - `POST /api/check` — accept a structured JSON body with a domain and optional servers.
//!
//! Phase 1 streams batch events (one per record type queried).
//! Phase 2 streams lint events (one per lint category, synchronous pure checks).
//! Phase 3 sends a done event with summary counts.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::{FuturesUnordered, Stream, StreamExt};
use mhost::RecordType;
use mhost::lints::{
    CheckResult, check_caa, check_cname_apex, check_dmarc_records, check_dnssec,
    check_https_svcb_mode, check_mx_sync, check_ns_count, check_spf, check_ttl, is_dmarc,
};
use mhost::resolver::lookup::Uniquify;
use mhost::resolver::{Lookups, MultiQuery, Resolver};
use mhost::resources::rdata::TXT;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::api::{AppState, BatchEvent, STREAM_TIMEOUT_SECS};
use crate::api::query::{
    build_resolver_group, effective_server_specs, make_error_event, parse_server_spec,
    record_breaker_outcomes, target_keys_from_servers,
};
use crate::circuit_breaker::{BreakerState, CircuitBreakerRegistry};
use crate::error::{ApiError, ErrorResponse};
use crate::parser::ParsedQuery;
use crate::security::QueryPolicy;
use crate::RequestId;

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
    raw_query: axum::extract::RawQuery,
    Json(body): Json<CheckRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    // Reject POST with a query string -- ambiguous input.
    if raw_query.0.is_some() {
        return Err(ApiError::AmbiguousInput);
    }

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
    let stream_guard = state.rate_limiter.check_query_cost(
        client_ip,
        &target_keys,
        total_cost,
        CHECK_TOTAL_STEPS,
    )?;

    let (resolver_group, breaker_keys) =
        build_resolver_group(&parsed, &state.config, timeout).await?;

    let resolvers = resolver_group.resolvers().to_vec();
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(32);

    let rid = request_id.0;
    let circuit_breakers = state.circuit_breakers.clone();

    tokio::spawn(async move {
        let _stream_guard = stream_guard;
        metrics::gauge!("prism_active_checks").increment(1.0);
        let start = Instant::now();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(STREAM_TIMEOUT_SECS);

        // ------------------------------------------------------------------
        // Phase 1: DNS lookups — all 16 types in parallel (FuturesUnordered)
        // ------------------------------------------------------------------
        let dmarc_domain = format!("_dmarc.{domain}");

        // Each future yields (record_type_label, Lookups, is_dmarc_lookup).
        type LookupFut = std::pin::Pin<
            Box<dyn std::future::Future<Output = (String, Lookups, bool)> + Send>,
        >;
        let futs: FuturesUnordered<LookupFut> = FuturesUnordered::new();
        for rt in CHECK_RECORD_TYPES.iter() {
            let rt = *rt;
            let domain = domain.clone();
            let resolvers = resolvers.clone();
            let breaker_keys = breaker_keys.clone();
            let circuit_breakers = Arc::clone(&circuit_breakers);
            let tx_err = tx.clone();
            futs.push(Box::pin(async move {
                let lookups = fan_out_lookup(
                    &domain,
                    rt,
                    &resolvers,
                    &breaker_keys,
                    &circuit_breakers,
                    &tx_err,
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
            futs.push(Box::pin(async move {
                let lookups = fan_out_lookup(
                    &dmarc_domain,
                    RecordType::TXT,
                    &resolvers,
                    &breaker_keys,
                    &circuit_breakers,
                    &tx_err,
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
                            };
                            let event = Event::default()
                                .event("batch")
                                .json_data(&batch)
                                .unwrap_or_else(|_| Event::default().event("batch").data("{}"));
                            if tx.send(Ok(event)).await.is_err() {
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
        // Phase 2: Lint checks (synchronous, pure)
        // ------------------------------------------------------------------
        let lint_checks: Vec<(&'static str, Vec<CheckResult>)> = vec![
            ("caa", check_caa(&all_lookups)),
            ("cname_apex", check_cname_apex(&all_lookups)),
            ("dnssec", check_dnssec(&all_lookups)),
            ("https_svcb", check_https_svcb_mode(&all_lookups)),
            ("mx", check_mx_sync(&all_lookups)),
            ("ns", check_ns_count(&all_lookups)),
            ("spf", check_spf(&all_lookups)),
            ("ttl", check_ttl(&all_lookups)),
            ("dmarc", check_dmarc_records(&dmarc_txts)),
        ];

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
            let event = Event::default()
                .event("lint")
                .json_data(&lint_event)
                .unwrap_or_else(|_| Event::default().event("lint").data("{}"));
            if tx.send(Ok(event)).await.is_err() {
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
        };
        let event = Event::default()
            .event("done")
            .json_data(&done)
            .unwrap_or_else(|_| Event::default().event("done").data("{}"));
        let _ = tx.send(Ok(event)).await;

        metrics::histogram!("prism_check_duration_seconds").record(elapsed.as_secs_f64());
        metrics::gauge!("prism_active_checks").decrement(1.0);
    });

    let stream = ReceiverStream::new(rx);

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
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
) -> Lookups {
    let query = match MultiQuery::single(domain, rt) {
        Ok(q) => q,
        Err(e) => {
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
            let _ = tx
                .send(Ok(make_error_event(
                    "PROVIDER_DEGRADED",
                    &format!("circuit breaker open for {breaker_key}, skipping"),
                )))
                .await;
            continue;
        }
        let r = resolver.clone();
        let q = query.clone();
        handles.push(tokio::spawn(async move { r.lookup(q).await }));
    }

    let mut merged = Lookups::empty();
    for handle in handles {
        match handle.await {
            Ok(Ok(lookups)) => {
                record_breaker_outcomes(circuit_breakers, &lookups);
                merged = merged.merge(lookups);
            }
            Ok(Err(e)) => {
                let _ = tx
                    .send(Ok(make_error_event("RESOLVER_ERROR", &e.to_string())))
                    .await;
            }
            Err(e) => {
                let _ = tx
                    .send(Ok(make_error_event("INTERNAL_ERROR", &e.to_string())))
                    .await;
            }
        }
    }

    merged
}
