//! The evidence entry route (V2 spec v2-inspectors, Phase 4, C5): `GET /r/<id>/<protocol>`.
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::entries::V1Lookup;
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
            protocol: Protocol::Email,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("email.mx").unwrap(),
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
async fn c5_entry_evidence_route_answers_200_410_404_and_500() {
    let (store, _dir) = open().await;
    let id = record(&store, "a.example.com").await;
    let v1_none: V1Lookup = Arc::new(|_| Box::pin(async { None }));

    let (status, body) = get(&app(store.clone(), None), &format!("/r/{id}/email")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("data-protocol=\"email\""), "{body}");
    assert!(body.contains(&format!("entry {id}")), "{body}");

    let (status, _) = get(&app(store.clone(), None), "/r/AAAAAAAA/email").await;
    assert_eq!(status, StatusCode::GONE, "no v1 lookup");

    let (status, _) = get(&app(store.clone(), Some(v1_none)), "/r/AAAAAAAA/email").await;
    assert_eq!(status, StatusCode::GONE, "v1 lookup answering None");

    let (status, _) = get(&app(store, None), "/r/abc/tls").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "malformed id");

    let (store, dir) = open().await;
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(dir.path().join("history.db"));
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
    sqlx::query("DROP TABLE entries")
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = get(&app(store, None), "/r/AAAAAAAA/email").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "failing store");
}
