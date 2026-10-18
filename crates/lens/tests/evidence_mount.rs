//! Mount rules for `/r/<id>/<protocol>` in `lens::app` (spec v2-inspectors, Phase 4, C8).
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
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, RETENTION_SECS, Recorded};

const ENTRY_DOMAIN: &str = "example.com";
const NOW: i64 = RETENTION_SECS + 5000;

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

fn a_run() -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("tls.chain_trusted").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

async fn get(app: &Router, uri: &str) -> (StatusCode, String, String) {
    let resp = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let ct = resp
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, ct, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn entry_card_and_evidence_view_are_answered_by_their_own_handlers() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = base_config();
    config.history.enabled = true;
    config.history.db_path = dir.path().join("history.db");
    config.snapshots.enabled = false;

    let history = HistoryStore::open(&config.history.db_path).await.unwrap();
    history.migrate().await.unwrap();
    let history = Arc::new(history);
    let id = match history
        .record(ENTRY_DOMAIN, &a_run(), &[], NOW, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap()
    {
        Recorded::First { id } => id,
        other => panic!("expected First, got {other:?}"),
    };

    let mut state = AppState::new(config).unwrap();
    state.history_store = Some(Arc::clone(&history));
    let app = lens::app(state);

    let (status, ct, body) = get(&app, &format!("/r/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(ct.starts_with("text/html"), "entry page content-type: {ct}");
    assert!(body.contains(ENTRY_DOMAIN));

    let (status, ct, _) = get(&app, &format!("/r/{id}/og.png")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ct, "image/png");

    let (status, ct, body) = get(&app, &format!("/r/{id}/tls")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        ct.starts_with("text/html"),
        "evidence view content-type: {ct}"
    );
    assert!(body.contains(r#"data-protocol="tls""#), "body: {body}");
    assert!(body.contains(&format!("entry {id}")), "body: {body}");

    let (status, _, _) = get(&app, &format!("/r/{id}/tls/x")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
