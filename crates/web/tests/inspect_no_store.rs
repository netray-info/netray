//! V2 spec v2-inspectors, Phase 5, requirement 11 (C15, C3): a live inspector run is never
//! recorded: the history store is untouched and the run line says so; no grade is shown.
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::{Block, BlockId, EVIDENCE_SCHEMA_VERSION, Shape};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, Tool, inspect_router,
};
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

/// A job seam admitting everything and answering one text block per run.
struct Stub;

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: std::net::IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }

    fn start(&self, req: JobRequest) -> Result<Job, String> {
        let id = format!("{}.note", req.tool.as_str());
        let output: BoxFuture<'static, Result<Vec<Block>, String>> = Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(vec![Block {
                id: BlockId::parse(&id).unwrap(),
                title: "Note".into(),
                shape: Shape::Text("stub".into()),
            }])
        });
        Ok(Job::new(vec![JobPart {
            label: "part".into(),
            output,
        }]))
    }
}

/// POSTs one run and reads the whole stream; returns the status and the body.
async fn post(app: &axum::Router, tool: &str, body: String) -> (StatusCode, String) {
    let resp = app
        .clone()
        .oneshot(
            Request::post(format!("/inspect/{tool}/run"))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// One form body per tool: (tool, field, target).
const RUNS: [(&str, &str, &str); 4] = [
    ("dns", "q", "example.com"),
    ("tls", "h", "example.com"),
    ("http", "url", "https%3A%2F%2Fexample.com%2F"),
    ("ip", "ip", "192.0.2.1"),
];

#[tokio::test]
async fn c15_c3_live_runs_never_write_the_store_and_say_not_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let store = HistoryStore::open(&db).await.unwrap();
    store.migrate().await.unwrap();
    match store
        .record("example.com", &run(), &[], T, |p, c| diff(p, c, &[], None))
        .await
        .unwrap()
    {
        Recorded::First { .. } => {}
        other => panic!("expected First, got {other:?}"),
    }
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(&db);
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
    let before = snapshot(&pool).await;
    assert_eq!(before.0, 1);

    let app = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()));

    for (tool, field, target) in RUNS {
        let (status, body) = post(&app, tool, format!("{field}={target}")).await;
        assert_eq!(status, StatusCode::OK, "{tool}");
        assert!(
            body.contains("event: end"),
            "{tool} stream not read to end: {body}"
        );
        assert!(
            body.contains(r#"<span class=\"run-line__part\">live</span>"#),
            "{tool}: {body}"
        );
        assert!(
            body.contains(r#"<span class=\"run-line__part\">not recorded</span>"#),
            "{tool}: {body}"
        );
        assert!(!body.contains(r#"class=\"grade"#), "{tool}: {body}");
    }

    assert_eq!(snapshot(&pool).await, before);
}
