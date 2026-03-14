//! API route definitions and shared application state.

pub mod authcompare;
pub mod check;
pub mod compare;
pub mod dnssec;
pub mod meta;
pub mod parse;
pub mod query;
pub mod results;
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
use netray_common::enrichment::EnrichmentClient;
use crate::reload::HotState;
use crate::result_cache::ResultCache;
use crate::security::IpExtractor;

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
    /// Transport used for this batch (compare mode only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    /// Source of this batch: "recursive" or "authoritative" (auth mode only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Shared state passed to all API handlers via axum's `State` extractor.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub circuit_breakers: Arc<CircuitBreakerRegistry>,
    pub ip_extractor: Arc<IpExtractor>,
    pub result_cache: Arc<ResultCache>,
    pub hot_state: HotState,
    pub ip_enrichment: Option<Arc<EnrichmentClient>>,
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
        dnssec::post_handler,
        compare::post_handler,
        authcompare::post_handler,
        parse::parse_handler,
        meta::servers,
        meta::record_types,
        meta::health,
        meta::ready,
        meta::client_config,
    ),
    components(schemas(
        query::PostQueryRequest,
        check::CheckRequest,
        trace::TraceRequest,
        dnssec::DnssecRequest,
        parse::ParseRequest,
        parse::ParseResponse,
        parse::TokenInfo,
        parse::Completion,
        meta::HealthResponse,
        meta::ReadyResponse,
        meta::ServerInfo,
        meta::ServerConfigInfo,
        meta::RecordTypeInfo,
        meta::ClientConfig,
        ErrorResponse,
        ErrorInfo,
    )),
    tags(
        (name = "Query", description = "DNS lookups with multi-server fan-out"),
        (name = "Check", description = "Comprehensive DNS health check with lint analysis"),
        (name = "Trace", description = "DNS delegation chain walk from root to authoritative"),
        (name = "DNSSEC", description = "DNSSEC chain-of-trust validation from root to authoritative"),
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
        .route("/api/ready", get(meta::ready))
        .route("/api/servers", get(meta::servers))
        .route("/api/record-types", get(meta::record_types))
        .route("/api/config", get(meta::client_config))
        .route("/api/check", post(check::post_handler))
        .route("/api/trace", post(trace::post_handler))
        .route("/api/dnssec", post(dnssec::post_handler))
        .route("/api/compare", post(compare::post_handler))
        .route("/api/authcompare", post(authcompare::post_handler))
        .route("/api/parse", post(parse::parse_handler))
        .route("/api/results/{key}", get(results::get_handler))
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

// ---------------------------------------------------------------------------
// HTTP handler integration tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;

    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::circuit_breaker::CircuitBreakerRegistry;
    use crate::config::Config;
    use crate::reload::HotState;
    use crate::result_cache::ResultCache;
    use crate::security::IpExtractor;

    use super::{AppState, api_router, health_router};

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    /// Build an `AppState` from default config.
    fn default_state() -> AppState {
        let config = Config::load(None).expect("default config must be valid");
        let hot_state = HotState::new(&config);
        AppState {
            circuit_breakers: Arc::new(CircuitBreakerRegistry::new(&config.circuit_breaker)),
            ip_extractor: Arc::new(
                IpExtractor::new(&config.server.trusted_proxies)
                    .expect("invalid trusted_proxies configuration"),
            ),
            result_cache: Arc::new(ResultCache::new()),
            hot_state,
            ip_enrichment: None,
            config: Arc::new(config),
        }
    }

    /// Build the combined test router (health + api).
    ///
    /// Includes the `request_id_middleware` so the `Extension(request_id)`
    /// extractor in SSE handlers is satisfied.
    fn test_router(state: AppState) -> axum::Router {
        health_router()
            .merge(api_router(state))
            .layer(axum::middleware::from_fn(crate::request_id_middleware))
    }

    /// A loopback peer address used for all test requests.
    fn test_peer() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345)
    }

    /// Build a `Request` with the required `ConnectInfo<SocketAddr>` extension
    /// pre-populated so axum's `ConnectInfo` extractor succeeds in tests.
    fn get(uri: &str) -> Request<Body> {
        let peer = test_peer();
        Request::builder()
            .uri(uri)
            .extension(ConnectInfo::<SocketAddr>(peer))
            .body(Body::empty())
            .unwrap()
    }

    fn post_json(uri: &str, body: &str) -> Request<Body> {
        let peer = test_peer();
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .extension(ConnectInfo::<SocketAddr>(peer))
            .body(Body::from(body.to_owned()))
            .unwrap()
    }

    /// Consume a response body to a UTF-8 string.
    async fn body_string(body: axum::body::Body) -> String {
        let bytes = body.collect().await.unwrap().to_bytes();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    // -----------------------------------------------------------------------
    // GET /api/health
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn health_returns_200_with_ok_status() {
        let router = test_router(default_state());
        let resp = router.oneshot(get("/api/health")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_string(resp.into_body()).await;
        assert!(body.contains("\"ok\""), "body: {body}");
    }

    // -----------------------------------------------------------------------
    // GET /api/ready
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn ready_returns_200_when_no_open_circuit_breakers() {
        let router = test_router(default_state());
        let resp = router.oneshot(get("/api/ready")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_string(resp.into_body()).await;
        assert!(body.contains("\"ready\""), "body: {body}");
    }

    // -----------------------------------------------------------------------
    // GET /api/servers
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn servers_returns_200_with_non_empty_array() {
        let router = test_router(default_state());
        let resp = router.oneshot(get("/api/servers")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_string(resp.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert!(json.is_array());
        assert!(
            !json.as_array().unwrap().is_empty(),
            "servers array must not be empty"
        );
    }

    // -----------------------------------------------------------------------
    // GET /api/record-types
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn record_types_returns_200_with_non_empty_array() {
        let router = test_router(default_state());
        let resp = router.oneshot(get("/api/record-types")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_string(resp.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert!(json.is_array());
        assert!(
            !json.as_array().unwrap().is_empty(),
            "record-types array must not be empty"
        );
    }

    // -----------------------------------------------------------------------
    // POST /api/parse
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn parse_with_valid_input_returns_200_with_completions() {
        let router = test_router(default_state());
        let resp = router
            .oneshot(post_json("/api/parse", r#"{"input":"example.com @cl"}"#))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_string(resp.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert!(json["completions"].is_array(), "body: {body}");
        assert!(!json["completions"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn parse_with_invalid_json_returns_400() {
        let router = test_router(default_state());
        let resp = router
            .oneshot(post_json("/api/parse", "not-json"))
            .await
            .unwrap();
        // axum's Json extractor returns 400 for a syntactically invalid body.
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // -----------------------------------------------------------------------
    // POST /api/query — error cases
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn query_post_with_invalid_json_returns_400() {
        let router = test_router(default_state());
        let resp = router
            .oneshot(post_json("/api/query", "not-json"))
            .await
            .unwrap();
        // axum's Json extractor returns 400 for a syntactically invalid body.
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn query_post_with_missing_domain_returns_422() {
        let router = test_router(default_state());
        let resp = router
            .oneshot(post_json("/api/query", r#"{"record_types":["A"]}"#))
            .await
            .unwrap();
        // Missing required `domain` field — axum deserializes to default empty string,
        // which the handler rejects as INVALID_DOMAIN (400).
        assert!(
            resp.status() == StatusCode::BAD_REQUEST
                || resp.status() == StatusCode::UNPROCESSABLE_ENTITY,
            "status: {}",
            resp.status()
        );
    }

    #[tokio::test]
    async fn query_post_with_private_ip_server_returns_422() {
        let router = test_router(default_state());
        // Default config has allow_arbitrary_servers = false, so an IP server
        // is rejected before the IP check (ArbitraryServersDisabled → 422).
        let resp = router
            .oneshot(post_json(
                "/api/query",
                r#"{"domain":"example.com","servers":["192.168.1.1"]}"#,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_string(resp.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        let code = json["error"]["code"].as_str().unwrap_or("");
        assert!(
            code == "ARBITRARY_SERVERS_DISABLED" || code == "BLOCKED_TARGET_IP",
            "unexpected error code: {code}"
        );
    }

    #[tokio::test]
    async fn query_post_with_too_many_record_types_returns_422() {
        // Default max_record_types = 10; send 11.
        let types = r#"["A","AAAA","MX","TXT","NS","SOA","CNAME","CAA","SRV","HTTPS","SVCB"]"#;
        let body = format!(r#"{{"domain":"example.com","record_types":{types}}}"#);
        let router = test_router(default_state());
        let resp = router
            .oneshot(post_json("/api/query", &body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let resp_body = body_string(resp.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&resp_body).expect("valid JSON");
        assert_eq!(
            json["error"]["code"].as_str().unwrap(),
            "TOO_MANY_RECORD_TYPES"
        );
    }

    // -----------------------------------------------------------------------
    // POST /api/check — error cases
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn check_post_with_invalid_json_returns_400() {
        let router = test_router(default_state());
        let resp = router
            .oneshot(post_json("/api/check", "not-json"))
            .await
            .unwrap();
        // axum's Json extractor returns 400 for a syntactically invalid body.
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn check_post_with_private_ip_server_returns_422() {
        let router = test_router(default_state());
        let resp = router
            .oneshot(post_json(
                "/api/check",
                r#"{"domain":"example.com","servers":["192.168.1.1"]}"#,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_string(resp.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        let code = json["error"]["code"].as_str().unwrap_or("");
        assert!(
            code == "ARBITRARY_SERVERS_DISABLED" || code == "BLOCKED_TARGET_IP",
            "unexpected error code: {code}"
        );
    }

    // -----------------------------------------------------------------------
    // Rate limiting — per-IP limit eventually returns 429
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn rate_limit_eventually_returns_429() {
        // Build a state with a tiny per-IP rate limit so we can exhaust it
        // without sending hundreds of requests.
        use crate::config::{
            CircuitBreakerConfig, DnsConfig, EcosystemConfig, LimitsConfig, ServerConfig,
            TelemetryConfig, TraceConfig,
        };

        let config = Config {
            site_name: "prism".to_string(),
            server: ServerConfig {
                bind: ([127, 0, 0, 1], 8080).into(),
                metrics_bind: ([127, 0, 0, 1], 9090).into(),
                trusted_proxies: Vec::new(),
            },
            limits: LimitsConfig {
                per_ip_per_minute: 1,
                per_ip_burst: 1,
                per_target_per_minute: 60,
                per_target_burst: 20,
                global_per_minute: 1000,
                global_burst: 50,
                max_concurrent_connections: 256,
                per_ip_max_streams: 10,
                max_timeout_secs: 10,
                max_record_types: 10,
                max_servers: 4,
            },
            circuit_breaker: CircuitBreakerConfig {
                window_secs: 60,
                cooldown_secs: 30,
                failure_threshold: 0.5,
                min_requests: 5,
            },
            dns: DnsConfig {
                default_servers: vec!["cloudflare".to_owned()],
                allow_system_resolvers: true,
                allow_arbitrary_servers: false,
            },
            trace: TraceConfig {
                max_hops: 10,
                query_timeout_secs: 3,
            },
            telemetry: TelemetryConfig::default(),
            ecosystem: EcosystemConfig::default(),
        };

        let hot_state = HotState::new(&config);
        let state = AppState {
            circuit_breakers: Arc::new(CircuitBreakerRegistry::new(&config.circuit_breaker)),
            ip_extractor: Arc::new(
                IpExtractor::new(&config.server.trusted_proxies)
                    .expect("invalid trusted_proxies configuration"),
            ),
            result_cache: Arc::new(ResultCache::new()),
            hot_state,
            ip_enrichment: None,
            config: Arc::new(config),
        };

        // /api/ready has no rate limiting applied in this router but does go
        // through the same middleware. Use /api/servers (GET, no DNS) as the
        // probe — but rate limiting is applied per handler in execute_query,
        // not as middleware. So we need an endpoint that invokes rate limiting.
        //
        // Rate limiting is enforced inside execute_query() for query/check/trace.
        // For a quick test, use POST /api/query with a valid payload but trigger
        // the per-IP check. Since cost = record_types × servers = 1 × 1 = 1
        // and burst = 1, the second request should be rate-limited.
        //
        // However, execute_query() proceeds to build resolvers before we get
        // a 429 back — and we don't want real DNS. Instead, test the rate
        // limiter directly via the public `check_query_cost` API to verify the
        // 429 path is wired.
        //
        // For the HTTP-level 429 test we rely on the fact that rate limiting
        // happens before resolver construction. We send two identical requests
        // to POST /api/query with default servers. The rate limiter will fire
        // on the second one (burst=1). The response for the second should be
        // 429 regardless of whether DNS succeeds or fails.

        let make_query_req = || {
            post_json(
                "/api/query",
                r#"{"domain":"example.com","record_types":["A"]}"#,
            )
        };

        // First request: should be accepted (200 SSE stream or proceed past rate limiter).
        let router = test_router(state.clone());
        let resp1 = router.oneshot(make_query_req()).await.unwrap();
        // It may return 200 (SSE started) or some DNS error — either way it's not 429.
        assert_ne!(
            resp1.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "first request should not be rate-limited"
        );

        // Second request: burst is exhausted, should be 429.
        let router = test_router(state);
        let resp2 = router.oneshot(make_query_req()).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(
            resp2.headers().contains_key("retry-after"),
            "429 response must include Retry-After header"
        );
    }
}
