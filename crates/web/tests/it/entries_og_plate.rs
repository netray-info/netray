//! The entry OG card carries the plate of the layout (V2 spec v2-result-page, Phase 7,
//! requirement 29): `GET /r/{id}/og.png` through `entries::router` with a recording card stub.
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_engine::catalogue::catalogue;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck};
use netray_model::{Grade, Status};
use netray_store::{HistoryStore, RETENTION_SECS, Recorded};
use netray_web::entries::{CardRenderer, EntriesDeps, EntryCard, router};
use netray_web::plate::{card_layout, card_plate};
use tower::ServiceExt;

const T: i64 = 1_800_000_000;

fn run_with(hsts: Status) -> Run {
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
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections: vec![],
        checks,
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

async fn record(store: &HistoryStore, hsts: Status, now: i64) -> String {
    match store
        .record("a.example.com", &run_with(hsts), &[], now, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap()
    {
        Recorded::First { id } | Recorded::Changed { id, .. } => id,
        other => panic!("expected a new entry, got {other:?}"),
    }
}

/// The plate of the card the stub received for `/r/<id>/og.png`.
async fn plate_for(store: Arc<HistoryStore>, id: &str) -> String {
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let s = seen.clone();
    let card: CardRenderer = Arc::new(move |card: &EntryCard| {
        s.lock().unwrap().push(card.plate.clone());
        Ok(b"\x89PNG-STUB".to_vec())
    });
    let app: Router = router(EntriesDeps {
        history: store,
        card,
    });
    let resp = app
        .oneshot(
            Request::get(format!("/r/{id}/og.png"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let _ = resp.into_body().collect().await.unwrap();
    let plates = seen.lock().unwrap();
    assert_eq!(plates.len(), 1);
    plates[0].clone()
}

#[tokio::test]
async fn c29_card_plate_is_the_plate_of_the_entry_and_its_predecessor() {
    let (store, _dir) = open().await;
    let first = record(&store, Status::Fail, T).await;
    let second = record(&store, Status::Pass, T + 60).await;
    let entry = store.get(&second).await.unwrap().unwrap();
    let pred = store.get(&first).await.unwrap().unwrap();
    assert_eq!(entry.predecessor_id.as_deref(), Some(first.as_str()));

    let plate = plate_for(store, &second).await;

    assert_eq!(plate, card_plate(&card_layout(&entry.run, Some(&pred.run))));
    let line = plate
        .match_indices("class=\"plate__line\"")
        .map(|(at, _)| &plate[plate[..at].rfind('<').unwrap()..at + plate[at..].find('>').unwrap()])
        .find(|t| t.contains("data-code=\"http.hsts\""))
        .expect("http.hsts line");
    assert!(line.contains("data-tone=\"fixed\""), "{line}");
}

#[tokio::test]
async fn c29_card_plate_of_a_swept_predecessor_has_no_fixed_line() {
    let (store, _dir) = open().await;
    let first = record(&store, Status::Fail, T).await;
    let second = record(&store, Status::Pass, T + RETENTION_SECS).await;
    // The sweep always keeps the head's predecessor, so a third entry becomes the head and
    // `second`, whose predecessor `first` is swept, is the entry the card shows.
    record(&store, Status::Fail, T + RETENTION_SECS + 1).await;
    assert_eq!(store.sweep(T + RETENTION_SECS + 86_400).await.unwrap(), 1);
    assert!(
        store.get(&first).await.unwrap().is_none(),
        "predecessor not swept"
    );
    let entry = store.get(&second).await.unwrap().unwrap();

    let plate = plate_for(store, &second).await;

    assert_eq!(plate, card_plate(&card_layout(&entry.run, None)));
    assert!(!plate.contains("data-tone=\"fixed\""));
}
