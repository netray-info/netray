//! `/r/<id>/og.png` for entries (spec v2-entries, requirement 15; lens half).
//!
//! Copied from crates/lens/tests/entries_og_card.rs (spec v2-cutover, Phase 6); dropped the two
//! snapshot cases (`v1_snapshot_id_card_is_gone_with_empty_body`,
//! `history_disabled_card_answers_todays_not_found_page`): both read a V1 snapshot id, and the
//! snapshot store, the V1 lookup on `/r/{id}` and the `/r/` snapshot routes are gone.
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use std::time::{Duration, SystemTime};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use netray_engine::Registry;
use netray_engine::domain_run::{CheckOutput, OverallScore};
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, RETENTION_SECS, Recorded};
use netray_web::admission::{ApexState, BadgeCheckFn};
use netray_web::config::ApexConfig;

use crate::common;

const ENTRY_DOMAIN: &str = "a.example.com";
const NOW: i64 = RETENTION_SECS + 5000;

fn base_config() -> ApexConfig {
    let mut config = ApexConfig {
        cache_enabled: false,
        cache_window_seconds: 300,
        ..ApexConfig::default()
    };
    config.limits.rate_limit_per_ip_per_minute = 60;
    config.limits.rate_limit_per_ip_burst = 10;
    config.limits.rate_limit_global_per_minute = 1000;
    config.limits.rate_limit_global_burst = 100;
    config
}

struct World {
    app: Router,
    history: Arc<HistoryStore>,
    _dir: tempfile::TempDir,
    counter: Arc<AtomicUsize>,
}

async fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let history = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    history.migrate().await.unwrap();
    let history = Arc::new(history);

    let counter = Arc::new(AtomicUsize::new(0));
    let mut state = ApexState::new(
        base_config(),
        Registry::new(),
        Vec::new(),
        Duration::from_millis(2000),
        common::profile(),
    );
    state.history_store = Some(Arc::clone(&history));
    state.badge_check_fn = Some(counting_check_fn(counter.clone()));
    World {
        app: netray_web::host::apex::app(state),
        history,
        _dir: dir,
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

fn header<'a>(a: &'a Answer, name: &str) -> &'a str {
    a.headers
        .get(name)
        .map(|v| v.to_str().unwrap())
        .unwrap_or("")
}

#[tokio::test]
async fn entry_card_is_a_1200x630_png_with_etag_and_cache_control() {
    let w = world().await;
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
    let w = world().await;
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

#[test]
fn svg_for_entry_is_the_v1_card_without_the_score_line() {
    use netray_web::og::layout::DEFAULT_LABEL;
    use netray_web::og::render::{format_utc_timestamp, svg_for_entry, svg_for_grade};

    let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let plate = "<g class=\"plate\"><line class=\"plate__line\" data-code=\"x\"/></g>";
    let entry = svg_for_entry("a.example.com", "A", DEFAULT_LABEL, t, plate);
    let v1 = svg_for_grade("a.example.com", "A", DEFAULT_LABEL, 95.0, t, Some(plate));

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
