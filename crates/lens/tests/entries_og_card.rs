//! `/r/<id>/og.png` for entries (spec v2-entries, requirement 15; lens half).
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use std::time::{Duration, SystemTime};

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
use lens::score::OverallScore;
use lens::snapshot::{Snapshot, SnapshotFinding, SnapshotSection, SnapshotStore};
use lens::state::{AppState, BadgeCheckFn};
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, RETENTION_SECS, Recorded};

const ENTRY_DOMAIN: &str = "a.example.com";
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
            findings: vec![],
            evidence: vec![],
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
    body: Vec<u8>,
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
        body: bytes.to_vec(),
    }
}

fn cache_control(a: &Answer) -> &str {
    a.headers
        .get("cache-control")
        .map(|v| v.to_str().unwrap())
        .unwrap_or("")
}

fn header<'a>(a: &'a Answer, name: &str) -> &'a str {
    a.headers
        .get(name)
        .map(|v| v.to_str().unwrap())
        .unwrap_or("")
}

#[tokio::test]
async fn entry_card_is_a_1200x630_png_with_etag_and_cache_control() {
    let w = world(true, true).await;
    let id = record(&w.history, ENTRY_DOMAIN, NOW).await;

    let a = get(&w.app, &format!("/r/{id}/og.png")).await;
    assert_eq!(a.status, StatusCode::OK);
    assert_eq!(header(&a, "content-type"), "image/png");
    assert_eq!(header(&a, "cache-control"), "public, max-age=3600");
    assert!(!header(&a, "etag").is_empty());
    let img = image::load_from_memory(&a.body).expect("body decodes as an image");
    assert_eq!((img.width(), img.height()), (1200, 630));
    assert_eq!(w.counter.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn entry_card_matching_if_none_match_answers_304() {
    let w = world(true, true).await;
    let id = record(&w.history, ENTRY_DOMAIN, NOW).await;

    let first = get(&w.app, &format!("/r/{id}/og.png")).await;
    assert_eq!(first.status, StatusCode::OK);
    let etag = header(&first, "etag").to_string();
    assert!(!etag.is_empty());

    let resp = w
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/r/{id}/og.png"))
                .header("if-none-match", &etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
}

#[tokio::test]
async fn v1_snapshot_id_card_is_gone_with_empty_body() {
    let w = world(true, true).await;
    let id = w
        .snapshots
        .clone()
        .unwrap()
        .insert(make_snapshot(SNAP_DOMAIN, "B"))
        .await
        .unwrap();

    let a = get(&w.app, &format!("/r/{id}/og.png")).await;
    assert_eq!(a.status, StatusCode::GONE);
    assert!(a.body.is_empty());
}

#[tokio::test]
async fn history_disabled_card_answers_todays_not_found_page() {
    let w = world(false, true).await;
    let id = w
        .snapshots
        .clone()
        .unwrap()
        .insert(make_snapshot(SNAP_DOMAIN, "B"))
        .await
        .unwrap();

    let a = get(&w.app, &format!("/r/{id}/og.png")).await;
    assert_eq!(a.status, StatusCode::NOT_FOUND);
    assert_eq!(a.body, netray_web::entries::NOT_FOUND_HTML.as_bytes());
}

#[test]
fn svg_for_entry_is_the_v1_card_without_the_score_line() {
    use lens::og::layout::DEFAULT_LABEL;
    use lens::og::render::{format_utc_timestamp, svg_for_entry, svg_for_grade};

    let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let entry = svg_for_entry("a.example.com", "A", DEFAULT_LABEL, t);
    let v1 = svg_for_grade("a.example.com", "A", DEFAULT_LABEL, 95.0, t);

    assert!(entry.contains(">A<"));
    assert!(entry.contains("a.example.com"));
    assert!(entry.contains(&format_utc_timestamp(t)));
    assert!(!entry.contains('%'));

    let e: Vec<&str> = entry.lines().collect();
    let g: Vec<&str> = v1.lines().collect();
    assert_eq!(e.len(), g.len(), "same layout, same number of lines");
    let differing: Vec<usize> = (0..e.len()).filter(|&i| e[i] != g[i]).collect();
    assert_eq!(differing.len(), 1, "only the score line differs");
    assert!(g[differing[0]].contains("95.0%"));
}
