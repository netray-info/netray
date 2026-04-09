use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Path, State};
use axum::response::{Html, IntoResponse, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::stream;
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

use crate::checks;
use crate::error::{ErrorResponse, MailError};
use crate::quality::types::{CheckResult, Grade, SubCheck, Verdict};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Request / Response types
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
pub struct MetaResponse {
    pub version: &'static str,
    pub service: &'static str,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct InspectRequest {
    pub domain: String,
    #[serde(default)]
    pub dkim_selectors: Vec<String>,
}

// ---------------------------------------------------------------------------
// OpenAPI
// ---------------------------------------------------------------------------

#[derive(OpenApi)]
#[openapi(
    info(title = "beacon", description = "Email security inspector — DNS-only mail posture analysis"),
    paths(
        health_handler,
        ready_handler,
        meta_handler,
        inspect_post_handler,
        inspect_get_handler,
    ),
    components(schemas(
        HealthResponse,
        ReadyResponse,
        MetaResponse,
        InspectRequest,
        CheckResult,
        SubCheck,
        Verdict,
        Grade,
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
        .route("/inspect", post(inspect_post_handler))
        .route("/inspect/{domain}", get(inspect_get_handler))
        .route("/api/meta", get(meta_handler))
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
        (status = 503, description = "Service not ready"),
    )
)]
async fn ready_handler(State(state): State<AppState>) -> impl IntoResponse {
    if state.dns_resolver.is_initialized() {
        (
            axum::http::StatusCode::OK,
            [(axum::http::header::CACHE_CONTROL, "no-cache")],
            Json(ReadyResponse { status: "ok" }),
        )
    } else {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            [(axum::http::header::CACHE_CONTROL, "no-cache")],
            Json(ReadyResponse {
                status: "not ready",
            }),
        )
    }
}

#[utoipa::path(
    get,
    path = "/api/meta",
    responses(
        (status = 200, description = "Service metadata", body = MetaResponse),
    )
)]
async fn meta_handler() -> Json<MetaResponse> {
    Json(MetaResponse {
        version: env!("CARGO_PKG_VERSION"),
        service: "beacon",
    })
}

#[utoipa::path(
    post,
    path = "/inspect",
    request_body = InspectRequest,
    responses(
        (status = 200, description = "SSE stream of check results"),
        (status = 400, description = "Invalid domain", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
    )
)]
#[axum::debug_handler]
async fn inspect_post_handler(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(body): Json<InspectRequest>,
) -> Result<Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>>, MailError>
{
    let domain = crate::input::parse_domain(&body.domain)?;

    if body.dkim_selectors.len() > state.config.dkim.max_user_selectors {
        return Err(MailError::TooManySelectors {
            max: state.config.dkim.max_user_selectors,
        });
    }

    // Rate limiting
    let client_ip = state.ip_extractor.extract(&headers, peer);
    if let Err(e) = state.rate_limiter.check(client_ip) {
        metrics::counter!("beacon_rate_limit_rejections_total").increment(1);
        return Err(e);
    }

    metrics::counter!("beacon_requests_total", "endpoint" => "inspect", "method" => "post").increment(1);

    let events = checks::run_all_checks(
        domain,
        body.dkim_selectors,
        state.config.clone(),
        state.http_client.clone(),
        state.http_client_follow.clone(),
        state.enrichment_client.clone(),
    )
    .await;

    metrics::counter!("beacon_sse_events_total").increment(events.len() as u64);

    let sse_stream = stream::iter(events.into_iter().map(|event| {
        let sse_event: axum::response::sse::Event = event.into();
        Ok::<_, std::convert::Infallible>(sse_event)
    }));

    Ok(Sse::new(sse_stream))
}

#[utoipa::path(
    get,
    path = "/inspect/{domain}",
    params(
        ("domain" = String, Path, description = "Domain to inspect"),
    ),
    responses(
        (status = 200, description = "SSE stream of check results"),
        (status = 400, description = "Invalid domain", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
    )
)]
async fn inspect_get_handler(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    Path(raw_domain): Path<String>,
) -> Result<Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>>, MailError>
{
    let decoded = percent_encoding::percent_decode_str(&raw_domain)
        .decode_utf8_lossy()
        .to_string();
    let domain = crate::input::parse_domain(&decoded)?;

    // Rate limiting
    let client_ip = state.ip_extractor.extract(&headers, peer);
    if let Err(e) = state.rate_limiter.check(client_ip) {
        metrics::counter!("beacon_rate_limit_rejections_total").increment(1);
        return Err(e);
    }

    metrics::counter!("beacon_requests_total", "endpoint" => "inspect", "method" => "get").increment(1);

    let events = checks::run_all_checks(
        domain,
        Vec::new(),
        state.config.clone(),
        state.http_client.clone(),
        state.http_client_follow.clone(),
        state.enrichment_client.clone(),
    )
    .await;

    metrics::counter!("beacon_sse_events_total").increment(events.len() as u64);

    let sse_stream = stream::iter(events.into_iter().map(|event| {
        let sse_event: axum::response::sse::Event = event.into();
        Ok::<_, std::convert::Infallible>(sse_event)
    }));

    Ok(Sse::new(sse_stream))
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

    async fn test_router() -> Router {
        let config = crate::config::Config::load(None).unwrap();
        let state = AppState::new(&config).await.unwrap();
        health_router(state.clone()).merge(api_router(state))
    }

    async fn do_get(app: &Router, uri: &str) -> (StatusCode, serde_json::Value) {
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
        let app = test_router().await;
        let (status, body) = do_get(&app, "/health").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn meta_returns_version() {
        let app = test_router().await;
        let (status, body) = do_get(&app, "/api/meta").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["version"].is_string());
        assert_eq!(body["service"], "beacon");
    }

    #[tokio::test]
    async fn openapi_returns_json() {
        let app = test_router().await;
        let (status, body) = do_get(&app, "/api-docs/openapi.json").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["openapi"], "3.1.0");
        assert_eq!(body["info"]["title"], "beacon");
    }
}
