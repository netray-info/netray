//! Query endpoint: parse, validate, execute DNS lookups, and stream results as SSE.
//!
//! - `GET /api/query?q=...` — parse a dig-inspired query string.
//! - `POST /api/query` — accept a structured JSON body.
//!
//! Both handlers validate the parsed query against [`QueryPolicy`], build a set
//! of resolvers from the server specs, fan out per-record-type lookups across all
//! resolvers concurrently, and stream batch results as SSE events.

use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures::stream::{FuturesUnordered, StreamExt};
use mhost::RecordType;
use mhost::nameserver::NameServerConfig;
use mhost::nameserver::predefined::PredefinedProvider;
use mhost::resolver::{Lookups, MultiQuery, Resolver, ResolverGroup, ResolverGroupBuilder};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::RequestId;
use crate::api::{AppState, BatchEvent, STREAM_TIMEOUT_SECS};
use crate::circuit_breaker::CircuitBreakerRegistry;
use crate::config::Config;
use crate::error::{ApiError, ErrorResponse};
use crate::parser::{self, ParsedQuery, ServerSpec, Transport};
use crate::record_format;
use crate::result_cache::{CachedEvent, CachedResult, ResultCache};
use crate::security::QueryPolicy;
use netray_common::enrichment::{EnrichmentClient, IpInfo};

