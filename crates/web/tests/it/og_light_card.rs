//! The social cards use the light tokens (pustina-findings C7, C16): the canvas is the light
//! background token `#f7f8fa` (tokens.json `color.light.bg`), not V1's dark `#0d1220`.
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use netray_engine::Registry;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::admission::ApexState;
use netray_web::config::ApexConfig;

use crate::common;

/// `color.light.bg` in crates/web/tokens.json.
const LIGHT_BG: [u8; 4] = [0xf7, 0xf8, 0xfa, 0xff];

fn run() -> Run {
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

fn top_left(png: &[u8]) -> [u8; 4] {
    let img = image::load_from_memory(png).expect("decodes").to_rgba8();
    img.get_pixel(0, 0).0
}

async fn get_png(app: axum::Router, uri: &str) -> Vec<u8> {
    let resp = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "{uri}");
    to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec()
}

async fn seeded_state(dir: &tempfile::TempDir) -> (ApexState, String) {
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let id = match store
        .record("example.com", &run(), &[], now - 10, |p, c| {
            netray_model::diff::diff(p, c, &[], None)
        })
        .await
        .unwrap()
    {
        Recorded::First { id } => id,
        other => panic!("expected First, got {other:?}"),
    };
    let mut config = ApexConfig {
        cache_enabled: false,
        ..ApexConfig::default()
    };
    config.limits.rate_limit_per_ip_per_minute = 60;
    config.limits.rate_limit_per_ip_burst = 10;
    config.limits.rate_limit_global_per_minute = 1000;
    config.limits.rate_limit_global_burst = 100;
    let mut state = ApexState::new(
        config,
        Registry::new(),
        Vec::new(),
        Duration::from_millis(2000),
        common::profile(),
    );
    state.history_store = Some(Arc::new(store));
    (state, id)
}

#[tokio::test]
async fn entry_card_top_left_pixel_is_the_light_background_token() {
    let dir = tempfile::tempdir().unwrap();
    let (state, id) = seeded_state(&dir).await;
    let app = netray_web::host::apex::app(state);
    let png = get_png(app, &format!("/r/{id}/og.png")).await;
    assert_eq!(top_left(&png), LIGHT_BG);
}

#[tokio::test]
async fn domain_card_top_left_pixel_is_the_light_background_token() {
    let dir = tempfile::tempdir().unwrap();
    let (state, _id) = seeded_state(&dir).await;
    let app = netray_web::pages::og::router(state);
    let png = get_png(app, "/og/example.com.png").await;
    assert_eq!(top_left(&png), LIGHT_BG);
}

#[test]
fn card_svg_fills_the_light_background_and_carries_no_dark_canvas() {
    use netray_web::og::render::svg_for_entry;
    let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let svg = svg_for_entry("a.example.com", "A", "", t, "<g/>");
    assert!(
        svg.contains("fill=\"#f7f8fa\""),
        "light bg token missing: {svg}"
    );
    assert!(!svg.contains("#0d1220"), "V1 dark canvas still present");
}

#[test]
fn card_svg_shows_the_protocol_grades_of_the_run_in_the_score_slot() {
    use netray_engine::domain_run::grade_label;
    use netray_engine::history::score_run;
    use netray_engine::scoring::grade_for;
    use netray_web::og::render::svg_for_entry;

    let profile = common::profile();
    let mut fixture = run();
    fixture.checks[0].id = CheckId::parse("dns.dnssec").unwrap();
    let score = score_run(&fixture, &profile);
    // The fixture run measures DNS only, so the slot reads one entry: "DNS <grade>".
    assert_eq!(score.sections.len(), 1);
    let dns = &score.sections[&Protocol::Dns];
    let expected = format!("DNS {}", grade_label(grade_for(&profile, dns.percentage)));

    let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let plate = netray_web::plate::card_plate(&netray_web::plate::card_layout(&fixture, None));
    let svg = svg_for_entry("a.example.com", "A", "", t, &plate);
    assert!(
        svg.contains(&format!(">{expected}<")),
        "no `{expected}` in: {svg}"
    );
}
