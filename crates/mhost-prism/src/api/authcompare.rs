//! Authoritative-vs-Recursive comparison endpoint.
//!
//! - `POST /api/authcompare` — discover authoritative NS for a domain, then
//!   query both the authoritative servers (RD=0) and configured recursive
//!   resolvers in parallel, streaming comparison results.
//!
//! Phase 1: Discover authoritative nameservers via recursive NS lookup.
//! Phase 2: Fan out per-record-type lookups to both auth (RD=0) and recursive
//!           resolvers concurrently. Each batch event includes `source` field
//!           ("authoritative" or "recursive").

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::{FuturesUnordered, Stream, StreamExt};
use hickory_proto::rr::{Name, RecordType};
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
use crate::dns_raw;
use crate::error::{ApiError, ErrorResponse};
use crate::record_format;
use crate::result_cache::{CachedEvent, CachedResult, ResultCache};
use crate::security::QueryPolicy;

// ---------------------------------------------------------------------------
// SSE event payloads
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct AuthCompareDoneEvent {
    request_id: String,
    total_queries: u32,
    duration_ms: u64,
    warnings: Vec<String>,
    /// Discovered authoritative nameservers, e.g. ["ns1.example.com (1.2.3.4)"].
    auth_servers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_key: Option<String>,
}

// ---------------------------------------------------------------------------
// POST handler
// ---------------------------------------------------------------------------

