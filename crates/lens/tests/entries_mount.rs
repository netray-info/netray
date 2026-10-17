//! Mount rules for `/r/<id>` in `lens::app` (spec v2-entries, requirements 8, 10, 13, 14).
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use chrono::Utc;
use tower::ServiceExt;

use lens::check::CheckOutput;
use lens::config::{
    BackendsConfig, BadgesConfig, CacheConfig, Config, EcosystemConfig, OgCardsConfig,
    RateLimitConfig, ScoringConfig, ServerConfig, SiteConfig, SnapshotsConfig,
};
use lens::routes::snapshot_router;
use lens::score::OverallScore;
use lens::snapshot::{Snapshot, SnapshotFinding, SnapshotSection, SnapshotStore};
use lens::state::{AppState, BadgeCheckFn};
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, RETENTION_SECS, Recorded};

const NOT_FOUND_HTML: &str = r#"<!DOCTYPE html><html><head><title>Not found</title></head><body><h1>Snapshot not found</h1><p>This snapshot has expired or the ID is unknown.</p></body></html>"#;
const ENTRY_DOMAIN: &str = "entry.example.com";
const SNAP_DOMAIN: &str = "snap.example.com";
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

struct World {
    app: Router,
    history: Arc<HistoryStore>,
    snapshots: Option<Arc<SnapshotStore>>,
    dir: tempfile::TempDir,
    counter: Arc<AtomicUsize>,
}

async fn world(history_on: bool, snapshots_on: bool) -> World {
    let dir = tempfile::tempdir().unwrap();
    let mut config = base_config();
    config.history.enabled = history_on;
    config.history.db_path = dir.path().join("history.db");
    config.snapshots.enabled = snapshots_on;
    config.snapshots.db_path = dir.path().join("snapshots.db");

    let history = HistoryStore::open(&config.history.db_path).await.unwrap();
    history.migrate().await.unwrap();
    let history = Arc::new(history);
    let snapshots = SnapshotStore::new(&config.snapshots.db_path).await.unwrap();
    snapshots.migrate().await.unwrap();
    let snapshots = Arc::new(snapshots);

    let counter = Arc::new(AtomicUsize::new(0));
    let mut state = AppState::new(config).unwrap();
    state.history_store = Some(Arc::clone(&history));
    state.snapshot_store = Some(Arc::clone(&snapshots));
    state.badge_check_fn = Some(counting_check_fn(counter.clone()));
    World {
        app: lens::app(state),
        history,
        snapshots: Some(snapshots),
        dir,
        counter,
    }
}

fn counting_check_fn(counter: Arc<AtomicUsize>) -> BadgeCheckFn {
    Arc::new(move |_domain: String| {
        let counter = counter.clone();
        let fut: Pin<Box<dyn std::future::Future<Output = CheckOutput> + Send>> =
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                CheckOutput {
                    domain: "example.com".to_string(),
                    sections: HashMap::new(),
                    score: OverallScore {
                        sections: HashMap::new(),
                        overall_percentage: 80.0,
                        grade: "A".to_string(),
                        hard_fail_triggered: false,
                        hard_fail_checks: vec![],
                        not_applicable: HashMap::new(),
                        complete: true,
                    },
                    duration_ms: 1,
                }
            });
        fut
    })
}

fn a_run() -> Run {
    Run {
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
        }],
        blocks: vec![],
    }
}

async fn record(store: &HistoryStore, domain: &str, now: i64) -> String {
    match store
        .record(domain, &a_run(), &[], now, |p, c| diff(p, c, &[], None))
        .await
        .unwrap()
    {
        Recorded::First { id } => id,
        other => panic!("expected First, got {other:?}"),
    }
}

struct Answer {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: String,
}

