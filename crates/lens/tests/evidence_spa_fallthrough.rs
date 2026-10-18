//! `lens::app` with history enabled (spec v2-inspectors, Phase 4): the evidence router answers
//! the five protocol segments only; every other two-segment path still reaches `spa::handler`.
use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use lens::config::{
    BackendsConfig, BadgesConfig, CacheConfig, Config, EcosystemConfig, OgCardsConfig,
    RateLimitConfig, ScoringConfig, ServerConfig, SiteConfig, SnapshotsConfig,
};
use lens::state::AppState;
use netray_store::HistoryStore;

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
        inspect: Default::default(),
        api: Default::default(),
    }
}

async fn app() -> (Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut config = base_config();
    config.history.enabled = true;
    config.history.db_path = dir.path().join("history.db");
    let history = HistoryStore::open(&config.history.db_path).await.unwrap();
    history.migrate().await.unwrap();
    let mut state = AppState::new(config).unwrap();
    state.history_store = Some(Arc::new(history));
    (lens::app(state), dir)
}

async fn get(app: &Router, uri: &str) -> (StatusCode, String) {
    let resp = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn unknown_second_segment_reaches_the_spa_with_history_enabled() {
    let (app, _dir) = app().await;
    let (spa_status, spa_body) = get(&app, "/some/client/route").await;
    assert_eq!(spa_status, StatusCode::OK);
    assert!(spa_body.contains("<div id=\"root\""));

    let (status, body) = get(&app, "/example.com/smtp").await;
    assert_eq!(status, spa_status);
    assert_eq!(body, spa_body);
}

#[tokio::test]
async fn evidence_route_is_mounted_when_history_is_enabled() {
    let (app, _dir) = app().await;
    let (status, body) = get(&app, "/example.com/tls").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("href=\"/example.com\""), "got: {body}");
}
