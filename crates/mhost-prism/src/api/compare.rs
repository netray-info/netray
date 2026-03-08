//! Transport comparison endpoint: query over all 4 transports and compare answers.
//!
//! - `POST /api/compare` — accept a structured JSON body.
//!
//! For each transport (UDP, TCP, TLS, HTTPS), builds a resolver group and fans out
//! per-record-type lookups. Each batch event includes the transport that produced it.
//! Providers that don't support a transport are skipped with a warning.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::{FuturesUnordered, Stream, StreamExt};
use mhost::resolver::MultiQuery;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::RequestId;
use crate::api::query::{
    PostQueryRequest, build_resolver_group, convert_post_body, effective_server_specs,
    extract_ips_from_cached_events, make_error_event, record_breaker_outcomes,
    send_enrichment_event, target_keys_from_servers,
};
use crate::api::{AppState, BatchEvent, STREAM_TIMEOUT_SECS};
use crate::error::{ApiError, ErrorResponse};
use crate::parser::Transport;
use crate::record_format;
use crate::result_cache::{CachedEvent, CachedResult, ResultCache};
use crate::security::QueryPolicy;

// All four transports to compare.
const ALL_TRANSPORTS: [Transport; 4] = [
    Transport::Udp,
    Transport::Tcp,
    Transport::Tls,
    Transport::Https,
];

fn transport_name(t: Transport) -> &'static str {
    match t {
        Transport::Udp => "udp",
        Transport::Tcp => "tcp",
        Transport::Tls => "tls",
        Transport::Https => "https",
    }
}

// ---------------------------------------------------------------------------
// SSE event payloads
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct CompareDoneEvent {
    request_id: String,
    total_queries: u32,
    duration_ms: u64,
    warnings: Vec<String>,
    transports: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_key: Option<String>,
}

// ---------------------------------------------------------------------------
// POST handler
// ---------------------------------------------------------------------------

