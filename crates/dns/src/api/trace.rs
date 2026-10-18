//! Trace endpoint: walk the DNS delegation chain for a domain, streaming results as SSE.
//!
//! - `POST /api/trace` — accept a JSON body with domain and optional record_type/timeout_secs.
//!
//! Performs iterative, non-recursive resolution from root servers, emitting one `hop` SSE
//! event per delegation level and a final `done` event with summary counts.

use std::net::SocketAddr;

use axum::Json;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;

use crate::RequestId;
use crate::api::query::StreamParams;
use crate::api::{AppState, respond};
use crate::error::{ApiError, ErrorResponse};

// ---------------------------------------------------------------------------
// POST handler
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct TraceRequest {
    /// Domain name to trace (e.g. `"example.com"`).
    pub domain: String,
    /// DNS record type to trace (default: `"A"`).
    #[serde(default = "default_record_type")]
    pub record_type: String,
    /// Query timeout per hop in seconds. Clamped to config max.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
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
    let client_ip = state.ip_extractor.extract(&headers, peer_addr);
    tracing::Span::current().record("client_ip", tracing::field::display(&client_ip));
    tracing::debug!(%client_ip, %peer_addr, domain = %body.domain, record_type = %body.record_type, "trace POST");

    let limiter = state.hot_state.rate_limiter.load_full();
    let job = state
        .jobs()
        .trace(&body, client_ip, &limiter, request_id.0)
        .await?;
    Ok(respond(
        job,
        Some((
            state.result_cache.clone(),
            "trace",
            body.domain.to_ascii_lowercase(),
        )),
        stream_params.stream,
    )
    .await)
}
