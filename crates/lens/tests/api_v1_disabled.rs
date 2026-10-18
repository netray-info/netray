//! `[api] enabled = false` leaves every `/api/v1` path to lens as before (spec v2-api, requirement 8):
//! no JSON 413 and no relaxed docs CSP from v1.
use std::net::SocketAddr;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use lens::config::{
    BackendsConfig, BadgesConfig, CacheConfig, Config, EcosystemConfig, OgCardsConfig,
    RateLimitConfig, ScoringConfig, ServerConfig, SiteConfig, SnapshotsConfig,
};
use lens::state::AppState;

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

fn app(config: Config) -> Router {
    lens::app(AppState::new(config).unwrap())
}

async fn send(app: &Router, mut req: Request<Body>) -> (StatusCode, Option<String>, String) {
    let peer: SocketAddr = "203.0.113.9:4000".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let ct = resp
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_string());
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, ct, String::from_utf8_lossy(&bytes).into_owned())
}

fn is_json(ct: &Option<String>) -> bool {
    ct.as_deref()
        .is_some_and(|c| c.starts_with("application/json"))
}

#[tokio::test]
async fn disabled_api_answers_no_json_413_and_no_relaxed_csp_under_v1() {
    let mut config = base_config();
    config.api.enabled = false;
    let app = app(config);
    let req = Request::get("/api/v1/catalogue")
        .header("content-length", "9000")
        .body(Body::from(vec![b'a'; 9000]))
        .unwrap();
    let (s, ct, body) = send(&app, req).await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(!is_json(&ct), "v1 is off, yet a JSON 413: {ct:?} {body}");

    let csp = |path: &'static str| {
        let app = app.clone();
        async move {
            let mut req = Request::get(path).body(Body::empty()).unwrap();
            req.extensions_mut().insert(ConnectInfo(
                "203.0.113.9:4000".parse::<SocketAddr>().unwrap(),
            ));
            let resp = app.oneshot(req).await.unwrap();
            resp.headers()
                .get("content-security-policy")
                .map(|v| v.to_str().unwrap().to_string())
        }
    };
    assert_eq!(
        csp("/api/v1/docs").await,
        csp("/nope").await,
        "v1 is off: /api/v1/docs is an SPA path and keeps the strict CSP"
    );
}
