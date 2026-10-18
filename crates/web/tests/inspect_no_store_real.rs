//! V2 spec v2-inspectors, Phase 6, requirement 11 (C8): a REAL inspector job (the API parts
//! behind `InspectJobs`) run against a store leaves the entry count and every `last_seen_at`
//! unchanged, and its stream says "not recorded".
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_tls::testing::{StubTlsBackend, stub_state};
use netray_web::api::{
    ApiConfig, ApiIpConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, InspectJobs, RunStart,
    RunTrigger,
};
use netray_web::pages::{InspectConfig, InspectDeps, inspect_router};
use tower::ServiceExt;

const T: i64 = 1_800_000_000;

struct NoRuns;

impl DomainRuns for NoRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_string())
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: std::net::IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
    }
}

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

#[tokio::test]
async fn c8_real_jobs_never_write_the_store_and_say_not_recorded() {
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

    let config = ApiConfig {
        ip: ApiIpConfig {
            internal_mode: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let state = ApiState::new(config, Arc::new(NoRuns))
        .with_store(Some(Arc::new(store)))
        .with_parts(ApiParts {
            tls: Some(stub_state(Arc::new(StubTlsBackend::new())).inspector()),
            http: Some(Arc::new(netray_http::testing::StubHttpInspect::new())),
            ip: Some(Arc::new(netray_ip::testing::StubIpLookup::new())),
            ..Default::default()
        });
    let app = inspect_router(InspectDeps::new(
        Arc::new(InspectJobs::new(state)),
        InspectConfig::default(),
    ));

    for (tool, body) in [
        ("tls", "h=example.com"),
        ("http", "url=https%3A%2F%2Fexample.com%2F"),
        ("ip", "ip=192.0.2.1"),
    ] {
        let (status, stream) = post(&app, tool, body.to_string()).await;
        assert_eq!(status, StatusCode::OK, "{tool}");
        assert!(
            stream.contains("event: end"),
            "{tool} stream not read to end: {stream}"
        );
        assert!(
            stream.contains(r#"<span class=\"run-line__part\">live</span>"#),
            "{tool}: {stream}"
        );
        assert!(
            stream.contains(r#"<span class=\"run-line__part\">not recorded</span>"#),
            "{tool}: {stream}"
        );
        assert!(
            stream.contains(r#"id=\"ev-"#),
            "{tool}: the real job produced no evidence block: {stream}"
        );
    }

    assert_eq!(snapshot(&pool).await, before);
}