async fn get(app: &Router, uri: &str) -> Answer {
    let resp = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    Answer {
        status,
        headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

fn cache_control(a: &Answer) -> &str {
    a.headers
        .get("cache-control")
        .map(|v| v.to_str().unwrap())
        .unwrap_or("")
}

#[tokio::test]
async fn history_enabled_mounts_entries_router() {
    let w = world(true, true).await;
    let id = record(&w.history, ENTRY_DOMAIN, NOW).await;

    let hit = get(&w.app, &format!("/r/{id}")).await;
    assert_eq!(hit.status, StatusCode::OK);
    assert!(hit.body.contains(ENTRY_DOMAIN));

    // A well-formed ID that is nowhere is gone (410) on the entries router, not 404.
    let gone = get(&w.app, "/r/AAAAAAAA").await;
    assert_eq!(gone.status, StatusCode::GONE);

    // The entries router's catch-all answers 404 with the shared body.
    let rest = get(&w.app, &format!("/r/{id}/whatever")).await;
    assert_eq!(rest.status, StatusCode::NOT_FOUND);
    assert_eq!(rest.body, NOT_FOUND_HTML);
}

#[tokio::test]
async fn history_disabled_snapshots_enabled_keeps_snapshot_router_answers() {
    let w = world(false, true).await;

    let unknown = get(&w.app, "/r/AAAAAAAA").await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    assert_eq!(unknown.body, NOT_FOUND_HTML);

    let og = get(&w.app, "/r/AAAAAAAA/og.png").await;
    assert_eq!(og.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn both_disabled_r_path_answers_as_the_spa() {
    let w = world(false, false).await;
    let spa = get(&w.app, "/some-client-route").await;
    let r = get(&w.app, "/r/AAAAAAAA").await;
    assert_eq!(r.status, spa.status);
    assert_eq!(r.body, spa.body);
    assert_ne!(r.body, NOT_FOUND_HTML);
}

#[tokio::test]
async fn history_on_snapshots_off_serves_entry() {
    let w = world(true, false).await;
    let id = record(&w.history, ENTRY_DOMAIN, NOW).await;
    let hit = get(&w.app, &format!("/r/{id}")).await;
    assert_eq!(hit.status, StatusCode::OK);
    assert!(hit.body.contains(ENTRY_DOMAIN));
}

#[tokio::test]
async fn live_v1_snapshot_is_served_when_history_is_on() {
    let w = world(true, true).await;
    let store = w.snapshots.clone().unwrap();
    let snap = make_snapshot(SNAP_DOMAIN, "B");
    let id = store.insert(snap).await.unwrap();

    let stored = store.get(&id).await.unwrap().unwrap();
    let expected = lens::snapshot::render_snapshot_html(&stored, &SiteConfig::default());

    let a = get(&w.app, &format!("/r/{id}")).await;
    assert_eq!(a.status, StatusCode::OK);
    assert_eq!(a.body, expected);
    assert_eq!(cache_control(&a), "public, max-age=86400, immutable");
}

#[tokio::test]
async fn entry_wins_over_v1_snapshot_with_same_id() {
    let w = world(true, true).await;
    let entry_id = record(&w.history, ENTRY_DOMAIN, NOW).await;
    let store = w.snapshots.clone().unwrap();
    let snap_id = store.insert(make_snapshot(SNAP_DOMAIN, "B")).await.unwrap();

    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}",
        w.dir.path().join("snapshots.db").display()
    ))
    .await
    .unwrap();
    sqlx::query("UPDATE snapshots SET shortid = ? WHERE shortid = ?")
        .bind(&entry_id)
        .bind(&snap_id)
        .execute(&pool)
        .await
        .unwrap();

    let a = get(&w.app, &format!("/r/{entry_id}")).await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.body.contains(ENTRY_DOMAIN));
    assert!(!a.body.contains(SNAP_DOMAIN));
}

#[tokio::test]
async fn r_requests_start_no_run_and_write_nothing() {
    let w = world(true, true).await;
    let entry_id = record(&w.history, ENTRY_DOMAIN, NOW).await;
    let swept_id = record(&w.history, "swept.example.com", 1000).await;
    assert_eq!(w.history.sweep(NOW + 10).await.unwrap(), 1);
    let snap_id = w
        .snapshots
        .clone()
        .unwrap()
        .insert(make_snapshot(SNAP_DOMAIN, "B"))
        .await
        .unwrap();

    let before = w
        .history
        .get(&entry_id)
        .await
        .unwrap()
        .unwrap()
        .last_seen_at;
    let entry = get(&w.app, &format!("/r/{entry_id}")).await;
    let gone = get(&w.app, &format!("/r/{swept_id}")).await;
    let snap = get(&w.app, &format!("/r/{snap_id}")).await;
    assert_eq!(entry.status, StatusCode::OK);
    assert_eq!(gone.status, StatusCode::GONE);
    assert_eq!(snap.status, StatusCode::OK);

    assert_eq!(w.counter.load(Ordering::SeqCst), 0);
    let after = w
        .history
        .get(&entry_id)
        .await
        .unwrap()
        .unwrap()
        .last_seen_at;
    assert_eq!(after, before);
}

#[tokio::test]
async fn snapshot_router_not_found_body_is_the_shared_constant() {
    let state = AppState::new(base_config()).unwrap();
    let app = snapshot_router().with_state(state);
    let a = get(&app, "/r/AAAAAAAA").await;
    assert_eq!(a.status, StatusCode::NOT_FOUND);
    assert_eq!(a.body, netray_web::entries::NOT_FOUND_HTML);
    assert_eq!(a.body, NOT_FOUND_HTML);
}
