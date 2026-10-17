//! API v1 read errors (spec v2-api, requirements 6, 10-13): a failed store read answers a documented
//! `500` on the domain reads, and an entry id the path extractor refuses is malformed (`404`), as on `/r/<id>`.
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::api::openapi;
use netray_web::api::{ApiConfig, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger};
use serde_json::Value;
use tower::ServiceExt;

const T: i64 = 1_800_000_000;
const DOMAIN: &str = "gone.example.com";

struct StubRuns;

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_string())
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
    }
}

fn run(status: Status) -> Run {
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
            status,
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

/// Records `status` for `domain` at `now` and returns the entry id (First or Changed).
async fn record(store: &HistoryStore, domain: &str, status: Status, now: i64) -> String {
    match store
        .record(domain, &run(status), &[], now, |p, c| diff(p, c, &[], None))
        .await
        .unwrap()
    {
        Recorded::First { id } | Recorded::Changed { id, .. } => id,
        other => panic!("expected First or Changed, got {other:?}"),
    }
}

fn app(store: Option<Arc<HistoryStore>>) -> Router {
    netray_web::api::router(
        ApiState::new(ApiConfig::default(), Arc::new(StubRuns)).with_store(store),
    )
}

async fn get(app: &Router, path: &str) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    (
        status,
        serde_json::from_str(&text).unwrap_or(Value::String(text)),
    )
}

fn assert_error(body: &Value, code: &str) {
    assert_eq!(body["error"]["code"], code, "body: {body}");
}

#[tokio::test]
async fn failed_store_read_answers_a_documented_500_on_domain_reads() {
    let (store, dir) = open().await;
    record(&store, "a.example.com", Status::Pass, T).await;
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(dir.path().join("history.db"));
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
    sqlx::query("DROP TABLE entries")
        .execute(&pool)
        .await
        .unwrap();
    let app = app(Some(store));
    let doc = serde_json::to_value(openapi()).unwrap();
    for (path, key) in [
        ("/domains/a.example.com", "/api/v1/domains/{domain}"),
        (
            "/domains/a.example.com/diff",
            "/api/v1/domains/{domain}/diff",
        ),
        (
            "/domains/a.example.com/history",
            "/api/v1/domains/{domain}/history",
        ),
    ] {
        let (status, body) = get(&app, path).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{path}: {body}");
        assert_error(&body, "INTERNAL_ERROR");
        assert!(
            doc["paths"][key]["get"]["responses"].get("500").is_some(),
            "{key}: 500 is answered but not documented"
        );
    }
}

#[tokio::test]
async fn entry_id_refused_by_the_extractor_is_malformed_404() {
    let (store, _dir) = open().await;
    let app = app(Some(store));
    for path in ["/entries/%FF%FF%FF%FF", "/entries/%FF%FF%FF%FF/diff"] {
        let (status, body) = get(&app, path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}: {body}");
        assert_error(&body, "NOT_FOUND");
    }
}
