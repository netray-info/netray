//! Evidence routes in `lens::app` leave the SPA's hashed assets alone (spec v2-inspectors, Phase 4, C7).
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
        api: Default::default(),
    }
}

async fn app(history_on: bool) -> (Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut config = base_config();
    config.history.enabled = history_on;
    config.history.db_path = dir.path().join("history.db");
    let history = HistoryStore::open(&config.history.db_path).await.unwrap();
    history.migrate().await.unwrap();
    let mut state = AppState::new(config).unwrap();
    if history_on {
        state.history_store = Some(Arc::new(history));
    }
    (lens::app(state), dir)
}

async fn get(app: &Router, uri: &str) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let resp = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, headers, bytes.to_vec())
}

fn hashed_js_asset() -> String {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/frontend/dist/assets");
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .find(|n| n.starts_with("index-") && n.ends_with(".js"))
        .expect("a hashed index-<hash>.js under frontend/dist/assets")
}

#[tokio::test]
async fn c7_hashed_asset_unchanged_with_evidence_router_mounted() {
    let uri = format!("/assets/{}", hashed_js_asset());
    let (on, _d1) = app(true).await;
    let (off, _d2) = app(false).await;

    let a = get(&on, &uri).await;
    let b = get(&off, &uri).await;
    for (status, headers, body) in [&a, &b] {
        assert_eq!(*status, StatusCode::OK);
        let ct = headers.get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("javascript"), "content-type was {ct}");
        assert_eq!(
            headers.get("cache-control").unwrap().to_str().unwrap(),
            "public, max-age=31536000, immutable"
        );
        assert!(!body.is_empty());
    }
    assert_eq!(a.1.get("content-type"), b.1.get("content-type"));
    assert_eq!(a.1.get("cache-control"), b.1.get("cache-control"));
    assert_eq!(a.2, b.2);

    // Evidence route is mounted with history on: empty store answers 404 with the explore link.
    let (status, _h, body) = get(&on, "/example.com/tls").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        String::from_utf8_lossy(&body).contains(r#"href="/example.com""#),
        "404 body lacks the domain link"
    );
}
