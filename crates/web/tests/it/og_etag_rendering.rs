//! A card ETag changes with the card's rendering (spec pustina-findings, reader finding 3): a
//! client holding the old dark card's ETag revalidates to 200, never to 304.
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::response::Response;
use netray_engine::Registry;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::admission::ApexState;
use netray_web::config::ApexConfig;
use netray_web::entries::{CardRenderer, EntriesDeps, router};
use tower::ServiceExt;
use xxhash_rust::xxh3::xxh3_64;

use crate::common;

const T: i64 = 1_800_000_000;

fn run(grade: Grade) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade,
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

async fn open() -> (Arc<HistoryStore>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    (Arc::new(store), dir)
}

async fn get(app: axum::Router, uri: &str, inm: &str) -> Response {
    app.oneshot(
        Request::get(uri)
            .header(header::IF_NONE_MATCH, inm)
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn entry_card_old_format_etag_revalidates_to_200_with_a_new_etag() {
    let (store, _dir) = open().await;
    let id = match store
        .record("a.example.com", &run(Grade::A), &[], T, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap()
    {
        Recorded::First { id } => id,
        other => panic!("expected First, got {other:?}"),
    };
    let card: CardRenderer = Arc::new(|_| Ok(b"\x89PNG-STUB".to_vec()));
    let app = router(EntriesDeps {
        history: store,
        card,
    });
    let old = format!("\"{id}-A\"");

    let resp = get(app, &format!("/r/{id}/og.png"), &old).await;

    assert_eq!(resp.status(), StatusCode::OK, "old ETag {old} must not 304");
    let new = resp.headers().get(header::ETAG).unwrap().to_str().unwrap();
    assert_ne!(new, old);
}

#[tokio::test]
async fn domain_card_old_format_etag_revalidates_to_200_with_a_new_etag() {
    let mut config = ApexConfig {
        cache_enabled: true,
        cache_window_seconds: 300,
        ..ApexConfig::default()
    };
    config.og_cards.enabled = true;
    let mut state = ApexState::new(
        config,
        Registry::new(),
        Vec::new(),
        Duration::from_millis(2000),
        common::profile(),
    );
    let (store, _dir) = open().await;
    let stored = run(Grade::B);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    store
        .record("example.com", &stored, &[], now - 10, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap();
    state.history_store = Some(store);
    let pct = netray_engine::history::score_run(&stored, &state.scoring_profile).overall_percentage;
    let plate = netray_web::plate::card_plate(&netray_web::plate::card_layout(&stored, None));
    // The ETag format before the light card: hash of domain, grade, label, score and plate.
    let hash = xxh3_64(format!("example.com\x00B\x00\x00{pct:.1}\x00{plate}").as_bytes());
    let old = format!("\"{hash:016x}\"");

    let app = netray_web::pages::og::router(state);
    let resp = get(app, "/og/example.com.png", &old).await;

    assert_eq!(resp.status(), StatusCode::OK, "old ETag {old} must not 304");
    let new = resp.headers().get(header::ETAG).unwrap().to_str().unwrap();
    assert_ne!(new, old);
}
