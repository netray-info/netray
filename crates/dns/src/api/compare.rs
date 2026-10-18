//! Transport comparison endpoint: query over all 4 transports and compare answers.
//!
//! - `POST /api/compare` — accept a structured JSON body.
//!
//! For each transport (UDP, TCP, TLS, HTTPS), builds a resolver group and fans out
//! per-record-type lookups. Each batch event includes the transport that produced it.
//! Providers that don't support a transport are skipped with a warning.

use std::net::SocketAddr;

use axum::Json;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;

use crate::RequestId;
use crate::api::query::{PostQueryRequest, StreamParams};
use crate::api::{AppState, respond};
use crate::error::{ApiError, ErrorResponse};

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
    Query(stream_params): Query<StreamParams>,
    Json(body): Json<PostQueryRequest>,
) -> Result<Response, ApiError> {
    let client_ip = state.ip_extractor.extract(&headers, peer_addr);
    tracing::Span::current().record("client_ip", tracing::field::display(&client_ip));
    tracing::debug!(%client_ip, %peer_addr, "compare POST");

    let limiter = state.hot_state.rate_limiter.load_full();
    let job = state
        .jobs()
        .compare(&body, client_ip, &limiter, request_id.0)
        .await?;
    Ok(respond(
        job,
        Some((state.result_cache.clone(), "compare", body.domain)),
        stream_params.stream,
    )
    .await)
}
