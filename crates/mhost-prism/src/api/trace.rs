//! Trace endpoint: walk the DNS delegation chain for a domain, streaming results as SSE.
//!
//! - `POST /api/trace` — accept a JSON body with domain and optional record_type/timeout_secs.
//!
//! Performs iterative, non-recursive resolution from root servers, emitting one `hop` SSE
//! event per delegation level and a final `done` event with summary counts.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::Response;
use hickory_proto::rr::RecordType;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::RequestId;
use crate::api::query::{StreamParams, make_error_event, send_enrichment_event};
use crate::api::{AppState, CollectedResponse, STREAM_TIMEOUT_SECS};
use crate::dns_trace;
use crate::error::{ApiError, ErrorResponse};
use crate::result_cache::{CachedEvent, CachedResult, ResultCache};

// Cost charged against per-IP and global rate limiters per trace request.
// Trace queries root/TLD/auth servers (public infrastructure), so we skip
// per-target charging and apply a flat cost of 16 tokens — same as check.
const TRACE_COST: u32 = 16;

// ---------------------------------------------------------------------------
// SSE event payloads
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct HopEvent {
    request_id: String,
    hop: dns_trace::TraceHop,
}

#[derive(Serialize)]
struct TraceDoneEvent {
    request_id: String,
    duration_ms: u64,
    hops: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_key: Option<String>,
}

// ---------------------------------------------------------------------------
// POST handler
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct TraceRequest {
    /// Domain name to trace (e.g. `"example.com"`).
    domain: String,
    /// DNS record type to trace (default: `"A"`).
    #[serde(default = "default_record_type")]
    record_type: String,
    /// Query timeout per hop in seconds. Clamped to config max.
    #[serde(default)]
    timeout_secs: Option<u64>,
}

fn default_record_type() -> String {
    "A".to_owned()
}

