use std::net::SocketAddr;
use std::time::Instant;

use axum::extract::{ConnectInfo, Query, State};
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

use crate::error::{AppError, ErrorResponse};
use crate::inspect;
use crate::inspect::assembler::*;
#[allow(unused_imports)] // Used in #[openapi] attribute
use crate::quality::types::{CheckStatus, QualityCheck, QualityReport};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReadyResponse {
    pub status: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigResponse {
    pub version: &'static str,
}

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
pub struct InspectRequest {
    pub url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct InspectQuery {
    pub url: Option<String>,
}

// ---------------------------------------------------------------------------
// OpenAPI
// ---------------------------------------------------------------------------

#[derive(OpenApi)]
#[openapi(
    info(title = "spectra", description = "HTTP header inspection and security audit"),
    paths(
        health_handler,
        ready_handler,
        config_handler,
        inspect_post_handler,
        inspect_get_handler,
    ),
    components(schemas(
        HealthResponse,
        ReadyResponse,
        ConfigResponse,
        InspectRequest,
        RedirectHop,
        HttpUpgrade,
        HstsCheck,
        HeaderCheck,
        CorsReport,
        CachingReport,
        CacheControlDirectives,
        CdnReport,
        FingerprintReport,
        InfoLeakage,
        ReportingReport,
        EnrichmentInfo,
        QualityReport,
        QualityCheck,
        CheckStatus,
        ErrorResponse,
    ))
)]
pub struct ApiDoc;

// ---------------------------------------------------------------------------
// Routers
// ---------------------------------------------------------------------------

pub fn health_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
        .with_state(state)
}

pub fn api_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/api/inspect",
            get(inspect_get_handler).post(inspect_post_handler),
        )
        .route("/api/config", get(config_handler))
        .route("/api-docs/openapi.json", get(openapi_handler))
        .route("/docs", get(docs_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/health",
    tag = "Probes",
    responses(
        (status = 200, description = "Service is alive", body = HealthResponse),
    )
)]
async fn health_handler() -> impl IntoResponse {
    (
        [(axum::http::header::CACHE_CONTROL, "no-cache")],
        Json(HealthResponse { status: "ok" }),
    )
}

#[utoipa::path(
    get,
    path = "/ready",
    tag = "Probes",
    responses(
        (status = 200, description = "Service is ready", body = ReadyResponse),
    )
)]
async fn ready_handler() -> impl IntoResponse {
    (
        [(axum::http::header::CACHE_CONTROL, "no-cache")],
        Json(ReadyResponse { status: "ready" }),
    )
}

#[utoipa::path(
    get,
    path = "/api/config",
    responses(
        (status = 200, description = "Service configuration", body = ConfigResponse),
    )
)]
async fn config_handler() -> Json<ConfigResponse> {
    Json(ConfigResponse {
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[utoipa::path(
    post,
    path = "/api/inspect",
    request_body = InspectRequest,
    responses(
        (status = 200, description = "Inspection result"),
        (status = 400, description = "Invalid URL", body = ErrorResponse),
        (status = 403, description = "Target blocked", body = ErrorResponse),
        (status = 422, description = "Invalid target", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 504, description = "Timeout", body = ErrorResponse),
    )
)]
async fn inspect_post_handler(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(body): Json<InspectRequest>,
) -> Result<Json<InspectResponse>, AppError> {
    let raw_url = body
        .url
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::InvalidUrl("url field is required".to_string()))?;

    do_inspect(&state, peer, &headers, raw_url).await
}

#[utoipa::path(
    get,
    path = "/api/inspect",
    params(
        ("url" = String, Query, description = "URL to inspect"),
    ),
    responses(
        (status = 200, description = "Inspection result"),
        (status = 400, description = "Invalid URL", body = ErrorResponse),
        (status = 403, description = "Target blocked", body = ErrorResponse),
        (status = 422, description = "Invalid target", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 504, description = "Timeout", body = ErrorResponse),
    )
)]
async fn inspect_get_handler(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    Query(query): Query<InspectQuery>,
) -> Result<Json<InspectResponse>, AppError> {
    let raw_url = query
        .url
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::InvalidUrl("url field is required".to_string()))?;

    let decoded = percent_encoding::percent_decode_str(raw_url)
        .decode_utf8_lossy()
        .to_string();

    do_inspect(&state, peer, &headers, &decoded).await
}

async fn do_inspect(
    state: &AppState,
    peer: SocketAddr,
    req_headers: &axum::http::HeaderMap,
    raw_url: &str,
) -> Result<Json<InspectResponse>, AppError> {
    let start = Instant::now();

    // 1. Parse and normalize URL
    let url = crate::input::parse_url(raw_url)?;

    // 2. Resolve and validate target (SSRF check)
    let resolved_addr = crate::input::validate_target(&url).await?;

    // 3. Rate limiting
    let client_ip = state.ip_extractor.extract(req_headers, peer);
    let hostname = url.host_str().unwrap_or_default();
    state.rate_limiter.check(client_ip, hostname)?;

    // 4. Execute inspection
    let result = inspect::inspect(&url, resolved_addr, &state.config.inspect).await?;

    // 5. IP enrichment (non-blocking, failure is OK)
    let enrichment_org = if let Some(ref client) = state.enrichment_client {
        client.lookup(resolved_addr.ip(), None).await.and_then(|info| info.org)
    } else {
        None
    };

    let enrichment_base_url = state
        .config
        .enrichment
        .ip_url
        .as_deref()
        .unwrap_or("https://ip.netray.info");

    let duration_ms = start.elapsed().as_millis() as u64;

    // 6. Assemble response
    let response = inspect::assemble_response(
        &url,
        resolved_addr,
        result,
        enrichment_org,
        enrichment_base_url,
        duration_ms,
    );

    Ok(Json(response))
}

async fn openapi_handler() -> impl IntoResponse {
    let mut doc = ApiDoc::openapi();
    doc.info.version = env!("CARGO_PKG_VERSION").to_string();
    Json(doc)
}

async fn docs_handler() -> Html<&'static str> {
    Html(include_str!("scalar_docs.html"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request as HttpRequest, StatusCode};
    use tower::ServiceExt;

    fn test_config() -> crate::config::Config {
        crate::config::Config::load(None).unwrap()
    }

    fn test_router() -> Router {
        let state = AppState::new(&test_config());
        health_router(state.clone()).merge(api_router(state))
    }

    async fn get(app: &Router, uri: &str) -> (StatusCode, serde_json::Value) {
        let response = app
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
        (status, body)
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let app = test_router();
        let (status, body) = get(&app, "/health").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn ready_returns_ok() {
        let app = test_router();
        let (status, body) = get(&app, "/ready").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ready");
    }

    #[tokio::test]
    async fn config_returns_version() {
        let app = test_router();
        let (status, body) = get(&app, "/api/config").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["version"].is_string());
    }

    #[tokio::test]
    async fn openapi_returns_json() {
        let app = test_router();
        let (status, body) = get(&app, "/api-docs/openapi.json").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["openapi"], "3.1.0");
        assert_eq!(body["info"]["title"], "spectra");
    }
}
