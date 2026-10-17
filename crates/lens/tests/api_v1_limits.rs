//! The `[api]` per-IP limiter on `/api/v1` (spec v2-api, requirements 8 and 9).
use std::net::SocketAddr;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use lens::config::{
    BackendsConfig, BadgesConfig, CacheConfig, Config, EcosystemConfig, OgCardsConfig,
    RateLimitConfig, ScoringConfig, ServerConfig, SiteConfig, SnapshotsConfig,
};
use lens::state::AppState;
use netray_web::api::ApiConfig;

fn base_config() -> Config {
    Config {
        modules: Default::default(),
        server: ServerConfig {
            bind: ([127, 0, 0, 1], 0).into(),
            metrics_bind: ([127, 0, 0, 1], 0).into(),
            trusted_proxies: Vec::new(),
        },
        backends: BackendsConfig {
            resolve_timeout_ms: 2000,
            dns: lens::config::BackendConfig {
                timeout_ms: 100,
                ..Default::default()
            },
            tls: lens::config::BackendConfig {
                url: Some("http://127.0.0.1:19998".to_string()),
                timeout_ms: 100,
                ..Default::default()
            },
            ip: lens::config::BackendConfig {
                timeout_ms: 100,
                ..Default::default()
            },
            http: None,
            email: None,
        },
        ecosystem: EcosystemConfig::default(),
        cache: CacheConfig {
            enabled: false,
            ttl_seconds: 300,
        },
        telemetry: Default::default(),
        rate_limit: RateLimitConfig {
            per_ip_per_minute: 60,
            per_ip_burst: 10,
            global_per_minute: 1000,
            global_burst: 100,
        },
        scoring: ScoringConfig::default(),
        site: SiteConfig::default(),
        badges: BadgesConfig::default(),
        og_cards: OgCardsConfig::default(),
        snapshots: SnapshotsConfig::default(),
        history: lens::config::HistoryConfig::default(),
        admission: lens::config::AdmissionConfig::default(),
        api: Default::default(),
    }
}

fn app(trusted_proxies: &[&str]) -> Router {
    let mut config = base_config();
    config.server.trusted_proxies = trusted_proxies.iter().map(|s| s.to_string()).collect();
    lens::app(AppState::new(config).unwrap())
}

async fn status_of(
    app: &Router,
    uri: &str,
    peer: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, Option<String>, String) {
    let mut req = Request::builder().uri(uri);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let mut req = req.body(Body::empty()).unwrap();
    let peer: SocketAddr = peer.parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let retry = resp
        .headers()
        .get("retry-after")
        .map(|v| v.to_str().unwrap().to_string());
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, retry, String::from_utf8_lossy(&bytes).into_owned())
}

#[test]
fn api_config_defaults() {
    let c = ApiConfig::default();
    assert!(c.enabled);
    assert_eq!(c.per_ip_per_minute, 60);
    assert_eq!(c.per_ip_burst, 10);
}

#[tokio::test]
async fn eleventh_catalogue_request_in_a_burst_is_rate_limited() {
    let app = app(&[]);
    for i in 0..10 {
        let (s, _, _) = status_of(&app, "/api/v1/catalogue", "192.0.2.1:4000", &[]).await;
        assert_eq!(s, StatusCode::OK, "request {}", i + 1);
    }
    let (s, retry, body) = status_of(&app, "/api/v1/catalogue", "192.0.2.1:4000", &[]).await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert!(retry.is_some(), "Retry-After header missing");
    assert!(body.contains("RATE_LIMITED"), "body: {body}");
}

#[tokio::test]
async fn limiter_guards_profiles_and_meta() {
    for path in ["/api/v1/profiles/3", "/api/v1/meta"] {
        let app = app(&[]);
        for _ in 0..10 {
            let (s, _, _) = status_of(&app, path, "192.0.2.1:4000", &[]).await;
            assert_eq!(s, StatusCode::OK, "{path}");
        }
        let (s, _, _) = status_of(&app, path, "192.0.2.1:4000", &[]).await;
        assert_eq!(s, StatusCode::TOO_MANY_REQUESTS, "{path}");
    }
}

#[tokio::test]
async fn openapi_and_docs_are_never_limited() {
    let app = app(&[]);
    for path in ["/api/v1/openapi.json", "/api/v1/docs"] {
        for i in 0..11 {
            let (s, _, _) = status_of(&app, path, "192.0.2.1:4000", &[]).await;
            assert_eq!(s, StatusCode::OK, "{path} request {}", i + 1);
        }
    }
}

#[tokio::test]
async fn limiter_keys_on_x_real_ip_behind_a_trusted_proxy_not_cf_header() {
    let app = app(&["10.0.0.0/8"]);
    for i in 0..10 {
        let cf = format!("198.51.100.{}", i + 1);
        let (s, _, _) = status_of(
            &app,
            "/api/v1/catalogue",
            "10.0.0.1:5000",
            &[("x-real-ip", "192.0.2.7"), ("cf-connecting-ip", &cf)],
        )
        .await;
        assert_eq!(s, StatusCode::OK, "request {}", i + 1);
    }
    let (s, _, _) = status_of(
        &app,
        "/api/v1/catalogue",
        "10.0.0.1:5000",
        &[
            ("x-real-ip", "192.0.2.7"),
            ("cf-connecting-ip", "198.51.100.99"),
        ],
    )
    .await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);

    let (s, _, _) = status_of(
        &app,
        "/api/v1/catalogue",
        "10.0.0.1:5000",
        &[
            ("x-real-ip", "192.0.2.8"),
            ("cf-connecting-ip", "198.51.100.99"),
        ],
    )
    .await;
    assert_eq!(s, StatusCode::OK);
}
