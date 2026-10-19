//! `/og/<domain>.png` draws the plate of the entry it shows over that entry's predecessor, and
//! the plain card for an admitted run (spec v2-result-page, Phase 7, requirement 29).
//!
//! Copied from crates/lens/tests/og_plate_history.rs (spec v2-cutover, Phase 6); nothing dropped.
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use netray_engine::Registry;
use netray_engine::catalogue::catalogue;
use netray_engine::domain_run::{CheckOutput, OverallScore};
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck};
use netray_model::{Grade, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::admission::{ApexState, BadgeCheckFn};
use netray_web::config::{ApexConfig, OgCardsConfig};
use netray_web::og::layout::DEFAULT_LABEL;
use netray_web::og::render::{svg_for_grade, svg_to_png};
use netray_web::plate::{card_layout, card_plate};

mod common;

const DOMAIN: &str = "shop.example.com";

/// lens's config of this test: the cache off, OG cards on; no sections.
fn new_state() -> ApexState {
    let mut config = ApexConfig {
        cache_enabled: false,
        cache_window_seconds: 300,
        og_cards: OgCardsConfig { enabled: true },
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

fn run_with_hsts(hsts: Status, profile_version: u32) -> Run {
    let checks = catalogue()
        .entries()
        .iter()
        .map(|e| RunCheck {
            id: e.id.clone(),
            status: if e.id.to_string() == "http.hsts" {
                hsts
            } else {
                Status::Pass
            },
            findings: vec![],
            evidence: vec![],
        })
        .collect();
    Run {
        profile_version,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections: vec![],
        checks,
        blocks: vec![],
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

async fn get_og(state: &ApexState) -> (StatusCode, Vec<u8>) {
    let resp = netray_web::pages::og::router(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/og/{DOMAIN}.png"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 8 * 1024 * 1024).await.unwrap();
    (status, bytes.to_vec())
}

async fn state_with_history(dir: &tempfile::TempDir) -> (ApexState, Arc<HistoryStore>) {
    let history = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    history.migrate().await.unwrap();
    let history = Arc::new(history);
    let mut state = new_state();
    state.history_store = Some(history.clone());
    (state, history)
}

async fn record(history: &HistoryStore, run: &Run, at: i64) {
    match history
        .record(DOMAIN, run, &[], at, |p, c| diff(p, c, &[], None))
        .await
        .unwrap()
    {
        Recorded::First { .. } | Recorded::Changed { .. } => {}
        other => panic!("expected a new entry, got {other:?}"),
    }
}

#[tokio::test]
async fn a_fresh_head_card_draws_the_plate_over_its_predecessor_with_the_fixed_line() {
    let dir = tempfile::tempdir().unwrap();
    let (state, history) = state_with_history(&dir).await;
    let v = state.scoring_profile.version;
    let e1 = run_with_hsts(Status::Fail, v);
    let e2 = run_with_hsts(Status::Pass, v);
    let seen = now_unix() - 10;
    record(&history, &e1, seen - 3600).await;
    record(&history, &e2, seen).await;

    let plate = card_plate(&card_layout(&e2, Some(&e1)));
    assert!(
        plate.contains("class=\"plate__line\" data-code=\"http.hsts\" data-tone=\"fixed\""),
        "the expected plate holds a fixed http.hsts"
    );
    let pct = netray_engine::history::score_run(&e2, &state.scoring_profile).overall_percentage;
    let svg = svg_for_grade(
        DOMAIN,
        "B",
        DEFAULT_LABEL,
        pct,
        UNIX_EPOCH + Duration::from_secs(seen as u64),
        Some(&plate),
    );
    let expected = svg_to_png(&svg, state.font_db.clone()).unwrap();

    let (status, body) = get_og(&state).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body == expected,
        "PNG differs from the head entry's plate over its predecessor"
    );
}

fn mock() -> BadgeCheckFn {
    Arc::new(move |_domain: String| {
        let fut: Pin<Box<dyn std::future::Future<Output = CheckOutput> + Send>> =
            Box::pin(async move {
                CheckOutput {
                    domain: DOMAIN.to_string(),
                    sections: HashMap::new(),
                    score: OverallScore {
                        sections: HashMap::new(),
                        overall_percentage: 10.0,
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

#[tokio::test]
async fn an_admitted_run_after_a_stale_head_answers_the_plain_card() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, history) = state_with_history(&dir).await;
    state.badge_check_fn = Some(mock());
    let v = state.scoring_profile.version;
    record(
        &history,
        &run_with_hsts(Status::Fail, v),
        now_unix() - 20_000,
    )
    .await;
    record(
        &history,
        &run_with_hsts(Status::Pass, v),
        now_unix() - 10_000,
    )
    .await;

    let before = now_unix();
    let (status, body) = get_og(&state).await;
    let after = now_unix();
    assert_eq!(status, StatusCode::OK);

    let plain = |at: i64| {
        let svg = svg_for_grade(
            DOMAIN,
            "A",
            DEFAULT_LABEL,
            10.0,
            UNIX_EPOCH + Duration::from_secs(at as u64),
            None,
        );
        assert_eq!(svg.matches("plate__").count(), 0);
        svg_to_png(&svg, state.font_db.clone()).unwrap()
    };
    assert!(
        body == plain(before) || body == plain(after),
        "an admitted run's card carries no plate"
    );
}