/// Walk the DNS delegation chain for a domain from root servers to authoritative.
///
/// Performs iterative, non-recursive resolution: queries root servers → TLD servers
/// → authoritative servers, emitting one `hop` event per delegation level.
///
/// ## SSE Events
///
/// - `hop` — one per delegation level: `{"request_id","hop":{"level","nameservers","referrals","records","latency_ms"}}`
/// - `done` — final summary: `{"request_id","duration_ms","hops"}`
/// - `error` — non-fatal error: `{"code","message"}`
#[utoipa::path(
    post, path = "/api/trace",
    tag = "Trace",
    request_body = TraceRequest,
    responses(
        (status = 200, description = "SSE stream of delegation hops and done summary", content_type = "text/event-stream"),
        (status = 400, description = "Bad request (invalid domain or record type)", body = ErrorResponse),
        (status = 429, description = "Rate limit exceeded", body = ErrorResponse),
    )
)]
pub async fn post_handler(
    State(state): State<AppState>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Query(stream_params): Query<StreamParams>,
    Json(body): Json<TraceRequest>,
) -> Result<Response, ApiError> {
    let domain = body.domain.to_ascii_lowercase();
    if domain.is_empty() {
        return Err(ApiError::InvalidDomain("empty domain".into()));
    }
    if domain.len() > 253 {
        return Err(ApiError::InvalidDomain(format!(
            "domain exceeds maximum length of 253 characters (got {})",
            domain.len()
        )));
    }

    let name =
        dns_trace::parse_name(&domain).map_err(|e| ApiError::InvalidDomain(e.to_string()))?;

    let record_type = dns_trace::parse_record_type(&body.record_type)
        .map_err(|_| ApiError::InvalidRecordType(body.record_type.clone()))?;

    // Block types that don't make sense in an iterative trace.
    match record_type {
        RecordType::ANY | RecordType::AXFR | RecordType::IXFR => {
            return Err(ApiError::InvalidRecordType(body.record_type.clone()));
        }
        _ => {}
    }

    let client_ip = state.ip_extractor.extract(&headers, peer_addr);
    tracing::debug!(%client_ip, %peer_addr, domain = %domain, record_type = %body.record_type, "trace POST");

    // Clamp timeout to config max; default to config max.
    let timeout_secs = body
        .timeout_secs
        .map(|t| t.min(state.config.limits.max_timeout_secs))
        .unwrap_or(state.config.trace.query_timeout_secs);
    let query_timeout = Duration::from_secs(timeout_secs);

    let max_hops = state.config.trace.max_hops as usize;

    // Rate limiting: flat cost against per-IP and global limiters only.
    // Trace queries public infrastructure (not user-specified servers), so
    // per-target charging is skipped.
    let stream_guard = state.hot_state.rate_limiter.load().check_query_cost(
        client_ip,
        &[],
        TRACE_COST,
        TRACE_COST,
    )?;

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(32);
    let (done_tx, done_rx) = if !stream_params.stream {
        let (s, r) = tokio::sync::oneshot::channel::<Vec<crate::result_cache::CachedEvent>>();
        (Some(s), Some(r))
    } else {
        (None, None)
    };
    let rid = request_id.0;
    let result_cache = state.result_cache.clone();
    let query_string = domain.clone();
    let enrichment_svc = state.ip_enrichment.clone();

    tokio::spawn(async move {
        let _stream_guard = stream_guard;
        metrics::gauge!("prism_active_traces").increment(1.0);
        let start = Instant::now();
        let mut cached_events: Vec<CachedEvent> = Vec::new();

        let hops = match tokio::time::timeout(
            Duration::from_secs(STREAM_TIMEOUT_SECS),
            dns_trace::walk(name, record_type, max_hops, query_timeout),
        )
        .await
        {
            Ok(hops) => hops,
            Err(_) => {
                let _ = tx
                    .send(Ok(make_error_event(
                        "STREAM_TIMEOUT",
                        "stream deadline exceeded",
                    )))
                    .await;
                metrics::counter!("prism_queries_total", "endpoint" => "trace", "status" => "error").increment(1);
                metrics::gauge!("prism_active_traces").decrement(1.0);
                return;
            }
        };
        let hop_count = hops.len();

        for hop in hops {
            let event_payload = HopEvent {
                request_id: rid.clone(),
                hop,
            };
            if let Ok(json_val) = serde_json::to_value(&event_payload) {
                cached_events.push(CachedEvent {
                    event_type: "hop".to_owned(),
                    data: json_val,
                });
            }
            let event = Event::default()
                .event("hop")
                .json_data(&event_payload)
                .unwrap_or_else(|_| Event::default().event("hop").data("{}"));
            if tx.send(Ok(event)).await.is_err() {
                metrics::counter!("prism_queries_total", "endpoint" => "trace", "status" => "error").increment(1);
                metrics::gauge!("prism_active_traces").decrement(1.0);
                return;
            }
        }

        // Enrich server IPs from all hops.
        if let Some(ref svc) = enrichment_svc {
            let mut ips: Vec<std::net::IpAddr> = Vec::new();
            for event in &cached_events {
                if event.event_type != "hop" {
                    continue;
                }
                let Some(results) = event.data["hop"]["server_results"].as_array() else {
                    continue;
                };
                for sr in results {
                    if let Some(ip_str) = sr["server_ip"].as_str()
                        && let Ok(ip) = ip_str.parse::<std::net::IpAddr>()
                        && !ips.contains(&ip)
                    {
                        ips.push(ip);
                    }
                }
            }
            send_enrichment_event(svc, &ips, &rid, &tx, &mut cached_events).await;
        }

        let elapsed = start.elapsed();
        tracing::info!(
            request_id = %rid,
            domain = %domain,
            duration_ms = elapsed.as_millis(),
            "trace completed"
        );
        let cache_key = ResultCache::generate_key();
        let done = TraceDoneEvent {
            request_id: rid,
            duration_ms: elapsed.as_millis() as u64,
            hops: hop_count,
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
                    mode: "trace".to_owned(),
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

        metrics::counter!("prism_queries_total", "endpoint" => "trace", "status" => "ok")
            .increment(1);
        metrics::histogram!("prism_trace_duration_seconds").record(elapsed.as_secs_f64());
        metrics::gauge!("prism_active_traces").decrement(1.0);
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
