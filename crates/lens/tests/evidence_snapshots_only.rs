//! Snapshots-only lens (`[history] enabled = false`) answers `/r/<id>/tls` and `/<domain>/tls` as
//! before the evidence views (spec v2-inspectors, Phase 4, C9).
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use chrono::Utc;
use tower::ServiceExt;

use lens::config::{
    BackendsConfig, BadgesConfig, CacheConfig, Config, EcosystemConfig, OgCardsConfig,
    RateLimitConfig, ScoringConfig, ServerConfig, SiteConfig, SnapshotsConfig,
};
use lens::snapshot::{Snapshot, SnapshotFinding, SnapshotSection, SnapshotStore};
use lens::state::AppState;

const NOT_FOUND_HTML: &str = r#"<!DOCTYPE html><html><head><title>Not found</title></head><body><h1>Snapshot not found</h1><p>This snapshot has expired or the ID is unknown.</p></body></html>"#;

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

fn make_snapshot(domain: &str, grade: &str) -> Snapshot {
    Snapshot {
        shortid: String::new(),
        domain: domain.to_string(),
        grade: grade.to_string(),
        score: 95.0,
        sections: vec![SnapshotSection {
            name: "DNS".to_string(),
            grade: grade.to_string(),
            passes: 5,
            warns: 0,
            fails: 0,
            skips: 0,
            findings: vec![SnapshotFinding {
                check_name: "dnssec".to_string(),
                verdict: "pass".to_string(),
                message: "DNSSEC enabled".to_string(),
                earned: 5.0,
                possible: 5,
                fix_hint: None,
                fix_owner: None,
                guide_url: None,
            }],
        }],
        server_addresses: vec![],
        created_at: Utc::now(),
        lens_version: "0.9.1".to_string(),
    }
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
async fn c9_snapshots_only_lens_has_no_evidence_views() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = base_config();
    config.history.enabled = false;
    config.history.db_path = dir.path().join("history.db");
    config.snapshots.enabled = true;
    config.snapshots.db_path = dir.path().join("snapshots.db");

    let snapshots = SnapshotStore::new(&config.snapshots.db_path).await.unwrap();
    snapshots.migrate().await.unwrap();
    let snapshots = std::sync::Arc::new(snapshots);
    let id = snapshots
        .insert(make_snapshot("snap.example.com", "B"))
        .await
        .unwrap();

    let mut state = AppState::new(config).unwrap();
    state.snapshot_store = Some(snapshots);
    let app = lens::app(state);

    // The V1 snapshot id on an evidence path: snapshot_router has no such route.
    let (status, body) = get(&app, &format!("/r/{id}/tls")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, NOT_FOUND_HTML);

    // A domain evidence path is no evidence view: it reaches the SPA shell like any unknown path.
    let (spa_status, spa_body) = get(&app, "/some-client-route").await;
    let (status, body) = get(&app, "/example.com/tls").await;
    assert_eq!(status, spa_status);
    assert_eq!(body, spa_body);
    assert!(!body.contains("evidence-section"));
}