// ---------------------------------------------------------------------------
// SSE event payloads
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct DoneEvent {
    request_id: String,
    total_queries: u32,
    duration_ms: u64,
    warnings: Vec<String>,
    /// Transport used for this query (e.g., "udp", "tls").
    transport: String,
    /// Whether DNSSEC mode was requested.
    dnssec: bool,
    /// Providers skipped due to open circuit breakers during this query.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    degraded_providers: Vec<String>,
    /// Cache key for retrieving this result via permalink.
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_key: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct EnrichmentEvent {
    pub request_id: String,
    pub enrichments: std::collections::HashMap<String, IpInfo>,
}

#[derive(Serialize)]
struct ErrorEvent {
    code: String,
    message: String,
}

/// Extract IP addresses from cached batch events (A/AAAA record values).
pub(crate) fn extract_ips_from_cached_events(events: &[CachedEvent]) -> Vec<IpAddr> {
    let mut ips = Vec::new();
    for event in events {
        if event.event_type != "batch" {
            continue;
        }
        // Walk the JSON: data.lookups.lookups[].result.Response.records[].data.{A,AAAA}
        let lookups = &event.data["lookups"]["lookups"];
        if let Some(arr) = lookups.as_array() {
            for lookup in arr {
                if let Some(records) = lookup["result"]["Response"]["records"].as_array() {
                    for record in records {
                        if let Some(data) = record["data"].as_object() {
                            for key in ["A", "AAAA"] {
                                if let Some(val) = data.get(key).and_then(|v| v.as_str())
                                    && let Ok(ip) = val.parse::<IpAddr>()
                                    && !ips.contains(&ip)
                                {
                                    ips.push(ip);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    ips
}

/// Send an enrichment SSE event if enrichment is enabled and IPs are found.
pub(crate) async fn send_enrichment_event(
    enrichment_svc: &Arc<EnrichmentClient>,
    ips: &[IpAddr],
    request_id: &str,
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    cached_events: &mut Vec<CachedEvent>,
) {
    if ips.is_empty() {
        return;
    }
    let enrichments = enrichment_svc.lookup_batch(ips).await;
    if enrichments.is_empty() {
        return;
    }

    let enrichment_event = EnrichmentEvent {
        request_id: request_id.to_owned(),
        enrichments: enrichments
            .into_iter()
            .map(|(ip, info)| (ip.to_string(), info))
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

// ---------------------------------------------------------------------------
// GET handler
// ---------------------------------------------------------------------------

/// Shared query parameters for all streaming endpoints.
#[derive(Deserialize, utoipa::IntoParams)]
pub struct StreamParams {
    /// Set to `false` to collect all events and return a single JSON response
    /// instead of streaming Server-Sent Events.
    #[serde(default = "default_stream")]
    pub stream: bool,
}

fn default_stream() -> bool {
    true
}

#[derive(Deserialize, utoipa::IntoParams)]
pub struct QueryParams {
    /// Query string in dig-inspired syntax: `domain [TYPE...] [@server...] [+flag...]`.
    /// Example: `example.com MX @cloudflare +tls`
    q: Option<String>,
    /// Set to `false` to receive a single JSON response instead of SSE.
    #[serde(default = "default_stream")]
    stream: bool,
}

/// Run a DNS query using the dig-inspired query language.
///
/// Parses the `q` parameter, fans out per-record-type lookups across all specified
/// resolvers concurrently, and streams results as Server-Sent Events.
///
/// ## SSE Events
///
/// - `batch` — one per record type: `{"request_id","record_type","lookups","completed","total"}`
/// - `done` — final summary: `{"request_id","total_queries","duration_ms","warnings","transport","dnssec"}`
/// - `error` — non-fatal error: `{"code","message"}`
#[utoipa::path(
    get, path = "/api/query",
    tag = "Query",
    params(QueryParams),
    responses(
        (status = 200, description = "SSE stream of DNS lookup results", content_type = "text/event-stream"),
        (status = 400, description = "Bad request (invalid query syntax, domain, or server)", body = ErrorResponse),
        (status = 422, description = "Query rejected by policy (blocked type, private IP, limits exceeded)", body = ErrorResponse),
        (status = 429, description = "Rate limit exceeded", body = ErrorResponse),
    )
)]
pub async fn get_handler(
    State(state): State<AppState>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Query(params): Query<QueryParams>,
) -> Result<Response, ApiError> {
    let q = params
        .q
        .ok_or(ApiError::ParseError("missing query parameter 'q'".into()))?;

    let parsed = parser::parse(&q).map_err(|e| ApiError::ParseError(e.to_string()))?;

    let client_ip = state.ip_extractor.extract(&headers, peer_addr);
    tracing::debug!(%client_ip, %peer_addr, "query GET");

    execute_query(parsed, state, client_ip, request_id.0, q, params.stream).await
}

// ---------------------------------------------------------------------------
// POST handler
// ---------------------------------------------------------------------------

/// Run a DNS query using a structured JSON body.
///
/// Fans out per-record-type lookups across all specified resolvers concurrently
/// and streams results as Server-Sent Events.
///
/// ## SSE Events
///
/// - `batch` — one per record type: `{"request_id","record_type","lookups","completed","total"}`
/// - `done` — final summary: `{"request_id","total_queries","duration_ms","warnings","transport","dnssec"}`
/// - `error` — non-fatal error: `{"code","message"}`
#[utoipa::path(
    post, path = "/api/query",
    tag = "Query",
    request_body = PostQueryRequest,
    responses(
        (status = 200, description = "SSE stream of DNS lookup results", content_type = "text/event-stream"),
        (status = 400, description = "Bad request (invalid domain, record type, or server)", body = ErrorResponse),
        (status = 422, description = "Query rejected by policy (blocked type, private IP, limits exceeded)", body = ErrorResponse),
        (status = 429, description = "Rate limit exceeded", body = ErrorResponse),
    )
)]
pub async fn post_handler(
    State(state): State<AppState>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Query(stream_params): Query<StreamParams>,
    Json(body): Json<PostQueryRequest>,
) -> Result<Response, ApiError> {
    let query_display = body.domain.clone();
    let parsed = convert_post_body(body)?;

    let client_ip = state.ip_extractor.extract(&headers, peer_addr);
    tracing::debug!(%client_ip, %peer_addr, "query POST");

    execute_query(parsed, state, client_ip, request_id.0, query_display, stream_params.stream).await
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PostQueryRequest {
    /// Domain name to query (e.g. `"example.com"`).
    pub(crate) domain: String,
    /// DNS record types to query (e.g. `["A", "MX"]`). Defaults to A, AAAA, CNAME, MX.
    #[serde(default)]
    pub(crate) record_types: Vec<String>,
    /// DNS servers to use (e.g. `["cloudflare", "8.8.8.8"]`). Defaults to config default_servers.
    #[serde(default)]
    pub(crate) servers: Vec<String>,
    /// Transport: `udp` (default), `tcp`, `tls`, or `https`.
    #[serde(default)]
    pub(crate) transport: Option<String>,
    /// Enable DNSSEC mode (adds DNSKEY and DS record types).
    #[serde(default)]
    pub(crate) dnssec: bool,
}

/// Convert a structured POST body into a [`ParsedQuery`].
pub(crate) fn convert_post_body(body: PostQueryRequest) -> Result<ParsedQuery, ApiError> {
    let domain = body.domain.to_ascii_lowercase();
    if domain.is_empty() {
        return Err(ApiError::InvalidDomain("empty domain".into()));
    }

    let mut record_types = Vec::new();
    for rt_str in &body.record_types {
        let rt = RecordType::from_str(&rt_str.to_ascii_uppercase())
            .map_err(|_| ApiError::InvalidRecordType(rt_str.clone()))?;
        if !record_types.contains(&rt) {
            record_types.push(rt);
        }
    }

    if record_types.is_empty() {
        if domain.parse::<std::net::IpAddr>().is_ok() {
            record_types.push(RecordType::PTR);
        } else {
            record_types.extend_from_slice(&[
                RecordType::A,
                RecordType::AAAA,
                RecordType::CNAME,
                RecordType::MX,
            ]);
        }
    }

    let mut servers = Vec::new();
    for name in &body.servers {
        let server = parse_server_spec(name)?;
        servers.push(server);
    }

    let transport = body
        .transport
        .as_deref()
        .map(|t| match t.to_ascii_lowercase().as_str() {
            "udp" => Ok(Transport::Udp),
            "tcp" => Ok(Transport::Tcp),
            "tls" => Ok(Transport::Tls),
            "https" => Ok(Transport::Https),
            _ => Err(ApiError::ParseError(format!("unknown transport: {t}"))),
        })
        .transpose()?;

    Ok(ParsedQuery {
        domain,
        record_types,
        servers,
        transport,
        dnssec: body.dnssec,
        short: false,
        recursive: true,
        truncated_servers: false,
        warnings: Vec::new(),
    })
}

pub(crate) fn parse_server_spec(name: &str) -> Result<ServerSpec, ApiError> {
    if name.eq_ignore_ascii_case("system") {
        return Ok(ServerSpec::System);
    }
    if let Ok(provider) = PredefinedProvider::from_str(name) {
        return Ok(ServerSpec::Predefined(provider));
    }
    if let Ok(addr) = name.parse::<std::net::IpAddr>() {
        return Ok(ServerSpec::Ip { addr, port: 53 });
    }
    if let Some((addr_str, port_str)) = name.rsplit_once(':')
        && let (Ok(addr), Ok(port)) = (
            addr_str.parse::<std::net::IpAddr>(),
            port_str.parse::<u16>(),
        )
    {
        return Ok(ServerSpec::Ip { addr, port });
    }
    Err(ApiError::InvalidServer(name.to_owned()))
}

// ---------------------------------------------------------------------------
// Shared execution pipeline
// ---------------------------------------------------------------------------

async fn execute_query(
    mut parsed: ParsedQuery,
    state: AppState,
    client_ip: IpAddr,
    request_id: String,
    query_string: String,
    stream: bool,
) -> Result<Response, ApiError> {
    // Validate against query policy.
    let policy = QueryPolicy::new(&state.config);
    policy.validate(&parsed)?;

    let timeout_secs = state.config.limits.max_timeout_secs;
    let timeout = Duration::from_secs(timeout_secs);

    // When DNSSEC mode is requested, ensure DNSKEY and DS are queried so
    // DNSSEC records are visible in results. This is additive — explicit
    // record types from the user are preserved.
    if parsed.dnssec {
        for rt in [RecordType::DNSKEY, RecordType::DS] {
            if !parsed.record_types.contains(&rt) {
                parsed.record_types.push(rt);
            }
        }
    }

    // Rate limiting: compute cost and check budget before building resolvers.
    // Total cost = record_types × servers (SDD §8.2 query cost model).
    // Per-target cost = record_types (each target is only charged its share).
    let effective_servers = effective_server_specs(&parsed, &state.config);
    let target_keys = target_keys_from_servers(&effective_servers);
    let num_types = parsed.record_types.len() as u32;
    let num_servers = effective_servers.len().max(1) as u32;
    let total_cost = num_types * num_servers;
    let stream_guard = state.hot_state.rate_limiter.load().check_query_cost(
        client_ip,
        &target_keys,
        total_cost,
        num_types,
    )?;

    // Build resolver group and parallel circuit breaker keys.
    let (resolver_group, breaker_keys) =
        build_resolver_group(&parsed, &state.config, timeout).await?;

    // Extract individual resolvers from the group. We call Resolver::lookup()
    // per record type (which is Send — it uses tokio::task::spawn internally)
    // rather than ResolverGroup::lookup() (which is !Send due to ThreadRng in
    // the Uni mode path).
    let resolvers: Vec<Resolver> = resolver_group.resolvers().to_vec();

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(32);
    // When non-streaming, use a oneshot to receive all cached events after completion.
    let (done_tx, done_rx) = if !stream {
        let (s, r) = tokio::sync::oneshot::channel::<Vec<crate::result_cache::CachedEvent>>();
        (Some(s), Some(r))
    } else {
        (None, None)
    };

    let warnings = parsed.warnings.clone();
    let record_types = parsed.record_types.clone();
    let domain = parsed.domain.clone();
    let rid = request_id;
    let circuit_breakers = state.circuit_breakers.clone();
    let query_semaphore = state.query_semaphore.clone();
    let transport_name = match parsed.transport {
        Some(Transport::Udp) => "udp",
        Some(Transport::Tcp) => "tcp",
        Some(Transport::Tls) => "tls",
        Some(Transport::Https) => "https",
        None => "udp",
    }
    .to_owned();
    let dnssec_requested = parsed.dnssec;
    let result_cache = state.result_cache.clone();
    let enrichment_svc = state.ip_enrichment.clone();

    tokio::spawn(async move {
        // Hold stream guard for the lifetime of this task so the active stream
        // count is decremented when the SSE connection ends.
        let _stream_guard = stream_guard;
        metrics::gauge!("prism_active_queries").increment(1.0);
        let start = Instant::now();
        let mut cached_events: Vec<CachedEvent> = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(STREAM_TIMEOUT_SECS);
        let total = record_types.len() as u32;
        // std::sync::Mutex is safe here: the lock is never held across an .await point.
        let degraded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        // TODO: consider shared fan-out helper — check.rs already extracts
        // fan_out_lookup(), but query.rs returns a (RecordType, Lookups, had_error)
        // tuple and emits per-type metrics, while check.rs returns Lookups
        // directly and uses a bool tag for the DMARC lookup. The signatures
        // diverge enough that a shared abstraction would add indirection without
        // simplifying either call site.
        //
        // Build one future per record type. Each future fans out across all
        // resolvers concurrently and returns (RecordType, Lookups, had_error).
        // FuturesUnordered drives them all in parallel and yields each result
        // as it completes, so the total wall-clock time is bounded by the
        // slowest single-type query rather than the sum.
        let futs: FuturesUnordered<_> = record_types
            .iter()
            .map(|rt| {
                let rt = *rt;
                let domain = domain.clone();
                let resolvers = resolvers.clone();
                let breaker_keys = breaker_keys.clone();
                let circuit_breakers = Arc::clone(&circuit_breakers);
                let tx_err = tx.clone();
                let degraded = Arc::clone(&degraded);
                let semaphore = Arc::clone(&query_semaphore);
                async move {
                    let query = match MultiQuery::single(domain.as_str(), rt) {
                        Ok(q) => q,
                        Err(e) => {
                            tracing::warn!(domain = %domain, record_type = %rt, error = %e, "query build failed");
                            let _ = tx_err
                                .send(Ok(make_error_event("RESOLVER_ERROR", &e.to_string())))
                                .await;
                            return (rt, Lookups::empty(), true);
                        }
                    };

                    // Circuit breaker pre-check and per-resolver spawn.
                    let mut handles = Vec::with_capacity(resolvers.len());
                    for (idx, resolver) in resolvers.iter().enumerate() {
                        let breaker_key = &breaker_keys[idx];
                        if let Err(crate::circuit_breaker::BreakerState::Open) =
                            circuit_breakers.check(breaker_key)
                        {
                            tracing::warn!(domain = %domain, record_type = %rt, provider = %breaker_key, "circuit breaker open, skipping provider");
                            let _ = tx_err
                                .send(Ok(make_error_event(
                                    "PROVIDER_DEGRADED",
                                    &format!(
                                        "circuit breaker open for {breaker_key} ({rt}), skipping"
                                    ),
                                )))
                                .await;
                            if let Ok(mut d) = degraded.lock()
                                && !d.contains(breaker_key)
                            {
                                d.push(breaker_key.clone());
                            }
                            continue;
                        }
                        let r = resolver.clone();
                        let q = query.clone();
                        let sem = Arc::clone(&semaphore);
                        handles.push(tokio::spawn(async move {
                            let _permit = sem.acquire().await;
                            r.lookup(q).await
                        }));
                    }

                    let mut merged = Lookups::empty();
                    let mut had_error = false;
                    for handle in handles {
                        match handle.await {
                            Ok(Ok(lookups)) => {
                                record_breaker_outcomes(&circuit_breakers, &lookups);
                                merged = merged.merge(lookups);
                            }
                            Ok(Err(e)) => {
                                had_error = true;
                                tracing::warn!(domain = %domain, record_type = %rt, error = %e, "resolver lookup failed");
                                let _ = tx_err
                                    .send(Ok(make_error_event("RESOLVER_ERROR", &e.to_string())))
                                    .await;
                            }
                            Err(e) => {
                                had_error = true;
                                tracing::error!(domain = %domain, record_type = %rt, error = %e, "resolver task panicked");
                                let _ = tx_err
                                    .send(Ok(make_error_event("INTERNAL_ERROR", &e.to_string())))
                                    .await;
                            }
                        }
                    }

                    (rt, merged, had_error)
                }
            })
            .collect();

        tokio::pin!(futs);
        let mut completed: u32 = 0;
        let mut timed_out = false;

        loop {
            tokio::select! {
                maybe = futs.next() => {
                    match maybe {
                        None => break, // all record types completed
                        Some((rt, merged, had_error)) => {
                            completed += 1;
                            let status = if had_error { "error" } else { "ok" };
                            metrics::counter!(
                                "prism_queries_total",
                                "status" => status,
                                "record_type" => rt.to_string()
                            )
                            .increment(1);

                            let batch = BatchEvent {
                                request_id: rid.clone(),
                                record_type: rt.to_string(),
                                lookups: merged,
                                completed,
                                total,
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
                                // Client disconnected.
                                metrics::gauge!("prism_active_queries").decrement(1.0);
                                return;
                            }
                        }
                    }
                }
                _ = tokio::time::sleep_until(deadline) => {
                    timed_out = true;
                    let _ = tx
                        .send(Ok(make_error_event(
                            "STREAM_TIMEOUT",
                            "stream deadline exceeded",
                        )))
                        .await;
                    break;
                }
            }
        }

        if !timed_out {
            // Send enrichment event before done (non-blocking, graceful).
            if let Some(ref svc) = enrichment_svc {
                let ips = extract_ips_from_cached_events(&cached_events);
                send_enrichment_event(svc, &ips, &rid, &tx, &mut cached_events).await;
            }

            let elapsed = start.elapsed();
            tracing::info!(
                request_id = %rid,
                domain = %domain,
                duration_ms = elapsed.as_millis(),
                "query completed"
            );
            let degraded_providers = degraded.lock().map(|d| d.clone()).unwrap_or_default();

            // Cache the result and include the key in the done event.
            let cache_key = ResultCache::generate_key();
            let done = DoneEvent {
                request_id: rid,
                total_queries: total,
                duration_ms: elapsed.as_millis() as u64,
                warnings,
                transport: transport_name,
                dnssec: dnssec_requested,
                degraded_providers,
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
                        mode: "query".to_owned(),
                        events: cached_events.clone(),
                    },
                )
                .await;

            // Signal non-streaming callers with all collected events.
            if let Some(dtx) = done_tx {
                let _ = dtx.send(cached_events);
            }

            let event = Event::default()
                .event("done")
                .json_data(&done)
                .unwrap_or_else(|_| Event::default().event("done").data("{}"));
            let _ = tx.send(Ok(event)).await;

            metrics::histogram!("prism_query_duration_seconds")
                .record(start.elapsed().as_secs_f64());
        }

        metrics::gauge!("prism_active_queries").decrement(1.0);
    });

    if let Some(drx) = done_rx {
        // Non-streaming mode: wait for the task to complete and return JSON.
        use crate::api::CollectedResponse;
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

/// Record circuit breaker outcomes from lookup results.
pub(crate) fn record_breaker_outcomes(cb: &Arc<CircuitBreakerRegistry>, lookups: &Lookups) {
    for lookup in lookups.iter() {
        let server_name = lookup.name_server().to_string();
        if lookup.result().is_err() {
            cb.record_failure(&server_name);
        } else {
            cb.record_success(&server_name);
        }
    }
}

/// Resolve the effective server specs for a query, applying config defaults
/// when no servers are specified.
pub(crate) fn effective_server_specs(parsed: &ParsedQuery, config: &Config) -> Vec<ServerSpec> {
    if parsed.servers.is_empty() {
        config
            .dns
            .default_servers
            .iter()
            .filter_map(|s| PredefinedProvider::from_str(s).ok())
            .map(ServerSpec::Predefined)
            .collect()
    } else {
        parsed.servers.clone()
    }
}

/// Derive rate-limiting target keys from server specs.
///
/// Each unique server produces a key for per-target rate limiting:
/// - Predefined provider → lowercase provider name (e.g., `"cloudflare"`)
/// - System → `"system"`
/// - IP → address string (e.g., `"1.1.1.1"`)
pub(crate) fn target_keys_from_servers(servers: &[ServerSpec]) -> Vec<String> {
    servers
        .iter()
        .map(|s| match s {
            ServerSpec::Predefined(p) => p.to_string().to_ascii_lowercase(),
            ServerSpec::System => "system".to_string(),
            ServerSpec::Ip { addr, .. } => addr.to_string(),
        })
        .collect()
}

/// Build a [`ResolverGroup`] from the parsed query's server specs.
///
/// If the query specifies no servers, the config's default servers are used.
/// For predefined providers, configs are filtered to the requested transport
/// (default UDP) and IPv4 only — this avoids hanging on unreachable IPv6 or
/// slow TLS/HTTPS connections. Each provider contributes its primary + secondary
/// IPv4 addresses for the selected transport (typically 2 resolvers).
///
/// Returns the resolver group and a parallel `Vec<String>` of circuit breaker keys
/// (one per resolver, in the same order as `resolvers()`).
pub(crate) async fn build_resolver_group(
    parsed: &ParsedQuery,
    config: &Config,
    timeout: Duration,
) -> Result<(ResolverGroup, Vec<String>), ApiError> {
    let servers = effective_server_specs(parsed, config);

    let mut builder = ResolverGroupBuilder::new().timeout(timeout);
    let mut breaker_keys: Vec<String> = Vec::new();

    for server in &servers {
        match server {
            ServerSpec::Predefined(provider) => {
                let transport = parsed.transport.unwrap_or(Transport::Udp);
                let key = provider.to_string().to_ascii_lowercase();
                for ns_config in provider.configs() {
                    if !matches_transport(&ns_config, transport) {
                        continue;
                    }
                    if !is_ipv4_config(&ns_config) {
                        continue;
                    }
                    builder = builder.nameserver(ns_config);
                    breaker_keys.push(key.clone());
                }
            }
            ServerSpec::System => {
                builder = builder.system();
                breaker_keys.push("system".to_string());
            }
            ServerSpec::Ip { addr, port } => {
                let sock = SocketAddr::new(*addr, *port);
                let ns_config = match parsed.transport {
                    Some(Transport::Tcp) => NameServerConfig::tcp(sock),
                    Some(Transport::Tls) => NameServerConfig::tls(sock, addr.to_string()),
                    Some(Transport::Https) => NameServerConfig::https(sock, addr.to_string()),
                    Some(Transport::Udp) | None => NameServerConfig::udp(sock),
                };
                builder = builder.nameserver(ns_config);
                breaker_keys.push(format!("{addr}:{port}"));
            }
        }
    }

    let group = builder
        .build()
        .await
        .map_err(|e| ApiError::ResolverError(e.to_string()))?;

    Ok((group, breaker_keys))
}

/// Check if a nameserver config uses the specified transport.
fn matches_transport(config: &NameServerConfig, transport: Transport) -> bool {
    matches!(
        (config, transport),
        (NameServerConfig::Udp { .. }, Transport::Udp)
            | (NameServerConfig::Tcp { .. }, Transport::Tcp)
            | (NameServerConfig::Tls { .. }, Transport::Tls)
            | (NameServerConfig::Https { .. }, Transport::Https)
    )
}

/// Check if a nameserver config targets an IPv4 address.
fn is_ipv4_config(config: &NameServerConfig) -> bool {
    let ip = match config {
        NameServerConfig::Udp { ip_addr, .. }
        | NameServerConfig::Tcp { ip_addr, .. }
        | NameServerConfig::Tls { ip_addr, .. }
        | NameServerConfig::Https { ip_addr, .. } => ip_addr,
    };
    ip.is_ipv4()
}

pub(crate) fn make_error_event(code: &str, message: &str) -> Event {
    let payload = ErrorEvent {
        code: code.to_owned(),
        message: message.to_owned(),
    };
    Event::default()
        .event("error")
        .json_data(&payload)
        .unwrap_or_else(|_| Event::default().event("error").data("{}"))
}
