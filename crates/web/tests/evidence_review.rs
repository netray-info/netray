//! Review findings on the evidence routes (V2 spec v2-inspectors, requirement 6, interface I7).
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::entries::{EntriesDeps, EntryCard, NOT_FOUND_HTML, V1Lookup};
use netray_web::pages::{EvidenceDeps, evidence_router};
use tower::ServiceExt;

const T: i64 = 1_800_000_000;

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

async fn open() -> (Arc<HistoryStore>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    (Arc::new(store), dir)
}

async fn record(store: &HistoryStore, domain: &str) -> String {
    match store
        .record(domain, &run(), &[], T, |p, c| diff(p, c, &[], None))
        .await
        .unwrap()
    {
        Recorded::First { id } => id,
        other => panic!("expected First, got {other:?}"),
    }
}

fn app(store: Arc<HistoryStore>, v1: Option<V1Lookup>) -> Router {
    evidence_router(EvidenceDeps { history: store, v1 })
}

fn v1_stub(status: StatusCode) -> V1Lookup {
    Arc::new(move |_id: String| {
        Box::pin(async move {
            let resp: Response = (status, "v1").into_response();
            Some(resp)
        })
    })
}

async fn get(app: &Router, path: &str) -> (StatusCode, String) {
    let resp = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn invalid_utf8_path_segments_answer_the_404_page() {
    let (store, _dir) = open().await;
    let app = app(store, None);
    for path in ["/r/%C0/tls", "/r/AAAAAAAA/%C0", "/%C0/tls"] {
        let (status, body) = get(&app, path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert!(body.contains("<h1>Nothing here</h1>"), "{path}: {body}");
    }
}

#[tokio::test]
async fn failed_v1_snapshot_lookup_answers_500_and_a_found_one_404() {
    let (store, _dir) = open().await;
    let failing = app(
        store.clone(),
        Some(v1_stub(StatusCode::INTERNAL_SERVER_ERROR)),
    );
    let (status, _) = get(&failing, "/r/AAAAAAAA/tls").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "failed lookup");

    let found = app(store, Some(v1_stub(StatusCode::OK)));
    let (status, _) = get(&found, "/r/AAAAAAAA/tls").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "found snapshot");
}

#[tokio::test]
async fn domain_segment_follows_the_lens_validation_rule() {
    let (store, _dir) = open().await;
    record(&store, "localhost").await;
    let app = app(store, None);

    let (status, body) = get(&app, "/localhost/dns").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("data-protocol=\"dns\""), "{body}");

    let (status, body) = get(&app, "/192.0.2.1/dns").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!body.contains("No entry for"), "{body}");
}

#[tokio::test]
async fn no_entry_page_link_is_percent_encoded() {
    let (store, _dir) = open().await;
    let (status, body) = get(&app(store, None), "/%5Cevil.example.com/dns").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!body.contains("href=\"/\\"), "{body}");
    assert!(body.contains("href=\"/%5Cevil.example.com\""), "{body}");
}

#[tokio::test]
async fn trailing_slash_after_a_protocol_or_card_answers_the_entries_404() {
    let (store, _dir) = open().await;
    let id = record(&store, "a.example.com").await;
    let merged = netray_web::entries::router(EntriesDeps {
        history: store.clone(),
        v1: None,
        card: Arc::new(|_: &EntryCard| Ok(Vec::new())),
    })
    .merge(app(store, None));
    for path in [format!("/r/{id}/tls/"), format!("/r/{id}/og.png/")] {
        let (status, body) = get(&merged, &path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(body, NOT_FOUND_HTML, "{path}");
    }
}