/// Compare DNS results from authoritative nameservers vs recursive resolvers.
///
/// Phase 1 discovers the domain's authoritative NS via recursive resolution.
/// Phase 2 queries both auth (RD=0) and recursive resolvers per record type.
///
/// ## SSE Events
///
/// - `batch` — per record type per source: includes `source` field ("authoritative" or "recursive")
/// - `done` — summary with `auth_servers` listing discovered nameservers
/// - `error` — non-fatal error
#[utoipa::path(
    post, path = "/api/authcompare",
    tag = "Query",
    request_body = PostQueryRequest,
    responses(
        (status = 200, description = "SSE stream of auth-vs-recursive comparison results", content_type = "text/event-stream"),
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
    let parsed = convert_post_body(body)?;

    let client_ip = state.ip_extractor.extract(&headers, peer_addr);
    tracing::debug!(%client_ip, %peer_addr, "authcompare POST");

    let policy = QueryPolicy::new(&state.config);
    policy.validate(&parsed)?;

    let timeout = Duration::from_secs(state.config.limits.max_timeout_secs);

    // Rate limiting: cost = record_types * servers * 2 (auth + recursive) + 16 (NS discovery).
    let effective_servers = effective_server_specs(&parsed, &state.config);
    let target_keys = target_keys_from_servers(&effective_servers);
    let num_types = parsed.record_types.len() as u32;
    let num_servers = effective_servers.len().max(1) as u32;
    let total_cost = num_types * num_servers * 2 + 16;
    let stream_guard =
        state
            .rate_limiter
            .check_query_cost(client_ip, &target_keys, total_cost, num_types)?;

    let domain = parsed.domain.clone();
    let trace_timeout = Duration::from_secs(state.config.trace.query_timeout_secs);

    // ------------------------------------------------------------------
    // Phase 1: Discover authoritative NS (before spawn — ResolverGroup is !Send)
    // ------------------------------------------------------------------

    let domain_name =
        Name::from_ascii(&domain).map_err(|e| ApiError::InvalidDomain(e.to_string()))?;

    let (_resolver_group, _breaker_keys) =
        build_resolver_group(&parsed, &state.config, timeout).await?;

    // Extract individual resolvers (Send) for the recursive branch.
    let resolvers: Vec<mhost::resolver::Resolver> = _resolver_group.resolvers().to_vec();

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(64);
    let rid = request_id.0;
    let warnings = parsed.warnings.clone();
    let record_types: Vec<mhost::RecordType> = parsed.record_types.clone();
    let circuit_breakers = state.circuit_breakers.clone();
    let result_cache = state.result_cache.clone();
    let enrichment_svc = state.ip_enrichment.clone();

    tokio::spawn(async move {
        let _stream_guard = stream_guard;
        metrics::gauge!("prism_active_authcompares").increment(1.0);
        let start = Instant::now();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(STREAM_TIMEOUT_SECS);
        let mut cached_events: Vec<CachedEvent> = Vec::new();
        let mut completed: u32 = 0;
        let mut all_warnings = warnings;

        // ------------------------------------------------------------------
        // Phase 1: Discover authoritative NS (inside spawned task)
        // ------------------------------------------------------------------

        // Query NS records using individual resolvers (Send-safe).
        let ns_query = match MultiQuery::single(domain.as_str(), mhost::RecordType::NS) {
            Ok(q) => q,
            Err(e) => {
                let _ = tx
                    .send(Ok(make_error_event("RESOLVER_ERROR", &e.to_string())))
                    .await;
                metrics::gauge!("prism_active_authcompares").decrement(1.0);
                return;
            }
        };

        let mut ns_lookups = mhost::resolver::Lookups::empty();
        for resolver in &resolvers {
            let r = resolver.clone();
            let q = ns_query.clone();
            match tokio::spawn(async move { r.lookup(q).await }).await {
                Ok(Ok(lookups)) => {
                    ns_lookups = ns_lookups.merge(lookups);
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

        // Extract NS hostnames from response by serializing to JSON.
        let mut ns_names: Vec<String> = Vec::new();
        if let Ok(lookups_json) = serde_json::to_value(&ns_lookups)
            && let Some(lookups_arr) = lookups_json.get("lookups").and_then(|v| v.as_array())
        {
            for lookup in lookups_arr {
                if let Some(resp) = lookup.get("result").and_then(|r| r.get("Response"))
                    && let Some(records) = resp.get("records").and_then(|r| r.as_array())
                {
                    for record in records {
                        if let Some(ns_val) = record
                            .get("data")
                            .and_then(|d| d.get("NS"))
                            .and_then(|v| v.as_str())
                        {
                            let name = ns_val.to_string();
                            if !ns_names.contains(&name) {
                                ns_names.push(name);
                            }
                        }
                    }
                }
            }
        }

        if ns_names.is_empty() {
            all_warnings.push("no NS records found — auth comparison unavailable".to_owned());
            let _ = tx
                .send(Ok(make_error_event(
                    "NO_NS_RECORDS",
                    "could not discover authoritative nameservers",
                )))
                .await;
        }

        // Resolve NS hostnames to IPs.
        let mut ns_ips: HashMap<String, Vec<IpAddr>> = HashMap::new();
        for ns in &ns_names {
            ns_ips.insert(ns.clone(), Vec::new());
        }
        dns_raw::resolve_missing_glue(&mut ns_ips).await;

        // Build auth server list (IPv4 only, port 53).
        let auth_servers: Vec<SocketAddr> = ns_ips
            .iter()
            .flat_map(|(_, ips)| ips.iter().filter(|ip| ip.is_ipv4()).copied())
            .map(|ip| SocketAddr::new(ip, 53))
            .collect();

        // Build labels for done event.
        let mut auth_server_labels: Vec<String> = Vec::new();
        for (ns_name, ips) in &ns_ips {
            for ip in ips {
                if ip.is_ipv4() {
                    auth_server_labels.push(format!("{ns_name} ({ip})"));
                }
            }
        }

        let has_auth = !auth_servers.is_empty();
        let total_batches = if has_auth {
            record_types.len() as u32 * 2
        } else {
            record_types.len() as u32
        };

        // ------------------------------------------------------------------
        // Phase 2: Parallel fan-out (auth + recursive)
        // ------------------------------------------------------------------

        let mut timed_out = false;

        type BranchResult = Vec<(mhost::RecordType, mhost::resolver::Lookups, &'static str)>;
        let futs: FuturesUnordered<
            std::pin::Pin<Box<dyn std::future::Future<Output = BranchResult> + Send>>,
        > = FuturesUnordered::new();

        // Recursive branch.
        {
            let resolvers = resolvers;
            let record_types = record_types.clone();
            let domain = domain.clone();
            let circuit_breakers = Arc::clone(&circuit_breakers);
            let tx_err = tx.clone();

            futs.push(Box::pin(async move {
                let rt_futs: FuturesUnordered<_> = record_types
                    .iter()
                    .map(|rt| {
                        let rt = *rt;
                        let domain = domain.clone();
                        let resolvers = resolvers.clone();
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
                                    return (rt, mhost::resolver::Lookups::empty(), "recursive");
                                }
                            };

                            let mut handles = Vec::with_capacity(resolvers.len());
                            for resolver in &resolvers {
                                let r = resolver.clone();
                                let q = query.clone();
                                handles.push(tokio::spawn(async move { r.lookup(q).await }));
                            }

                            let mut merged = mhost::resolver::Lookups::empty();
                            for handle in handles {
                                match handle.await {
                                    Ok(Ok(lookups)) => {
                                        record_breaker_outcomes(&circuit_breakers, &lookups);
                                        merged = merged.merge(lookups);
                                    }
                                    Ok(Err(e)) => {
                                        let _ = tx_err
                                            .send(Ok(make_error_event(
                                                "RESOLVER_ERROR",
                                                &e.to_string(),
                                            )))
                                            .await;
                                    }
                                    Err(e) => {
                                        let _ = tx_err
                                            .send(Ok(make_error_event(
                                                "INTERNAL_ERROR",
                                                &e.to_string(),
                                            )))
                                            .await;
                                    }
                                }
                            }
                            (rt, merged, "recursive")
                        }
                    })
                    .collect();

                rt_futs.collect().await
            }));
        }

        // Authoritative branch (only if we have auth servers).
        if has_auth {
            let auth_servers = auth_servers.clone();
            let record_types = record_types.clone();
            let domain_name = domain_name.clone();
            let tx_err = tx.clone();

            futs.push(Box::pin(async move {
                let rt_futs: FuturesUnordered<_> = record_types
                    .iter()
                    .map(|rt| {
                        let rt = *rt;
                        let domain_name = domain_name.clone();
                        let auth_servers = auth_servers.clone();
                        let tx_err = tx_err.clone();
                        async move {
                            // Convert mhost RecordType to hickory RecordType.
                            let hickory_rt: RecordType = hickory_proto::rr::RecordType::from(rt);

                            let results = dns_raw::parallel_queries(
                                &auth_servers,
                                &domain_name,
                                hickory_rt,
                                trace_timeout,
                            )
                            .await;

                            // Convert raw responses to mhost Lookups format.
                            let mut lookups_vec = Vec::new();
                            for qr in &results {
                                match &qr.result {
                                    Ok(raw_resp) => {
                                        let response_time = raw_resp.latency;
                                        let records: Vec<_> = raw_resp
                                            .answers()
                                            .iter()
                                            .map(dns_raw::record_to_dns_record)
                                            .collect();

                                        // Build a synthetic lookup result.
                                        let ns_name = format!("{}", qr.server.ip());
                                        let result = if raw_resp.response_code()
                                            == hickory_proto::op::ResponseCode::NXDomain
                                        {
                                            serde_json::json!({
                                                "NxDomain": {
                                                    "response_time": {
                                                        "secs": response_time.as_secs(),
                                                        "nanos": response_time.subsec_nanos()
                                                    }
                                                }
                                            })
                                        } else {
                                            let records_json: Vec<serde_json::Value> = records
                                                .iter()
                                                .map(|r| {
                                                    serde_json::json!({
                                                        "data": {
                                                            r.record_type.clone(): r.rdata.clone()
                                                        },
                                                        "name": r.name,
                                                        "ttl": r.ttl,
                                                        "record_type": r.record_type
                                                    })
                                                })
                                                .collect();
                                            serde_json::json!({
                                                "Response": {
                                                    "records": records_json,
                                                    "response_time": {
                                                        "secs": response_time.as_secs(),
                                                        "nanos": response_time.subsec_nanos()
                                                    }
                                                }
                                            })
                                        };

                                        lookups_vec.push(serde_json::json!({
                                            "query": {
                                                "name": domain_name.to_ascii(),
                                                "record_type": rt.to_string()
                                            },
                                            "name_server": ns_name,
                                            "result": result
                                        }));
                                    }
                                    Err(e) => {
                                        let _ = tx_err
                                            .send(Ok(make_error_event(
                                                "AUTH_QUERY_ERROR",
                                                &format!("{}: {e}", qr.server),
                                            )))
                                            .await;
                                    }
                                }
                            }

                            // Build a Lookups from JSON (roundtrip through serde).
                            let lookups_json = serde_json::json!({ "lookups": lookups_vec });
                            let lookups: mhost::resolver::Lookups =
                                serde_json::from_value(lookups_json).unwrap_or_else(|e| {
                                    tracing::warn!("failed to deserialize auth lookups: {e}");
                                    mhost::resolver::Lookups::empty()
                                });

                            (rt, lookups, "authoritative")
                        }
                    })
                    .collect();

                rt_futs.collect().await
            }));
        }

        tokio::pin!(futs);

        loop {
            tokio::select! {
                maybe = futs.next() => {
                    match maybe {
                        None => break,
                        Some(results) => {
                            for (rt, merged, source) in results {
                                completed += 1;
                                let batch = BatchEvent {
                                    request_id: rid.clone(),
                                    record_type: rt.to_string(),
                                    lookups: merged,
                                    completed,
                                    total: total_batches,
                                    transport: None,
                                    source: Some(source.to_owned()),
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
                                    metrics::gauge!("prism_active_authcompares").decrement(1.0);
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
                auth_servers = ?auth_server_labels,
                "authcompare completed"
            );

            let cache_key = ResultCache::generate_key();
            let done = AuthCompareDoneEvent {
                request_id: rid,
                total_queries: completed,
                duration_ms: elapsed.as_millis() as u64,
                warnings: all_warnings,
                auth_servers: auth_server_labels,
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
                        mode: "auth".to_owned(),
                        events: cached_events,
                    },
                )
                .await;

            let event = Event::default()
                .event("done")
                .json_data(&done)
                .unwrap_or_else(|_| Event::default().event("done").data("{}"));
            let _ = tx.send(Ok(event)).await;

            metrics::histogram!("prism_authcompare_duration_seconds").record(elapsed.as_secs_f64());
        }

        metrics::gauge!("prism_active_authcompares").decrement(1.0);
    });

    let stream = ReceiverStream::new(rx);

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}
