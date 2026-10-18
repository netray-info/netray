//! API v1 domain reads (spec v2-api, requirements 10, 11, 12, 15): `/domains/{domain}`,
//! `/diff`, `/history`, driven through `netray_web::api::router`, paths relative to the mount.
use std::collections::BTreeSet;
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::{BlockId, EVIDENCE_SCHEMA_VERSION};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::api::{ApiConfig, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger};
use serde_json::{Value, json};
use tower::ServiceExt;

/// Mimics lens's `validate_domain`: trim, lowercase, strip one trailing dot, refuse `*` and empty.
struct StubRuns;

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        let d = domain.trim().to_lowercase();
        let d = d.strip_suffix('.').unwrap_or(&d).to_string();
        if d.is_empty() || d.contains('*') {
            return Err(DomainRefusal {
                code: "DOMAIN_INVALID",
                message: format!("invalid domain: {domain}"),
            });
        }
        Ok(d)
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

fn run(status: Status, grade: Grade) -> Run {
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
            status,
            findings: vec!["x".into()],
            evidence: vec![BlockId::parse("dns.caa").unwrap()],
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

async fn record(store: &HistoryStore, domain: &str, r: &Run, now: i64) -> String {
    match store
        .record(domain, r, &[], now, |p, c| diff(p, c, &[], None))
        .await
        .unwrap()
    {
        Recorded::First { id } | Recorded::Changed { id, .. } => id,
        other => panic!("expected a new entry, got {other:?}"),
    }
}

/// Three entries e1, e2, e3 for example.com, each a status change of the one check.
async fn three(store: &HistoryStore) {
    record(
        store,
        "example.com",
        &run(Status::Pass, Grade::A),
        1_800_000_000,
    )
    .await;
    record(
        store,
        "example.com",
        &run(Status::Fail, Grade::C),
        1_800_000_100,
    )
    .await;
    record(
        store,
        "example.com",
        &run(Status::Warn, Grade::B),
        1_800_000_200,
    )
    .await;
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
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn keys(v: &Value) -> BTreeSet<&str> {
    v.as_object().unwrap().keys().map(String::as_str).collect()
}

fn set(k: &[&'static str]) -> BTreeSet<&'static str> {
    k.iter().copied().collect()
}

#[tokio::test]
async fn domain_read_returns_the_latest_entry_view_with_the_stored_run() {
    let (store, _dir) = open().await;
    record(
        &store,
        "example.com",
        &run(Status::Pass, Grade::A),
        1_800_000_000,
    )
    .await;
    let entry = store.latest("example.com").await.unwrap().unwrap();

    let (status, body) = get(&app(Some(store.clone())), "/domains/Example.COM.").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        keys(&body),
        set(&[
            "id",
            "domain",
            "created_at",
            "last_seen_at",
            "predecessor_id",
            "profile_version",
            "grade",
            "engine_version",
            "run"
        ])
    );
    assert_eq!(body["id"], entry.id.as_str());
    assert_eq!(body["domain"], "example.com");
    assert_eq!(body["created_at"], 1_800_000_000);
    assert_eq!(body["last_seen_at"], 1_800_000_000);
    assert_eq!(body["predecessor_id"], Value::Null);
    assert_eq!(body["profile_version"], 3);
    assert_eq!(body["grade"], serde_json::to_value(Grade::A).unwrap());
    assert_eq!(body["engine_version"], "0.26.0");
    assert_eq!(body["run"], serde_json::to_value(&entry.run).unwrap());
    assert_eq!(body["run"]["checks"][0]["findings"], json!(["x"]));
    assert_eq!(body["run"]["checks"][0]["evidence"], json!(["dns.caa"]));
}

#[tokio::test]
async fn domain_read_errors_unknown_is_404_and_invalid_is_400() {
    let (store, _dir) = open().await;
    record(
        &store,
        "example.com",
        &run(Status::Pass, Grade::A),
        1_800_000_000,
    )
    .await;
    let app = app(Some(store));

    let (status, body) = get(&app, "/domains/example.org").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    let (status, body) = get(&app, "/domains/%2A.example.com").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "DOMAIN_INVALID");
}

#[tokio::test]
async fn diff_read_answers_the_stored_diff_of_the_latest_entry() {
    let (store, _dir) = open().await;
    let e1 = record(
        &store,
        "example.com",
        &run(Status::Pass, Grade::A),
        1_800_000_000,
    )
    .await;
    let e2 = record(
        &store,
        "example.com",
        &run(Status::Fail, Grade::C),
        1_800_000_100,
    )
    .await;
    let stored = store.get(&e2).await.unwrap().unwrap();
    assert!(stored.diff.is_some());

    let (status, body) = get(&app(Some(store)), "/domains/example.com/diff").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(keys(&body), set(&["entry_id", "predecessor_id", "diff"]));
    assert_eq!(body["entry_id"], e2.as_str());
    assert_eq!(body["predecessor_id"], e1.as_str());
    assert_eq!(
        body["diff"],
        serde_json::to_value(stored.diff.unwrap()).unwrap()
    );
}

#[tokio::test]
async fn diff_read_of_a_first_entry_is_null_and_of_no_entry_is_404() {
    let (store, _dir) = open().await;
    let e1 = record(
        &store,
        "example.com",
        &run(Status::Pass, Grade::A),
        1_800_000_000,
    )
    .await;
    let app = app(Some(store));

    let (status, body) = get(&app, "/domains/example.com/diff").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["entry_id"], e1.as_str());
    assert_eq!(body["predecessor_id"], Value::Null);
    assert_eq!(body["diff"], Value::Null);

    let (status, body) = get(&app, "/domains/example.org/diff").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

#[tokio::test]
async fn history_lists_entries_newest_first_with_five_keys() {
    let (store, _dir) = open().await;
    three(&store).await;
    let ids: Vec<String> = store
        .entries("example.com")
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.id)
        .collect();
    assert_eq!(ids.len(), 3);

    let app = app(Some(store));
    let (status, body) = get(&app, "/domains/example.com/history").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let items = body["entries"].as_array().expect("entries array");
    assert_eq!(items.len(), 3);
    let got: Vec<&str> = items.iter().map(|i| i["id"].as_str().unwrap()).collect();
    assert_eq!(got, ids.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(
        items
            .iter()
            .map(|i| i["created_at"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        vec![1_800_000_200, 1_800_000_100, 1_800_000_000]
    );
    for item in items {
        assert_eq!(
            keys(item),
            set(&[
                "id",
                "created_at",
                "last_seen_at",
                "grade",
                "profile_version"
            ])
        );
    }

    let (status, body) = get(&app, "/domains/example.org/history").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

#[tokio::test]
async fn without_a_store_every_domain_read_is_503_unavailable() {
    let app = app(None);
    for path in [
        "/domains/example.com",
        "/domains/example.com/diff",
        "/domains/example.com/history",
    ] {
        let (status, body) = get(&app, path).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{path}");
        assert_eq!(body["error"]["code"], "UNAVAILABLE", "{path}");
    }
}
