//! API v1 entry reads (spec v2-api, requirement 13, C10 to C15): `/entries/{id}` and
//! `/entries/{id}/diff` through `netray_web::api::router`, paths relative to the mount.
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
use netray_store::{HistoryStore, RETENTION_SECS, Recorded};
use netray_web::api::{ApiConfig, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger};
use serde_json::{Value, json};
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
async fn c10_existing_entry_answers_entry_view_and_diff_view_equal_to_the_store() {
    let (store, _dir) = open().await;
    let e1 = record(&store, "a.example.com", Status::Pass, T).await;
    let e2 = record(&store, "a.example.com", Status::Fail, T + 60).await;
    let app = app(Some(store.clone()));

    let (status, body) = get(&app, &format!("/entries/{e2}")).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let mut keys: Vec<&str> = body
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "created_at",
            "domain",
            "engine_version",
            "grade",
            "id",
            "last_seen_at",
            "predecessor_id",
            "profile_version",
            "run"
        ]
    );
    assert_eq!(body["id"], e2.as_str());
    assert_eq!(body["domain"], "a.example.com");
    assert_eq!(body["created_at"], T + 60);
    assert_eq!(body["last_seen_at"], T + 60);
    assert_eq!(body["predecessor_id"], e1.as_str());
    assert_eq!(body["profile_version"], 3);
    assert_eq!(body["grade"], "A");
    assert_eq!(body["engine_version"], "0.26.0");
    let stored = store.get(&e2).await.unwrap().unwrap();
    assert_eq!(body["run"], serde_json::to_value(&stored.run).unwrap());

    let (status, body) = get(&app, &format!("/entries/{e2}/diff")).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["entry_id"], e2.as_str());
    assert_eq!(body["predecessor_id"], e1.as_str());
    let stored_diff = stored.diff.expect("e2 has a stored diff");
    assert_eq!(body["diff"], serde_json::to_value(&stored_diff).unwrap());
    assert!(!body["diff"].is_null());

    let (status, body) = get(&app, &format!("/entries/{e1}/diff")).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(
        body,
        json!({"entry_id": e1, "predecessor_id": null, "diff": null})
    );
}

#[tokio::test]
async fn c11_swept_and_never_issued_ids_answer_410_malformed_ids_404() {
    let (store, _dir) = open().await;
    let id = record(&store, DOMAIN, Status::Pass, T).await;
    assert_eq!(store.sweep(T + RETENTION_SECS + 86_400).await.unwrap(), 1);
    let app = app(Some(store));

    for path in [format!("/entries/{id}"), format!("/entries/{id}/diff")] {
        let (status, body) = get(&app, &path).await;
        assert_eq!(status, StatusCode::GONE, "{path}: {body}");
        assert_error(&body, "GONE");
        assert!(!body.to_string().contains(DOMAIN), "{path}: {body}");
    }
    // C12: a live V1 snapshot id that is no entry is, at netray-web level, just a well-formed
    // id absent from the store; lens passes no V1 lookup into the API.
    for path in ["/entries/ZZZZZZZZ", "/entries/ZZZZZZZZ/diff"] {
        let (status, body) = get(&app, path).await;
        assert_eq!(status, StatusCode::GONE, "{path}: {body}");
        assert_error(&body, "GONE");
    }
    for path in [
        "/entries/abc",
        "/entries/AAAA-AAA",
        "/entries/abc/diff",
        "/entries/AAAA-AAA/diff",
    ] {
        let (status, body) = get(&app, path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}: {body}");
        assert_error(&body, "NOT_FOUND");
    }
}

#[tokio::test]
async fn c13_failed_history_lookup_answers_500_internal_error_not_410() {
    let (store, dir) = open().await;
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(dir.path().join("history.db"));
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
    sqlx::query("DROP TABLE entries")
        .execute(&pool)
        .await
        .unwrap();
    let app = app(Some(store));

    for path in ["/entries/AAAAAAAA", "/entries/AAAAAAAA/diff"] {
        let (status, body) = get(&app, path).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{path}: {body}");
        assert_error(&body, "INTERNAL_ERROR");
    }
}

#[tokio::test]
async fn c14_no_store_answers_503_unavailable() {
    let app = app(None);
    for path in ["/entries/AAAAAAAA", "/entries/AAAAAAAA/diff"] {
        let (status, body) = get(&app, path).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{path}: {body}");
        assert_error(&body, "UNAVAILABLE");
    }
}

#[tokio::test]
async fn c15_entry_whose_predecessor_was_swept_keeps_its_diff() {
    let (store, _dir) = open().await;
    let e1 = record(&store, "a.example.com", Status::Pass, T).await;
    let e2 = record(&store, "a.example.com", Status::Fail, T + 10).await;
    let _e3 = record(&store, "a.example.com", Status::Pass, T + 200_000).await;
    // e1 is old and not the head's predecessor: swept; e2 stays as e3's predecessor.
    assert_eq!(store.sweep(T + RETENTION_SECS + 86_400).await.unwrap(), 1);
    let stored = store.get(&e2).await.unwrap().expect("e2 kept");
    assert_eq!(stored.predecessor_id, None, "e1 {e1} was swept");
    let stored_diff = stored.diff.expect("e2 keeps its diff");

    let (status, body) = get(&app(Some(store)), &format!("/entries/{e2}/diff")).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["entry_id"], e2.as_str());
    assert_eq!(body["predecessor_id"], Value::Null);
    assert_eq!(body["diff"], serde_json::to_value(&stored_diff).unwrap());
}
