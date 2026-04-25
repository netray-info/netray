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
use crate::inspect::assembler::{
    CacheControlDirectives, CachingReport, CdnReport, CookieEntry, CorsReport, CspReport,
    EnrichmentInfo, FingerprintReport, HeaderCheck, HstsCheck, HttpUpgrade, InfoLeakage,
    InspectResponse, RedirectHop, ReportingReport, SecurityReport,
};
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

// `MetaResponse` is now `netray_common::ecosystem::EcosystemMeta`; the local
// `EcosystemLinks` type was deleted in favour of `EcosystemUrls`.

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
    info(
        title = "spectra",
        description = "HTTP header inspection and security audit"
    ),
    paths(
        health_handler,
        ready_handler,
        config_handler,
        meta_handler,
        inspect_post_handler,
        inspect_get_handler,
    ),
    components(schemas(
        HealthResponse,
        ReadyResponse,
        ConfigResponse,
        netray_common::ecosystem::EcosystemMeta,
        netray_common::ecosystem::EcosystemUrls,
        netray_common::ecosystem::RateLimitSummary,
        InspectRequest,
        InspectResponse,
        SecurityReport,
        CspReport,
        CookieEntry,
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
    )
)]
async fn ready_handler(State(_state): State<AppState>) -> impl IntoResponse {
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
    get,
    path = "/api/meta",
    responses(
        (status = 200, description = "Service metadata and ecosystem links", body = netray_common::ecosystem::EcosystemMeta),
    )
)]
async fn meta_handler(
    State(state): State<AppState>,
) -> Json<netray_common::ecosystem::EcosystemMeta> {
    use netray_common::ecosystem::{EcosystemMeta, EcosystemUrls, RateLimitSummary};
    use serde_json::{Map, Value, json};

    let meta = &state.config.meta;
    let limits_cfg = &state.config.limits;

    let mut features = Map::new();
    features.insert(
        "ip_enrichment".into(),
        Value::Bool(state.config.enrichment.ip_url.is_some()),
    );

    let mut limits = Map::new();
    limits.insert(
        "request_timeout_secs".into(),
        json!(state.config.inspect.request_timeout_secs),
    );
    limits.insert(
        "total_timeout_secs".into(),
        json!(state.config.inspect.total_timeout_secs),
    );
    limits.insert("max_redirects".into(), json!(state.config.inspect.max_redirects));
    limits.insert(
        "max_concurrent_connections".into(),
        json!(limits_cfg.max_concurrent_connections),
    );

    Json(EcosystemMeta {
        site_name: "spectra".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        ecosystem: EcosystemUrls {
            ip_base_url: meta.ip_base_url.clone().unwrap_or_default(),
            dns_base_url: meta.dns_base_url.clone().unwrap_or_default(),
            tls_base_url: meta.tls_base_url.clone().unwrap_or_default(),
            http_base_url: meta.http_base_url.clone().unwrap_or_default(),
            email_base_url: String::new(),
            lens_base_url: meta.lens_base_url.clone().unwrap_or_default(),
        },
        features,
        limits,
        rate_limit: RateLimitSummary {
            per_ip_per_minute: limits_cfg.per_ip_per_minute,
            per_ip_burst: limits_cfg.per_ip_burst,
            global_per_minute: 0,
            global_burst: 0,
        },
    })
}

#[utoipa::path(
    post,
    path = "/api/inspect",
    request_body = InspectRequest,
    responses(
        (status = 200, description = "Inspection result", body = InspectResponse),
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
        (status = 200, description = "Inspection result", body = InspectResponse),
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
    let result = do_inspect_inner(state, peer, req_headers, raw_url).await;
    let outcome = match &result {
        Ok(_) => "success",
        Err(AppError::RateLimited { .. }) => "rate_limited",
        Err(AppError::BlockedTarget(_)) => "blocked",
        Err(AppError::Timeout(_)) => "timeout",
        Err(_) => "error",
    };
    metrics::counter!("spectra_inspect_requests_total", "outcome" => outcome).increment(1);
    result
}

