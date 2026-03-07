//! API route definitions and shared application state.

pub mod check;
pub mod meta;
pub mod parse;
pub mod query;
pub mod trace;

use std::sync::Arc;

use axum::Router;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use mhost::resolver::Lookups;
use serde::Serialize;
use utoipa::OpenApi;

use crate::circuit_breaker::CircuitBreakerRegistry;
use crate::config::Config;
use crate::error::{ErrorInfo, ErrorResponse};
use crate::security::{IpExtractor, RateLimitState};

/// Hard cap on total SSE stream duration (SDD §8.1).
pub const STREAM_TIMEOUT_SECS: u64 = 30;

/// SSE batch event emitted once per record type as DNS results arrive.
#[derive(Serialize)]
pub struct BatchEvent {
    pub request_id: String,
    pub record_type: String,
    pub lookups: Lookups,
    pub completed: u32,
    pub total: u32,
}

/// Shared state passed to all API handlers via axum's `State` extractor.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub circuit_breakers: Arc<CircuitBreakerRegistry>,
    pub ip_extractor: Arc<IpExtractor>,
    pub rate_limiter: Arc<RateLimitState>,
}

// ---------------------------------------------------------------------------
// OpenAPI specification
// ---------------------------------------------------------------------------

#[derive(OpenApi)]
#[openapi(
    info(
        title = "prism",
        description = "Web-based DNS debugging API powered by mhost.\n\n\
            Queries multiple DNS servers in parallel and streams results as Server-Sent Events.\n\n\
            ## Rate Limiting\n\
            All endpoints (except `/api/health`) are rate-limited per source IP.\n\
            When the limit is exceeded (HTTP 429), the `Retry-After` header indicates\n\
            how many seconds to wait before retrying.\n\n\
            ## Query cost model\n\
            Cost = `record_types × servers`. The check and trace endpoints charge a\n\
            flat cost of 16 tokens per request.",
        license(name = "MIT OR Apache-2.0"),
    ),
    paths(
        query::get_handler,
        query::post_handler,
        check::post_handler,
        trace::post_handler,
        parse::parse_handler,
        meta::servers,
        meta::record_types,
        meta::health,
    ),
    components(schemas(
        query::PostQueryRequest,
        query::PostServerSpec,
        check::CheckRequest,
        trace::TraceRequest,
        parse::ParseRequest,
        parse::ParseResponse,
        parse::TokenInfo,
        parse::Completion,
        meta::HealthResponse,
        meta::ServerInfo,
        meta::ServerConfigInfo,
        meta::RecordTypeInfo,
        ErrorResponse,
        ErrorInfo,
    )),
    tags(
        (name = "Query", description = "DNS lookups with multi-server fan-out"),
        (name = "Check", description = "Comprehensive DNS health check with lint analysis"),
        (name = "Trace", description = "DNS delegation chain walk from root to authoritative"),
        (name = "Metadata", description = "Available servers and record types"),
        (name = "Probes", description = "Health check endpoint"),
    )
)]
struct ApiDoc;

// ---------------------------------------------------------------------------
// OpenAPI spec + docs handlers
// ---------------------------------------------------------------------------

async fn openapi_handler() -> Response {
    let mut spec = ApiDoc::openapi();
    spec.info.version = env!("CARGO_PKG_VERSION").to_string();
    let json = spec.to_pretty_json().unwrap_or_default();
    (
        axum::http::StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        json,
    )
        .into_response()
}

async fn docs_handler() -> Response {
    Html(include_str!("../scalar_docs.html")).into_response()
}

async fn docs_redirect() -> Response {
    (
        axum::http::StatusCode::MOVED_PERMANENTLY,
        [(axum::http::header::LOCATION, "/docs")],
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Routers
// ---------------------------------------------------------------------------

/// Build the API router with all endpoints.
///
/// Health is mounted separately so it can bypass rate limiting.
pub fn api_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/api/query",
            get(query::get_handler).post(query::post_handler),
        )
        .route("/api/servers", get(meta::servers))
        .route("/api/record-types", get(meta::record_types))
        .route("/api/check", post(check::post_handler))
        .route("/api/trace", post(trace::post_handler))
        .route("/api/parse", post(parse::parse_handler))
        .route("/api-docs/openapi.json", get(openapi_handler))
        .route("/docs", get(docs_handler))
        .route("/docs/", get(docs_redirect))
        .with_state(state)
}

/// Health endpoint router. Kept separate so it can be mounted outside
/// rate-limiting layers.
pub fn health_router() -> Router {
    Router::new().route("/api/health", get(meta::health))
}
