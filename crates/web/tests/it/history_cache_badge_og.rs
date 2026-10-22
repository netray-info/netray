//! Badge and OG cards answer from the history head (spec v2-admission, Phase 3,
//! requirements 10 and 11): a fresh head is a hit, a stale one goes through the
//! limiter and a run, and an incomplete run never becomes a cached grade.
//!
//! Copied from crates/lens/tests/history_cache_badge_og.rs (spec v2-cutover, Phase 6); nothing
//! dropped.
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use netray_engine::Registry;
use netray_engine::domain_run::{CheckOutput, OverallScore};
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::admission::{ApexState, BadgeCheckFn};
use netray_web::config::ApexConfig;

use crate::common;

const SHORT_CACHE: &str = "public, max-age=300, s-maxage=300";
const LONG_CACHE: &str = "public, max-age=3600, s-maxage=3600, stale-while-revalidate=86400";
const DOMAIN: &str = "example.com";

fn make_state() -> ApexState {
    let mut config = ApexConfig {
        cache_enabled: true,
        cache_window_seconds: 300,
        ..ApexConfig::default()
    };
    config.limits.rate_limit_per_ip_per_minute = 60;
    config.limits.rate_limit_per_ip_burst = 10;
    config.limits.rate_limit_global_per_minute = 1000;
    config.limits.rate_limit_global_burst = 100;
    config.og_cards.enabled = true;
    ApexState::new(
        config,
        Registry::new(),
        Vec::new(),
        Duration::from_millis(2000),
        common::profile(),
    )
}

fn mock(counter: Arc<AtomicUsize>, grade: &'static str, complete: bool) -> BadgeCheckFn {
    Arc::new(move |_domain: String| {
        let counter = counter.clone();
        let fut: Pin<Box<dyn std::future::Future<Output = CheckOutput> + Send>> =
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                CheckOutput {
                    domain: DOMAIN.to_string(),
                    sections: HashMap::new(),
                    score: OverallScore {
                        sections: HashMap::new(),
                        overall_percentage: 10.0,
                        grade: grade.to_string(),
                        hard_fail_triggered: grade == "F",
                        hard_fail_checks: vec![],
                        not_applicable: HashMap::new(),
                        complete,
                    },
                    duration_ms: 1,
                }
            });
        fut
    })
}

