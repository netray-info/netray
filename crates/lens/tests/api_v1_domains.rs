//! `/api/v1/domains/{domain}` in `lens::app` (spec v2-api, requirements 10, 15): lens's real
//! `validate_domain`, the history store from `[history]`, 503 with history disabled.
use std::net::SocketAddr;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

use lens::config::{
    BackendsConfig, BadgesConfig, CacheConfig, Config, EcosystemConfig, OgCardsConfig,
    RateLimitConfig, ScoringConfig, ServerConfig, SiteConfig, SnapshotsConfig,
};
use lens::state::AppState;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
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
    config.history.db_path = dir.path().join("h.db");
    let mut state = AppState::new(config.clone()).unwrap();
    if history_on {
        let store = HistoryStore::open(&config.history.db_path).await.unwrap();
        store.migrate().await.unwrap();
        let run = Run {
            profile_version: 3,
            engine_version: "0.26.0".into(),
            evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
            grade: Grade::A,
            complete: true,
            sections: vec![RunSection {
                protocol: Protocol::Dns,
                outcome: SectionKind::Measured,
                reason: None,
            }],
            checks: vec![RunCheck {
                id: CheckId::parse("dns.dnssec_valid").unwrap(),
                status: Status::Pass,
                findings: vec![],
                evidence: vec![],
            }],
            blocks: vec![],
        };
        store
            .record("example.com", &run, &[], 1_800_000_000, |p, c| {
                diff(p, c, &[], None)
            })
            .await
            .unwrap();
        state.history_store = Some(std::sync::Arc::new(store));
    }
    (lens::app(state), dir)
}

async fn get(app: &Router, path: &str) -> (StatusCode, Value) {
    let mut req = Request::get(path).body(Body::empty()).unwrap();
    let peer: SocketAddr = "203.0.113.9:4000".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn domain_read_normalises_with_lens_validation() {
    let (app, _dir) = app(true).await;
    let (status, body) = get(&app, "/api/v1/domains/Example.COM.").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["domain"], "example.com");
}

#[tokio::test]
async fn domain_read_refuses_a_wildcard_with_domain_invalid() {
    let (app, _dir) = app(true).await;
    let (status, body) = get(&app, "/api/v1/domains/%2A.example.com").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "DOMAIN_INVALID");
}

#[tokio::test]
async fn domain_read_with_history_disabled_is_503_unavailable() {
    let (app, _dir) = app(false).await;
    let (status, body) = get(&app, "/api/v1/domains/example.com").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["error"]["code"], "UNAVAILABLE");
}
