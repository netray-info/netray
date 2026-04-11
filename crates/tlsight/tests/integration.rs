//! Integration tests for the tlsight HTTP API.
//!
//! These tests spin up the router in-process using `tower::ServiceExt::oneshot`
//! with pre-populated `ConnectInfo<SocketAddr>` and `RequestId` extensions,
//! following the pattern used in `src/state.rs` unit tests.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use tlsight::config::Config;
use tlsight::routes::{api_router, health_router};
use tlsight::state::AppState;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ensure_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn default_config() -> Config {
    Config::load(None).expect("default config must be valid")
}

fn default_state(config: &Config) -> AppState {
    AppState::new(config)
}

fn test_router(state: AppState) -> axum::Router {
    health_router(state.clone())
        .merge(api_router(state))
        .layer(axum::middleware::from_fn(
            netray_common::middleware::request_id,
        ))
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
// 1. GET /health → 200 {"status":"ok"}
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health_returns_200() {
    ensure_crypto_provider();
    let config = default_config();
    let router = test_router(default_state(&config));
    let resp = router.oneshot(get("/health")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert_eq!(json["status"], "ok", "body: {body}");
}

// ---------------------------------------------------------------------------
// 2. GET /ready → 200 or 503 with JSON body containing "status" field.
//
// Default config has check_caa=true and check_dane=true; since dns_resolver
// is not initialized in AppState::new(), ready_handler returns 503.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ready_returns_200_or_503_with_status_field() {
    ensure_crypto_provider();
    let config = default_config();
    let router = test_router(default_state(&config));
    let resp = router.oneshot(get("/ready")).await.unwrap();
    let status = resp.status();
    assert!(
        status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE,
        "unexpected status: {status}"
    );
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert!(
        json["status"].is_string(),
        "missing status field; body: {body}"
    );
}

// ---------------------------------------------------------------------------
// 3. POST /api/inspect with RFC 1918 IP → 403 (BLOCKED_TARGET).
//
// 192.168.1.1 parses as Target::Ip, passes input validation, is then filtered
// by check_allowed_with_policy (private address) → all IPs blocked →
// AppError::BlockedTarget → 403.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rfc1918_target_returns_403() {
    ensure_crypto_provider();
    let config = default_config();
    let router = test_router(default_state(&config));
    let resp = router
        .oneshot(post_json("/api/inspect", r#"{"hostname":"192.168.1.1"}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert_eq!(json["error"]["code"], "BLOCKED_TARGET", "body: {body}");
}

// ---------------------------------------------------------------------------
// 4. Malformed input: POST /api/inspect with empty hostname → 400
//
// parse_input("") returns AppError::ParseError("empty input") → 400.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn empty_hostname_returns_400() {
    ensure_crypto_provider();
    let config = default_config();
    let router = test_router(default_state(&config));
    let resp = router
        .oneshot(post_json("/api/inspect", r#"{"hostname":""}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert!(
        json["error"]["code"].is_string(),
        "missing error code; body: {body}"
    );
}

// ---------------------------------------------------------------------------
// 5. Rate limiting: rapid POST /api/inspect requests eventually return 429.
//
// Build a state with per_ip_burst=1 so the second request exhausts the budget
// before any TLS handshake is attempted. Uses a blocked IP (127.0.0.1) as the
// target so even the first request hits the rate limiter path and returns
// immediately without network I/O (the inspect handler applies rate limiting
// before TLS connect).
//
// Note: rate limiting in tlsight's do_inspect fires after IP resolution and
// filtering. For a hostname-based target, the service would attempt DNS.
// We use allow_blocked_targets=true with loopback so the IP passes filtering
// but the first request consumes the burst, and the second is rejected.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rate_limit_returns_429() {
    ensure_crypto_provider();

    use tlsight::config::{
        DnsConfig, EcosystemConfig, LimitsConfig, QualityConfig, ServerConfig, TelemetryConfig,
        ValidationConfig,
    };

    let config = Config {
        site_name: "tlsight".to_string(),
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
            max_concurrent_connections: 256,
            max_concurrent_handshakes: 10,
            handshake_timeout_secs: 1,
            request_timeout_secs: 3,
            max_ports: 7,
            max_ips_per_hostname: 10,
            max_domain_length: 253,
            // Allow blocked targets so that loopback passes filtering,
            // ensuring rate limiting fires on both requests.
            allow_blocked_targets: true,
        },
        dns: DnsConfig { timeout_secs: 3 },
        validation: ValidationConfig {
            expiry_warning_days: 30,
            expiry_critical_days: 14,
            check_dane: false,
            check_caa: false,
            check_ct: false,
            custom_ca_dir: None,
        },
        ecosystem: EcosystemConfig::default(),
        quality: QualityConfig {
            enabled: false,
            http_check_timeout_secs: 5,
            skip_http_checks: true,
        },
        telemetry: TelemetryConfig::default(),
        backends: tlsight::config::BackendsConfig::default(),
    };

    // Use Arc<ArcSwap<Config>> as AppState::new expects it stored via ArcSwap.
    let state = AppState::new(&config);

    // Use a public IP that resolves immediately when allow_blocked_targets=true.
    // We use an IP address directly (no DNS) so the test is deterministic.
    // The first request should pass the rate limiter (cost=1, burst=1).
    // The second request exhausts the burst → 429.
    let make_req = || post_json("/api/inspect", r#"{"hostname":"127.0.0.1"}"#);

    // First request: rate limit not yet exhausted.
    let router = test_router(state.clone());
    let resp1 = router.oneshot(make_req()).await.unwrap();
    // The first request may fail with 502 (TLS connect refused) or succeed — either
    // way it must not be 429.
    assert_ne!(
        resp1.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "first request must not be rate-limited"
    );

    // Second request: burst exhausted → 429.
    let router = test_router(state);
    let resp2 = router.oneshot(make_req()).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::TOO_MANY_REQUESTS);
    let body = body_string(resp2.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert_eq!(json["error"]["code"], "RATE_LIMITED", "body: {body}");
}