async fn do_inspect_inner(
    state: &AppState,
    peer: SocketAddr,
    req_headers: &axum::http::HeaderMap,
    raw_url: &str,
) -> Result<Json<InspectResponse>, AppError> {
    let start = Instant::now();

    // 1. Parse and normalize URL
    let url = crate::input::parse_url(raw_url)?;

    // 2. Rate limiting (before DNS to avoid unnecessary resolution for throttled clients)
    let client_ip = state.ip_extractor.extract(req_headers, peer);
    tracing::Span::current().record("client_ip", tracing::field::display(client_ip));
    let hostname = url.host_str().unwrap_or_default();
    state.rate_limiter.check(client_ip, hostname)?;

    // 3. Resolve and validate target (SSRF check)
    let resolved_addr = crate::input::validate_target(&url).await?;

    // 4. Execute inspection
    let request_id = req_headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    tracing::Span::current().record("request_id", request_id.as_str());
    let result = inspect::inspect(
        &url,
        resolved_addr,
        &state.config.inspect,
        state.http_client.as_ref(),
    )
    .await?;

    // 5. IP enrichment (non-blocking, failure is OK)
    let enrichment = if let Some(ref client) = state.enrichment_client {
        match client.lookup(resolved_addr.ip(), None).await {
            Some(info) => {
                let threat = if info.is_c2 {
                    Some("C2".to_string())
                } else if info.is_spamhaus {
                    Some("DROP".to_string())
                } else if info.is_tor {
                    Some("TOR".to_string())
                } else {
                    None
                };
                inspect::EnrichmentData {
                    org: info.org,
                    ip_type: info.ip_type,
                    threat,
                    role: info.network_role,
                }
            }
            None => inspect::EnrichmentData::default(),
        }
    } else {
        inspect::EnrichmentData::default()
    };

    let enrichment_base_url = state
        .config
        .enrichment
        .ip_url
        .as_deref()
        .unwrap_or("https://ip.netray.info");

    let duration_ms = start.elapsed().as_millis() as u64;
    metrics::histogram!("spectra_inspect_duration_ms").record(duration_ms as f64);

    // 6. Assemble response
    let response = inspect::assemble_response(
        &url,
        resolved_addr,
        result,
        enrichment,
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
    async fn meta_returns_version_and_site_name() {
        let app = test_router();
        let (status, body) = get(&app, "/api/meta").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["version"].is_string());
        assert_eq!(body["site_name"], "spectra");
    }

    #[tokio::test]
    async fn openapi_returns_json() {
        let app = test_router();
        let (status, body) = get(&app, "/api-docs/openapi.json").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["openapi"], "3.1.0");
        assert_eq!(body["info"]["title"], "spectra");
    }

    async fn post_json(app: &Router, uri: &str, json: &str) -> (StatusCode, serde_json::Value) {
        use axum::extract::connect_info::MockConnectInfo;
        use std::net::{IpAddr, Ipv4Addr};
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1)), 12345);
        let app = app.clone().layer(MockConnectInfo(peer));
        let response = app
            .oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri(uri)
                    .header("content-type", "application/json")
                    .body(Body::from(json.to_string()))
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
    async fn malformed_url_returns_400() {
        let app = test_router();
        // http:// scheme is explicitly rejected (port-80 probing is automatic)
        let (status, body) =
            post_json(&app, "/api/inspect", r#"{"url":"http://example.com"}"#).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "INVALID_URL");
    }

    #[tokio::test]
    async fn ssrf_blocked_returns_403() {
        let app = test_router();
        let (status, body) = post_json(&app, "/api/inspect", r#"{"url":"https://10.0.0.1"}"#).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"]["code"], "TARGET_BLOCKED");
    }

    #[tokio::test]
    async fn missing_url_field_returns_400() {
        let app = test_router();
        let (status, body) = post_json(&app, "/api/inspect", r#"{}"#).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "INVALID_URL");
    }
}
