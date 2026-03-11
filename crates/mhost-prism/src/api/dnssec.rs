//! DNSSEC chain-of-trust endpoint: walk the delegation chain querying DNSKEY/DS/RRSIG
//! at each zone level, streaming results as SSE.
//!
//! - `POST /api/dnssec` — accept a JSON body with domain and optional timeout_secs.
//!
//! Performs iterative resolution from root servers, querying DNSSEC records at each
//! delegation level. Emits one `chain` SSE event per zone level and a final `done` event.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::RequestId;
use crate::api::query::make_error_event;
use crate::api::{AppState, STREAM_TIMEOUT_SECS};
use crate::dns_dnssec;
use crate::error::{ApiError, ErrorResponse};

// Flat rate limit cost — same as trace (queries public infrastructure).
const DNSSEC_COST: u32 = 16;

// ---------------------------------------------------------------------------
// SSE event payloads
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ChainEvent {
    request_id: String,
    level: dns_dnssec::ChainLevel,
}

#[derive(Serialize)]
struct DnssecDoneEvent {
    request_id: String,
    duration_ms: u64,
    levels: usize,
}

// ---------------------------------------------------------------------------
// POST handler
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct DnssecRequest {
    /// Domain name to validate (e.g. `"example.com"`).
    domain: String,
    /// Query timeout per level in seconds. Clamped to config max.
    #[serde(default)]
    timeout_secs: Option<u64>,
}

/// Walk the DNSSEC chain of trust for a domain from root to authoritative.
///
/// Queries DNSKEY, DS, and RRSIG records at each delegation level (root → TLD →
/// authoritative), producing structural findings about the chain of trust.
///
/// ## SSE Events
///
/// - `chain` — one per zone level: `{"request_id","level":{...}}`
/// - `done` — final summary: `{"request_id","duration_ms","levels"}`
/// - `error` — non-fatal error: `{"code","message"}`
#[utoipa::path(
    post, path = "/api/dnssec",
    tag = "DNSSEC",
    request_body = DnssecRequest,
    responses(
        (status = 200, description = "SSE stream of DNSSEC chain levels and done summary", content_type = "text/event-stream"),
        (status = 400, description = "Bad request (invalid domain)", body = ErrorResponse),
        (status = 429, description = "Rate limit exceeded", body = ErrorResponse),
    )
)]
pub async fn post_handler(
    State(state): State<AppState>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    raw_query: axum::extract::RawQuery,
    Json(body): Json<DnssecRequest>,
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

    let name = dns_dnssec::parse_name(&domain).map_err(ApiError::InvalidDomain)?;

    let client_ip = state.ip_extractor.extract(&headers, peer_addr);
    tracing::debug!(%client_ip, %peer_addr, domain = %domain, "dnssec POST");

    // Clamp timeout to config max; default to trace query timeout.
    let timeout_secs = body
        .timeout_secs
        .map(|t| t.min(state.config.limits.max_timeout_secs))
        .unwrap_or(state.config.trace.query_timeout_secs);
    let query_timeout = Duration::from_secs(timeout_secs);

    let max_hops = state.config.trace.max_hops as usize;

    // Rate limiting: flat cost against per-IP and global limiters only.
    let stream_guard = state.hot_state.rate_limiter.load().check_query_cost(
        client_ip,
        &[],
        DNSSEC_COST,
        DNSSEC_COST,
    )?;

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(32);
    let rid = request_id.0;

    tokio::spawn(async move {
        let _stream_guard = stream_guard;
        metrics::gauge!("prism_active_dnssec").increment(1.0);
        let start = Instant::now();

        let levels = match tokio::time::timeout(
            Duration::from_secs(STREAM_TIMEOUT_SECS),
            dns_dnssec::walk_chain(name, max_hops, query_timeout),
        )
        .await
        {
            Ok(levels) => levels,
            Err(_) => {
                let _ = tx
                    .send(Ok(make_error_event(
                        "STREAM_TIMEOUT",
                        "stream deadline exceeded",
                    )))
                    .await;
                metrics::counter!("prism_queries_total", "endpoint" => "dnssec", "status" => "error").increment(1);
                metrics::gauge!("prism_active_dnssec").decrement(1.0);
                return;
            }
        };
        let level_count = levels.len();

        for level in levels {
            let event_payload = ChainEvent {
                request_id: rid.clone(),
                level,
            };
            let event = Event::default()
                .event("chain")
                .json_data(&event_payload)
                .unwrap_or_else(|_| Event::default().event("chain").data("{}"));
            if tx.send(Ok(event)).await.is_err() {
                metrics::counter!("prism_queries_total", "endpoint" => "dnssec", "status" => "error").increment(1);
                metrics::gauge!("prism_active_dnssec").decrement(1.0);
                return;
            }
        }

        let elapsed = start.elapsed();
        tracing::info!(
            request_id = %rid,
            domain = %domain,
            duration_ms = elapsed.as_millis(),
            "dnssec chain walk completed"
        );
        let done = DnssecDoneEvent {
            request_id: rid,
            duration_ms: elapsed.as_millis() as u64,
            levels: level_count,
        };
        let event = Event::default()
            .event("done")
            .json_data(&done)
            .unwrap_or_else(|_| Event::default().event("done").data("{}"));
        let _ = tx.send(Ok(event)).await;

        metrics::counter!("prism_queries_total", "endpoint" => "dnssec", "status" => "ok")
            .increment(1);
        metrics::histogram!("prism_dnssec_duration_seconds").record(elapsed.as_secs_f64());
        metrics::gauge!("prism_active_dnssec").decrement(1.0);
    });

    let stream = ReceiverStream::new(rx);

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}