fn stored_run(grade: Grade, profile_version: u32) -> Run {
    Run {
        profile_version,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![
            RunCheck {
                id: CheckId::parse("dns.dnssec_valid").unwrap(),
                status: Status::Pass,
                findings: vec![],
                evidence: vec![],
            },
            RunCheck {
                id: CheckId::parse("tls.caa_compliant").unwrap(),
                status: Status::Pass,
                findings: vec![],
                evidence: vec![],
            },
        ],
        blocks: vec![],
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// A state with a history store; when `seed` is `Some((grade, age_secs))` the head is
/// seeded at `now - age`. Returns the state, the mock's call counter, the temp dir and
/// the seeded run.
async fn setup(
    seed: Option<(Grade, i64)>,
    mock_grade: &'static str,
    mock_complete: bool,
) -> (
    ApexState,
    Arc<AtomicUsize>,
    tempfile::TempDir,
    Option<(Run, i64)>,
) {
    let mut state = make_state();
    let counter = Arc::new(AtomicUsize::new(0));
    state.badge_check_fn = Some(mock(counter.clone(), mock_grade, mock_complete));
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let mut seeded = None;
    if let Some((grade, age)) = seed {
        let run = stored_run(grade, state.scoring_profile.version);
        let at = now_unix() - age;
        store
            .record(DOMAIN, &run, &[], at, |p, c| {
                netray_model::diff::diff(p, c, &[], None)
            })
            .await
            .unwrap();
        seeded = Some((run, at));
    }
    state.history_store = Some(Arc::new(store));
    (state, counter, dir, seeded)
}

fn badge_app(state: ApexState) -> Router {
    netray_web::pages::badge::router(state)
}

async fn get_badge(app: &Router) -> (String, String) {
    let req = Request::builder()
        .uri("/badge/example.com.svg")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cc = resp
        .headers()
        .get("cache-control")
        .expect("cache-control header must be present")
        .to_str()
        .unwrap()
        .to_string();
    let bytes = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    (String::from_utf8(bytes.to_vec()).unwrap(), cc)
}

#[tokio::test]
async fn c1_c5_c17_fresh_head_serves_the_badge_without_a_run() {
    let (state, calls, _dir, _) = setup(Some((Grade::B, 10)), "A", true).await;
    let (svg, cc) = get_badge(&badge_app(state)).await;
    assert!(svg.contains(">B<"), "badge must show B, got: {svg}");
    assert_eq!(cc, LONG_CACHE);
    assert_eq!(calls.load(Ordering::SeqCst), 0, "no run for a fresh head");
}

#[tokio::test]
async fn c18_stale_head_with_exhausted_limiter_serves_the_head_short() {
    let (state, calls, _dir, _) = setup(Some((Grade::B, 7200)), "A", true).await;
    let _ = state.badge_recompute_limiter.check_key(&DOMAIN.to_string());
    let (svg, cc) = get_badge(&badge_app(state)).await;
    assert!(svg.contains(">B<"), "badge must show B, got: {svg}");
    assert_eq!(cc, SHORT_CACHE);
    assert_eq!(calls.load(Ordering::SeqCst), 0, "limiter answers, no run");
}

#[tokio::test]
async fn c19_incomplete_run_with_a_head_serves_the_head_short() {
    let (state, calls, _dir, _) = setup(Some((Grade::B, 7200)), "F", false).await;
    let (svg, cc) = get_badge(&badge_app(state)).await;
    assert!(svg.contains(">B<"), "badge must show B, got: {svg}");
    assert!(!svg.contains(">F<"), "badge must not show F, got: {svg}");
    assert_eq!(cc, SHORT_CACHE);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn c20_incomplete_run_without_a_head_is_unscored_short() {
    let (state, calls, _dir, _) = setup(None, "F", false).await;
    let (svg, cc) = get_badge(&badge_app(state)).await;
    assert!(svg.contains(">?</text>"), "badge must show ?, got: {svg}");
    assert_eq!(cc, SHORT_CACHE);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn c21_complete_run_without_a_head_serves_its_grade_long() {
    let (state, calls, _dir, _) = setup(None, "F", true).await;
    let (svg, cc) = get_badge(&badge_app(state)).await;
    assert!(svg.contains(">F</text>"), "badge must show F, got: {svg}");
    assert_eq!(cc, LONG_CACHE);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn c4_c15_an_incomplete_run_is_never_a_hit_for_the_next_badge_request() {
    let (state, _calls, _dir, _) = setup(None, "F", false).await;
    let app = badge_app(state.clone());
    let (first, _) = get_badge(&app).await;
    assert!(
        first.contains(">?</text>"),
        "first must show ?, got: {first}"
    );
    // Let the run task finish recording, if it records at all.
    state.runs.drain().await;
    let (second, cc) = get_badge(&app).await;
    assert!(
        second.contains(">?</text>"),
        "second must still show ?, got: {second}"
    );
    assert!(!second.contains(">F</text>"), "never a cached F: {second}");
    assert_eq!(cc, SHORT_CACHE, "never long-cached");
}

/// Compared by PNG bytes, rendered the way the handler renders a hit.
#[tokio::test]
async fn c6_c22_fresh_head_serves_the_og_card_without_a_run() {
    let (state, calls, _dir, seeded) = setup(Some((Grade::B, 10)), "A", true).await;
    let (run, last_seen_at) = seeded.unwrap();
    let pct = netray_engine::history::score_run(&run, &state.scoring_profile).overall_percentage;
    let plate = netray_web::plate::card_plate(&netray_web::plate::card_layout(&run, None));
    let svg = netray_web::og::render::svg_for_grade(
        DOMAIN,
        "B",
        netray_web::og::layout::DEFAULT_LABEL,
        pct,
        UNIX_EPOCH + Duration::from_secs(last_seen_at as u64),
        Some(&plate),
    );
    let expected = netray_web::og::render::svg_to_png(&svg, state.font_db.clone()).unwrap();

    let app = netray_web::pages::og::router(state);
    let req = Request::builder()
        .uri("/og/example.com.png")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get("cache-control")
            .unwrap()
            .to_str()
            .unwrap(),
        LONG_CACHE
    );
    let bytes = to_bytes(resp.into_body(), 8 * 1024 * 1024).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0, "no run for a fresh head");
    assert!(
        bytes.as_ref() == expected.as_slice(),
        "PNG differs from the head's card"
    );
}