/// Compare DNS query results across all four transports (UDP, TCP, TLS, HTTPS).
///
/// For each transport, builds resolvers and fans out per-record-type lookups.
/// Batch events include a `transport` field indicating which transport produced
/// the result. Providers that don't support a given transport are skipped.
///
/// ## SSE Events
///
/// - `batch` — per record type per transport: includes `transport` field
/// - `done` — summary with `transports` listing available transports
/// - `error` — non-fatal error
#[utoipa::path(
    post, path = "/api/compare",
    tag = "Query",
    request_body = PostQueryRequest,
    responses(
        (status = 200, description = "SSE stream of transport comparison results", content_type = "text/event-stream"),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 422, description = "Query rejected by policy", body = ErrorResponse),
        (status = 429, description = "Rate limit exceeded", body = ErrorResponse),
    )
)]
pub async fn post_handler(
    State(state): State<AppState>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    raw_query: axum::extract::RawQuery,
    Json(body): Json<PostQueryRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    if raw_query.0.is_some() {
        return Err(ApiError::AmbiguousInput);
    }

    let query_display = body.domain.clone();
    // Strip any transport override — we'll iterate all transports ourselves.
    let mut parsed = convert_post_body(PostQueryRequest {
        transport: None,
        ..body
    })?;

    let client_ip = state.ip_extractor.extract(&headers, peer_addr);
    tracing::debug!(%client_ip, %peer_addr, "compare POST");

    let policy = QueryPolicy::new(&state.config);
    policy.validate(&parsed)?;

    let timeout = Duration::from_secs(state.config.limits.max_timeout_secs);

    // Probe which transports are available (have at least one resolver).
    let mut available_transports: Vec<Transport> = Vec::new();
    for t in ALL_TRANSPORTS {
        let mut probe = parsed.clone();
        probe.transport = Some(t);
        match build_resolver_group(&probe, &state.config, timeout).await {
            Ok((rg, _)) if !rg.resolvers().is_empty() => {
                available_transports.push(t);
            }
            _ => {}
        }
    }

    if available_transports.is_empty() {
        return Err(ApiError::ResolverError(
            "no transports available for the specified servers".into(),
        ));
    }

    // Rate limiting: cost = record_types * servers * available_transports.
    let effective_servers = effective_server_specs(&parsed, &state.config);
    let target_keys = target_keys_from_servers(&effective_servers);
    let num_types = parsed.record_types.len() as u32;
    let num_servers = effective_servers.len().max(1) as u32;
    let num_transports = available_transports.len() as u32;
    let total_cost = num_types * num_servers * num_transports;
    let stream_guard =
        state
            .rate_limiter
            .check_query_cost(client_ip, &target_keys, total_cost, num_types)?;

    // DNSSEC augmentation.
    if parsed.dnssec {
        for rt in [mhost::RecordType::DNSKEY, mhost::RecordType::DS] {
            if !parsed.record_types.contains(&rt) {
                parsed.record_types.push(rt);
            }
        }
    }

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(64);
    let rid = request_id.0;
    let warnings = parsed.warnings.clone();
    let record_types = parsed.record_types.clone();
    let domain = parsed.domain.clone();
    let circuit_breakers = state.circuit_breakers.clone();
    let result_cache = state.result_cache.clone();
    let enrichment_svc = state.ip_enrichment.clone();
    let config = state.config.clone();
    let total_batches = (record_types.len() as u32) * num_transports;

    tokio::spawn(async move {
        let _stream_guard = stream_guard;
        metrics::gauge!("prism_active_compares").increment(1.0);
        let start = Instant::now();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(STREAM_TIMEOUT_SECS);
        let mut cached_events: Vec<CachedEvent> = Vec::new();
        let mut completed: u32 = 0;
        let mut actual_transports: Vec<String> = Vec::new();
        let mut all_warnings = warnings;

        // For each available transport, build resolvers and run all record types.
        // Use FuturesUnordered to run transports concurrently.
        type TransportResult = (
            Transport,
            Vec<(mhost::RecordType, mhost::resolver::Lookups, bool)>,
        );
        let transport_futs: FuturesUnordered<
            std::pin::Pin<Box<dyn std::future::Future<Output = TransportResult> + Send>>,
        > = FuturesUnordered::new();

        for t in &available_transports {
            let t = *t;
            let mut probe = parsed.clone();
            probe.transport = Some(t);
            let config = config.clone();
            let circuit_breakers = Arc::clone(&circuit_breakers);
            let tx_err = tx.clone();
            let record_types = record_types.clone();
            let domain = domain.clone();

            transport_futs.push(Box::pin(async move {
                let (resolver_group, breaker_keys) =
                    match build_resolver_group(&probe, &config, timeout).await {
                        Ok(rg) => rg,
                        Err(e) => {
                            let _ = tx_err
                                .send(Ok(make_error_event("RESOLVER_ERROR", &e.to_string())))
                                .await;
                            return (t, Vec::new());
                        }
                    };
                let resolvers: Vec<mhost::resolver::Resolver> = resolver_group.resolvers().to_vec();
                if resolvers.is_empty() {
                    let _ = tx_err
                        .send(Ok(make_error_event(
                            "TRANSPORT_UNAVAILABLE",
                            &format!("no resolvers support {}", transport_name(t)),
                        )))
                        .await;
                    return (t, Vec::new());
                }

                // Fan out per record type.
                let rt_futs: FuturesUnordered<_> = record_types
                    .iter()
                    .map(|rt| {
                        let rt = *rt;
                        let domain = domain.clone();
                        let resolvers = resolvers.clone();
                        let breaker_keys = breaker_keys.clone();
                        let circuit_breakers = Arc::clone(&circuit_breakers);
                        let tx_err = tx_err.clone();
                        async move {
                            let query = match MultiQuery::single(domain.as_str(), rt) {
                                Ok(q) => q,
                                Err(e) => {
                                    let _ = tx_err
                                        .send(Ok(make_error_event(
                                            "RESOLVER_ERROR",
                                            &e.to_string(),
                                        )))
                                        .await;
                                    return (rt, mhost::resolver::Lookups::empty(), true);
                                }
                            };

                            let mut handles = Vec::with_capacity(resolvers.len());
                            for (idx, resolver) in resolvers.iter().enumerate() {
                                let breaker_key = &breaker_keys[idx];
                                if let Err(crate::circuit_breaker::BreakerState::Open) =
                                    circuit_breakers.check(breaker_key)
                                {
                                    continue;
                                }
                                let r = resolver.clone();
                                let q = query.clone();
                                handles.push(tokio::spawn(async move { r.lookup(q).await }));
                            }

                            let mut merged = mhost::resolver::Lookups::empty();
                            let mut had_error = false;
                            for handle in handles {
                                match handle.await {
                                    Ok(Ok(lookups)) => {
                                        record_breaker_outcomes(&circuit_breakers, &lookups);
                                        merged = merged.merge(lookups);
                                    }
                                    Ok(Err(e)) => {
                                        had_error = true;
                                        let _ = tx_err
                                            .send(Ok(make_error_event(
                                                "RESOLVER_ERROR",
                                                &e.to_string(),
                                            )))
                                            .await;
                                    }
                                    Err(e) => {
                                        had_error = true;
                                        let _ = tx_err
                                            .send(Ok(make_error_event(
                                                "INTERNAL_ERROR",
                                                &e.to_string(),
                                            )))
                                            .await;
                                    }
                                }
                            }
                            (rt, merged, had_error)
                        }
                    })
                    .collect();

                let results: Vec<_> = rt_futs.collect().await;
                (t, results)
            }));
        }

        tokio::pin!(transport_futs);
        let mut timed_out = false;

        loop {
            tokio::select! {
                maybe = transport_futs.next() => {
                    match maybe {
                        None => break,
                        Some((transport, results)) => {
                            if results.is_empty() {
                                all_warnings.push(format!("no resolvers available for {}", transport_name(transport)));
                                continue;
                            }
                            actual_transports.push(transport_name(transport).to_owned());

                            for (rt, merged, _had_error) in results {
                                completed += 1;
                                let batch = BatchEvent {
                                    request_id: rid.clone(),
                                    record_type: rt.to_string(),
                                    lookups: merged,
                                    completed,
                                    total: total_batches,
                                    transport: Some(transport_name(transport).to_owned()),
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
                                    metrics::gauge!("prism_active_compares").decrement(1.0);
                                    return;
                                }
                            }
                        }
                    }
                }
                _ = tokio::time::sleep_until(deadline) => {
                    timed_out = true;
                    let _ = tx.send(Ok(make_error_event("STREAM_TIMEOUT", "stream deadline exceeded"))).await;
                    break;
                }
            }
        }

        if !timed_out {
            // Enrichment.
            if let Some(ref svc) = enrichment_svc {
                let ips = extract_ips_from_cached_events(&cached_events);
                send_enrichment_event(svc, &ips, &rid, &tx, &mut cached_events).await;
            }

            let elapsed = start.elapsed();
            tracing::info!(
                request_id = %rid,
                domain = %domain,
                duration_ms = elapsed.as_millis(),
                transports = ?actual_transports,
                "compare completed"
            );

            actual_transports.sort();
            let cache_key = ResultCache::generate_key();
            let done = CompareDoneEvent {
                request_id: rid,
                total_queries: completed,
                duration_ms: elapsed.as_millis() as u64,
                warnings: all_warnings,
                transports: actual_transports,
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
                        query: query_display,
                        mode: "compare".to_owned(),
                        events: cached_events,
                    },
                )
                .await;

            let event = Event::default()
                .event("done")
                .json_data(&done)
                .unwrap_or_else(|_| Event::default().event("done").data("{}"));
            let _ = tx.send(Ok(event)).await;

            metrics::histogram!("prism_compare_duration_seconds").record(elapsed.as_secs_f64());
        }

        metrics::gauge!("prism_active_compares").decrement(1.0);
    });

    let stream = ReceiverStream::new(rx);

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}
