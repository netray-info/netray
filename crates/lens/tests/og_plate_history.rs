//! `/og/<domain>.png` draws the plate of the entry it shows over that entry's predecessor, and
//! the plain card for an admitted run (spec v2-result-page, Phase 7, requirement 29).
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use lens::check::CheckOutput;
use lens::config::{
    BackendsConfig, BadgesConfig, CacheConfig, Config, EcosystemConfig, OgCardsConfig,
    RateLimitConfig, ScoringConfig, ServerConfig, SiteConfig, SnapshotsConfig,
};
use lens::og::layout::DEFAULT_LABEL;
use lens::og::render::{svg_for_grade, svg_to_png};
use lens::score::OverallScore;
use lens::state::{AppState, BadgeCheckFn};
use netray_engine::catalogue::catalogue;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck};
use netray_model::{Grade, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::plate::{card_layout, card_plate};

const DOMAIN: &str = "shop.example.com";

fn config(dir: &std::path::Path) -> Config {
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
        og_cards: OgCardsConfig { enabled: true },
        snapshots: SnapshotsConfig::default(),
        history: lens::config::HistoryConfig {
            enabled: true,
            db_path: dir.join("history.db"),
            ..Default::default()
        },
        admission: lens::config::AdmissionConfig::default(),
        inspect: Default::default(),
        api: Default::default(),
    }
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

async fn get_og(state: &AppState) -> (StatusCode, Vec<u8>) {
    let resp = lens::routes::og_router()
        .with_state(state.clone())
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

async fn state_with_history(dir: &tempfile::TempDir) -> (AppState, Arc<HistoryStore>) {
    let cfg = config(dir.path());
    let history = HistoryStore::open(&cfg.history.db_path).await.unwrap();
    history.migrate().await.unwrap();
    let history = Arc::new(history);
    let mut state = AppState::new(cfg).unwrap();
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
