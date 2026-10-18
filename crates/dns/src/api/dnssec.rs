//! DNSSEC chain-of-trust endpoint: walk the delegation chain querying DNSKEY/DS/RRSIG
//! at each zone level, streaming results as SSE.
//!
//! - `POST /api/dnssec` — accept a JSON body with domain and optional timeout_secs.
//!
//! Performs iterative resolution from root servers, querying DNSSEC records at each
//! delegation level. Emits one `chain` SSE event per zone level and a final `done` event.

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
pub struct DnssecRequest {
    /// Domain name to validate (e.g. `"example.com"`).
    pub domain: String,
    /// Query timeout per level in seconds. Clamped to config max.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
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
    Query(stream_params): Query<StreamParams>,
    Json(body): Json<DnssecRequest>,
) -> Result<Response, ApiError> {
    let client_ip = state.ip_extractor.extract(&headers, peer_addr);
    tracing::Span::current().record("client_ip", tracing::field::display(&client_ip));
    tracing::debug!(%client_ip, %peer_addr, domain = %body.domain, "dnssec POST");

    let limiter = state.hot_state.rate_limiter.load_full();
    let job = state
        .jobs()
        .dnssec(&body, client_ip, &limiter, request_id.0)
        .await?;
    Ok(respond(job, None, stream_params.stream).await)
}
