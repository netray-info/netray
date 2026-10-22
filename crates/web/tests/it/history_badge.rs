//! The badge reads `score.complete`, not only the grade string (v2-history requirement 14).
//!
//! Copied from crates/lens/tests/history_badge.rs (spec v2-cutover, Phase 6); nothing dropped.
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use netray_engine::Registry;
use netray_engine::domain_run::{CheckOutput, OverallScore};
use netray_store::HistoryStore;
use netray_web::admission::{ApexState, BadgeCheckFn};
use netray_web::config::ApexConfig;

use crate::common;

const SHORT_CACHE: &str = "public, max-age=300, s-maxage=300";
const LONG_CACHE: &str = "public, max-age=3600, s-maxage=3600, stale-while-revalidate=86400";

fn make_state(cache: bool) -> ApexState {
    let mut config = ApexConfig {
        cache_enabled: cache,
        cache_window_seconds: 7200,
        ..ApexConfig::default()
    };
    config.limits.rate_limit_per_ip_per_minute = 60;
    config.limits.rate_limit_per_ip_burst = 10;
    config.limits.rate_limit_global_per_minute = 1000;
    config.limits.rate_limit_global_burst = 100;
    ApexState::new(
        config,
        Registry::new(),
        Vec::new(),
        Duration::from_millis(2000),
        common::profile(),
    )
}

fn mock_check_fn(grade: &'static str, complete: bool) -> BadgeCheckFn {
    Arc::new(move |_domain: String| {
        let fut: Pin<Box<dyn std::future::Future<Output = CheckOutput> + Send>> =
            Box::pin(async move {
                CheckOutput {
                    domain: "example.com".to_string(),
                    sections: HashMap::new(),
                    score: OverallScore {
                        sections: HashMap::new(),
                        overall_percentage: 0.0,
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

/// Fetch the badge for example.com with a mocked result; returns (svg, cache-control).
async fn badge(grade: &'static str, complete: bool) -> (String, String) {
    badge_with(grade, complete, None).await
}

/// As [`badge`], optionally with a history store (and the cache on).
async fn badge_with(
    grade: &'static str,
    complete: bool,
    store: Option<Arc<HistoryStore>>,
) -> (String, String) {
    let (svg, cc, _) = badge_state(grade, complete, store).await;
    (svg, cc)
}

/// As [`badge_with`], also returning the state the badge ran on.
async fn badge_state(
    grade: &'static str,
    complete: bool,
    store: Option<Arc<HistoryStore>>,
) -> (String, String, ApexState) {
    let mut state = make_state(store.is_some());
    state.history_store = store;
    state.badge_check_fn = Some(mock_check_fn(grade, complete));
    let app = netray_web::pages::badge::router(state.clone());
    let req = Request::builder()
        .uri("/badge/example.com.svg")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cc = resp
        .headers()
        .get("cache-control")
        .expect("cache-control header must be present")
        .to_str()
        .unwrap()
        .to_string();
    let bytes = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    (String::from_utf8(bytes.to_vec()).unwrap(), cc, state)
}

#[tokio::test]
async fn incomplete_result_with_grade_f_renders_unscored_badge() {
    let (svg, cc) = badge("F", false).await;
    let (unscored_svg, _) = badge("incomplete", false).await;

    assert_eq!(
        svg, unscored_svg,
        "an incomplete F must render like the unscored badge"
    );
    assert!(svg.contains(">?</text>"), "badge must show ?, got: {svg}");
    assert!(
        !svg.contains(">F</text>"),
        "badge must not show F, got: {svg}"
    );
    assert_eq!(cc, SHORT_CACHE);
}

#[tokio::test]
async fn incomplete_result_with_grade_f_renders_unscored_badge_with_a_history_store_and_no_entry() {
    // Follows v2-admission Phase 3: the cache-on case is a history store with no entry.
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    let (svg, cc, state) = badge_state("F", false, Some(store.clone())).await;
    assert!(svg.contains(">?</text>"), "badge must show ?, got: {svg}");
    assert_eq!(cc, SHORT_CACHE);
    // The incomplete run is never recorded, so no head appears.
    state.runs.drain().await;
    assert_eq!(store.head("example.com").await.unwrap(), None);
}

#[tokio::test]
async fn complete_result_with_grade_f_renders_f_badge() {
    let (svg, cc) = badge("F", true).await;

    assert!(svg.contains(">F</text>"), "badge must show F, got: {svg}");
    assert!(
        !svg.contains(">?</text>"),
        "badge must not show ?, got: {svg}"
    );
    assert_eq!(cc, LONG_CACHE);
}
