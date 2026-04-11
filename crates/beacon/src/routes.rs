use std::convert::Infallible;
use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Path, Query, State};
use axum::response::sse::KeepAlive;
use axum::response::{Html, IntoResponse, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tracing::Instrument;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ecosystem: Option<netray_common::ecosystem::EcosystemConfig>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct InspectRequest {
    pub domain: String,
    #[serde(default)]
    pub dkim_selectors: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct InspectQuery {
    #[serde(default)]
    pub selector: Vec<String>,
}

// ---------------------------------------------------------------------------
// OpenAPI
// ---------------------------------------------------------------------------

#[derive(OpenApi)]
#[openapi(
    info(
        title = "beacon",
        description = "Email security inspector — DNS-only mail posture analysis"
    ),
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
        netray_common::ecosystem::EcosystemConfig,
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
// Shared inspect logic
// ---------------------------------------------------------------------------

#[tracing::instrument(
    skip(state),
    fields(client_ip = tracing::field::Empty, domain = tracing::field::Empty)
)]
async fn do_inspect(
    domain: String,
    selectors: Vec<String>,
    client_ip: std::net::IpAddr,
    state: AppState,
) -> Result<
    Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, Infallible>>>,
    MailError,
> {
    tracing::Span::current().record("domain", domain.as_str());
    tracing::Span::current().record("client_ip", client_ip.to_string().as_str());

    let (tx, rx) = tokio::sync::mpsc::channel::<crate::quality::SseEvent>(32);

    let span = tracing::Span::current();
    tokio::spawn(
        checks::run_all_checks(
            domain,
            selectors,
            state.config.clone(),
            state.dns_resolver.clone(),
            state.http_client.clone(),
            state.http_client_follow.clone(),
            state.enrichment_client.clone(),
            tx,
        )
        .instrument(span),
    );

    let stream = ReceiverStream::new(rx).map(|event| {
        let sse_event: axum::response::sse::Event = event.into();
        Ok::<_, Infallible>(sse_event)
    });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keepalive"),
    ))
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
            Json(ReadyResponse { status: "ready" }),
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
async fn meta_handler(State(state): State<AppState>) -> Json<MetaResponse> {
    let eco = &state.config.ecosystem;
    let ecosystem = if eco.has_any() {
        Some(eco.clone())
    } else {
        None
    };

    Json(MetaResponse {
        version: env!("CARGO_PKG_VERSION"),
        service: "beacon",
        ecosystem,
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
) -> Result<
    Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, Infallible>>>,
    MailError,
> {
    let domain = crate::input::parse_domain(&body.domain)?;

    if body.dkim_selectors.len() > state.config.dkim.max_user_selectors {
        return Err(MailError::TooManySelectors {
            max: state.config.dkim.max_user_selectors,
        });
    }

    for s in &body.dkim_selectors {
        crate::input::validate_dkim_selector(s)?;
    }

    let client_ip = state.ip_extractor.extract(&headers, peer);
    if let Err(e) = state.rate_limiter.check(client_ip) {
        metrics::counter!("beacon_rate_limit_rejections_total").increment(1);
        return Err(e);
    }

    metrics::counter!("beacon_requests_total", "endpoint" => "inspect", "method" => "post")
        .increment(1);

    do_inspect(domain, body.dkim_selectors, client_ip, state).await
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
    Query(query): Query<InspectQuery>,
) -> Result<
    Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, Infallible>>>,
    MailError,
> {
    let decoded = percent_encoding::percent_decode_str(&raw_domain)
        .decode_utf8_lossy()
        .to_string();
    let domain = crate::input::parse_domain(&decoded)?;

    if query.selector.len() > state.config.dkim.max_user_selectors {
        return Err(MailError::TooManySelectors {
            max: state.config.dkim.max_user_selectors,
        });
    }

    for s in &query.selector {
        crate::input::validate_dkim_selector(s)?;
    }

    let client_ip = state.ip_extractor.extract(&headers, peer);
    if let Err(e) = state.rate_limiter.check(client_ip) {
        metrics::counter!("beacon_rate_limit_rejections_total").increment(1);
        return Err(e);
    }

    metrics::counter!("beacon_requests_total", "endpoint" => "inspect", "method" => "get")
        .increment(1);

    do_inspect(domain, query.selector, client_ip, state).await
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
            .oneshot(HttpRequest::builder().uri(uri).body(Body::empty()).unwrap())
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
    async fn meta_includes_ecosystem_when_configured() {
        let mut config = crate::config::Config::load(None).unwrap();
        config.ecosystem = netray_common::ecosystem::EcosystemConfig {
            ip_base_url: Some("https://ip.example.com".to_string()),
            dns_base_url: Some("https://dns.example.com".to_string()),
            ..Default::default()
        };
        let state = AppState::new(&config).await.unwrap();
        let app = health_router(state.clone()).merge(api_router(state));
        let (status, body) = do_get(&app, "/api/meta").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["service"], "beacon");
        let eco = &body["ecosystem"];
        assert!(
            eco.is_object(),
            "ecosystem should be present when configured"
        );
        assert_eq!(eco["ip_base_url"], "https://ip.example.com");
        assert_eq!(eco["dns_base_url"], "https://dns.example.com");
        // Unconfigured fields should be absent (skip_serializing_if)
        assert!(eco.get("tls_base_url").is_none() || eco["tls_base_url"].is_null());
    }

    #[tokio::test]
    async fn meta_omits_ecosystem_when_unconfigured() {
        let app = test_router().await;
        let (_, body) = do_get(&app, "/api/meta").await;
        // Default config has no ecosystem set — field should be absent
        assert!(
            body.get("ecosystem").is_none() || body["ecosystem"].is_null(),
            "ecosystem should be absent when not configured"
        );
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
