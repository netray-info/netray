//! The evidence routes (V2 spec v2-inspectors, Phase 4, C10): answering one never writes to the
//! history store: the entry count and every `last_seen_at` stay as they were.
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
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
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("tls.chain_trusted").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
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

async fn snapshot(pool: &sqlx::SqlitePool) -> (i64, Vec<(String, i64)>) {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entries")
        .fetch_one(pool)
        .await
        .unwrap();
    let rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT id, last_seen_at FROM entries ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap();
    (count, rows)
}

async fn status_of(app: &Router, path: &str) -> StatusCode {
    app.clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn c10_evidence_routes_never_write_to_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let store = HistoryStore::open(&db).await.unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    let id = record(&store, "example.com").await;
    record(&store, "other.example.com").await;

    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(&db);
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
    let before = snapshot(&pool).await;
    assert_eq!(before.0, 2);

    let app = evidence_router(EvidenceDeps { history: store });

    // The first request must be the 200, so the test cannot pass before the feature exists.
    assert_eq!(status_of(&app, "/example.com/tls").await, StatusCode::OK);

    let mut paths: Vec<String> = ["dns", "tls", "http", "email", "ip"]
        .iter()
        .flat_map(|p| [format!("/example.com/{p}"), format!("/r/{id}/{p}")])
        .collect();
    paths.extend(
        ["/r/{id}/smtp", "/r/AAAAAAAA/tls", "/other.example.com/tls"]
            .map(|p| p.replace("{id}", &id)),
    );
    for path in &paths {
        status_of(&app, path).await;
    }

    assert_eq!(snapshot(&pool).await, before);
}
