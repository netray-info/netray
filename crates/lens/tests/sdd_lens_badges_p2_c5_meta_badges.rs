/// Tests SDD R19: GET /api/meta includes features.badges bool.
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use lens::config::{BadgesConfig, CacheConfig};
use lens::routes::api_router;
use lens::state::AppState;

fn make_state(badges_enabled: bool) -> AppState {
    use lens::config::{
        BackendsConfig, Config, EcosystemConfig, RateLimitConfig, ScoringConfig, ServerConfig,
        SiteConfig,
    };
    let config = Config {
        server: ServerConfig {
            bind: ([127, 0, 0, 1], 0).into(),
            metrics_bind: ([127, 0, 0, 1], 0).into(),
            trusted_proxies: Vec::new(),
        },
        backends: BackendsConfig {
            dns: netray_common::backend::BackendConfig {
                url: Some("http://127.0.0.1:19999".to_string()),
                timeout_ms: 1000,
                ..Default::default()
            },
            dns_servers: Vec::new(),
            tls: netray_common::backend::BackendConfig {
                url: Some("http://127.0.0.1:19998".to_string()),
                timeout_ms: 1000,
                ..Default::default()
            },
            ip: netray_common::backend::BackendConfig {
                url: Some("http://127.0.0.1:19997".to_string()),
                timeout_ms: 1000,
                ..Default::default()
            },
            http: None,
            email: None,
        },
        ecosystem: EcosystemConfig::default(),
        cache: CacheConfig { enabled: false, ttl_seconds: 300 },
        telemetry: Default::default(),
        rate_limit: RateLimitConfig {
            per_ip_per_minute: 60,
            per_ip_burst: 10,
            global_per_minute: 1000,
            global_burst: 100,
        },
        scoring: ScoringConfig::default(),
        site: SiteConfig::default(),
        badges: BadgesConfig {
            enabled: badges_enabled,
            ..BadgesConfig::default()
        },
    };
    AppState::new(config).unwrap()
}

fn api_app(state: AppState) -> Router {
    let (router, _) = api_router().split_for_parts();
    router.with_state(state)
}

#[tokio::test]
async fn meta_includes_badges_true_when_enabled() {
    let app = api_app(make_state(true));
    let req = Request::builder()
        .uri("/api/meta")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 65536).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        json["features"]["badges"],
        serde_json::Value::Bool(true),
        "features.badges must be true when enabled"
    );
}

#[tokio::test]
async fn meta_includes_badges_false_when_disabled() {
    let app = api_app(make_state(false));
    let req = Request::builder()
        .uri("/api/meta")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 65536).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        json["features"]["badges"],
        serde_json::Value::Bool(false),
        "features.badges must be false when disabled"
    );
}
