//! Integration tests for the prism HTTP API.
//!
//! These tests spin up the router in-process using `tower::ServiceExt::oneshot`
//! with a pre-populated `ConnectInfo<SocketAddr>` extension, following the same
//! pattern already established in `src/api/mod.rs`.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Bring internal modules into scope (integration tests link against the crate).
// ---------------------------------------------------------------------------

use prism::api::{AppState, api_router, health_router};
use prism::circuit_breaker::CircuitBreakerRegistry;
use prism::config::Config;
use prism::reload::HotState;
use prism::result_cache::ResultCache;
use prism::security::IpExtractor;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

fn test_router(state: AppState) -> axum::Router {
    health_router()
        .merge(api_router(state))
        .layer(axum::middleware::from_fn(prism::request_id_middleware))
}

fn test_peer() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345)
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .extension(ConnectInfo::<SocketAddr>(test_peer()))
        .body(Body::empty())
        .unwrap()
}

fn post_json(uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .extension(ConnectInfo::<SocketAddr>(test_peer()))
        .body(Body::from(body.to_owned()))
        .unwrap()
}

async fn body_string(body: axum::body::Body) -> String {
    let bytes = body.collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

// ---------------------------------------------------------------------------
// 1. GET /api/health → 200 {"status":"ok"}
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health_returns_200() {
    let router = test_router(default_state());
    let resp = router.oneshot(get("/api/health")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert_eq!(json["status"], "ok", "body: {body}");
}

// ---------------------------------------------------------------------------
// 2. GET /api/ready → 200 or 503 with JSON body containing "status" field
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ready_returns_200_or_503_with_status_field() {
    let router = test_router(default_state());
    let resp = router.oneshot(get("/api/ready")).await.unwrap();
    let status = resp.status();
    assert!(
        status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE,
        "unexpected status: {status}"
    );
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert!(json["status"].is_string(), "missing status field; body: {body}");
}

// ---------------------------------------------------------------------------
// 3. GET /api/query?q=example.com%20ANY → 400 (BLOCKED_QUERY_TYPE via parser)
//
// The GET handler uses the dig-inspired query language parser which blocks ANY,
// AXFR, and IXFR. Blocked query types are surfaced as ParseError → 400.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn blocked_query_type_any_returns_400() {
    let router = test_router(default_state());
    // The query string parser rejects ANY and maps it to ParseError → 400.
    let resp = router
        .oneshot(get("/api/query?q=example.com%20ANY"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    let code = json["error"]["code"].as_str().unwrap_or("");
    assert_eq!(code, "PARSE_ERROR", "body: {body}");
}

// ---------------------------------------------------------------------------
// 4. POST /api/query with RFC 1918 server IP → 422 (BLOCKED_TARGET_IP or
//    ARBITRARY_SERVERS_DISABLED, depending on default config)
//
// Default config has allow_arbitrary_servers = false, so an IP server is
// rejected as ARBITRARY_SERVERS_DISABLED before the IP range check.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn blocked_target_ip_as_server_returns_422() {
    let router = test_router(default_state());
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
        "unexpected error code: {code}; body: {body}"
    );
}

// ---------------------------------------------------------------------------
// 5. Rate limiting: rapid requests eventually return 429 with Retry-After.
//
// Uses a custom state with burst=1 so the second request exhausts the budget.
// Rate limiting is enforced inside execute_query, which runs before resolver
// construction, so no real DNS queries are made.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rate_limit_returns_429() {
    use prism::config::{
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

    let make_req = || {
        post_json(
            "/api/query",
            r#"{"domain":"example.com","record_types":["A"]}"#,
        )
    };

    // First request should be accepted (rate limit not yet exhausted).
    let router = test_router(state.clone());
    let resp1 = router.oneshot(make_req()).await.unwrap();
    assert_ne!(
        resp1.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "first request must not be rate-limited"
    );

    // Second request: burst exhausted — must be 429.
    let router = test_router(state);
    let resp2 = router.oneshot(make_req()).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(
        resp2.headers().contains_key("retry-after"),
        "429 response must include Retry-After header"
    );
}
