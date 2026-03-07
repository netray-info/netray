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
use axum::extract::{ConnectInfo, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use hickory_proto::rr::RecordType;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::api::AppState;
use crate::api::query::make_error_event;
use crate::dns_trace;
use crate::error::{ApiError, ErrorResponse};

// Cost charged against per-IP and global rate limiters per trace request.
// Trace queries root/TLD/auth servers (public infrastructure), so we skip
// per-target charging and apply a flat cost of 16 tokens — same as check.
const TRACE_COST: u32 = 16;

// Hard cap on total streaming time (SDD §8.1).
const STREAM_TIMEOUT_SECS: u64 = 30;

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
    raw_query: axum::extract::RawQuery,
    Json(body): Json<TraceRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    // Reject POST with a query string — ambiguous input.
    if raw_query.0.is_some() {
        return Err(ApiError::AmbiguousInput);
    }

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
    let stream_guard =
        state
            .rate_limiter
            .check_query_cost(client_ip, &[], TRACE_COST, TRACE_COST)?;

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(32);
    let rid = uuid::Uuid::now_v7().to_string();

    tokio::spawn(async move {
        let _stream_guard = stream_guard;
        metrics::gauge!("prism_active_traces").increment(1.0);
        let start = Instant::now();

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
            let event = Event::default()
                .event("hop")
                .json_data(&event_payload)
                .unwrap_or_else(|_| Event::default().event("hop").data("{}"));
            if tx.send(Ok(event)).await.is_err() {
                metrics::gauge!("prism_active_traces").decrement(1.0);
                return;
            }
        }

        let elapsed = start.elapsed();
        let done = TraceDoneEvent {
            request_id: rid,
            duration_ms: elapsed.as_millis() as u64,
            hops: hop_count,
        };
        let event = Event::default()
            .event("done")
            .json_data(&done)
            .unwrap_or_else(|_| Event::default().event("done").data("{}"));
        let _ = tx.send(Ok(event)).await;

        metrics::histogram!("prism_trace_duration_seconds").record(elapsed.as_secs_f64());
        metrics::gauge!("prism_active_traces").decrement(1.0);
    });

    let stream = ReceiverStream::new(rx);

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}
