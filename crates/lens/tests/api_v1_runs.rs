//! `POST /api/v1/domains/{domain}/runs` starts a fresh run (specs/features/v2-api/spec.md,
//! Phase 3, requirement 14; criteria C3 to C8).
//!
//! The whole app (`lens::app`) is driven in-process, so the route goes through lens's `LensRuns`.
//! The modules answer the committed goldens; lens runs with `tests/fixtures/lens.production.toml`
//! and a `HistoryStore` on a temp file. The clock is real (the sqlx pool needs it); the throttle
//! minute is passed with `pass_throttle`. Entries are polled with a bound because the run task
//! records after the response.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

mod common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
use common::{
    dns_golden_raw, email_golden, email_incomplete, facts_golden_raw, http_module, ip_golden,
    pass_throttle, registry_with, tls_golden,
};
use lens::config::Config;
use lens::state::AppState;
use netray_engine::Module;
use netray_store::{Entry, HistoryStore};
use serde_json::Value;
use tempfile::NamedTempFile;
use tower::ServiceExt;

const URI: &str = "/api/v1/domains/example.com/runs";

struct Harness {
    app: Router,
    store: Arc<HistoryStore>,
    db: NamedTempFile,
}

/// Production config, a migrated store on a temp file, golden modules; `email` and `tls`
/// replace those two modules.
async fn harness(email: Box<dyn Module>, tls: Box<dyn Module>) -> Harness {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    config.cache.enabled = false;
    config.rate_limit.per_ip_per_minute = 100;
    config.rate_limit.per_ip_burst = 100;
    let db = NamedTempFile::new().unwrap();
    config.history.enabled = true;
    config.history.db_path = db.path().to_path_buf();

    let store = HistoryStore::open(db.path()).await.unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);

    let registry = registry_with(
        dns_golden_raw("prism.sse"),
        facts_golden_raw("prism.sse"),
        http_module(Some("spectra-inspect.json")),
        email,
        ip_golden("ifconfig-json.json"),
        tls,
    );
    let mut state = AppState::with_registry(config, registry).unwrap();
    state.history_store = Some(store.clone());

    let app = lens::app(state).layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
    Harness { app, store, db }
}

async fn golden_harness() -> Harness {
    harness(
        email_golden("beacon.sse"),
        tls_golden("tlsight-inspect.json"),
    )
    .await
}

/// POST with the given Accept header (none: no header); returns status, content type, body.
async fn post_runs(app: &Router, accept: Option<&str>) -> (StatusCode, String, String) {
    let mut req = Request::builder().method("POST").uri(URI);
    if let Some(a) = accept {
        req = req.header(header::ACCEPT, a);
    }
    let resp = app
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    (
        status,
        content_type,
        String::from_utf8(bytes.to_vec()).unwrap(),
    )
}

async fn post_json(app: &Router) -> Value {
    let (status, _, body) = post_runs(app, Some("application/json")).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("not JSON ({e}): {body}"))
}

/// Polls the store for `example.com` entries until `want` hold (up to 5 s); returns the last read.
async fn entries_until(store: &HistoryStore, want: usize) -> Vec<Entry> {
    let mut last = Vec::new();
    for _ in 0..250 {
        last = store.entries("example.com").await.unwrap();
        if last.len() >= want {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    last
}

#[tokio::test(flavor = "current_thread")]
async fn api_v1_runs_c3_first_run_is_recorded_and_answered_as_json() {
    let h = golden_harness().await;

    let answer = post_json(&h.app).await;

    assert_eq!(answer["recorded"], "first", "answer: {answer}");
    let id = answer["entry_id"].as_str().expect("entry_id is a string");
    let entries = entries_until(&h.store, 1).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let entries_after = h.store.entries("example.com").await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries_after.len(), 1, "exactly one new entry");
    assert_eq!(entries_after[0].id, id);
    assert!(answer["run"].is_object());
}

#[tokio::test(flavor = "current_thread")]
async fn api_v1_runs_c4_no_accept_header_answers_json_not_sse() {
    let h = golden_harness().await;

    let (status, content_type, body) = post_runs(&h.app, None).await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(
        content_type.starts_with("application/json"),
        "content-type: {content_type}"
    );
    let answer: Value = serde_json::from_str(&body).expect("the body is one JSON document");
    assert_eq!(answer["recorded"], "first");
}

#[tokio::test(flavor = "current_thread")]
async fn api_v1_runs_c5_identical_rerun_answers_the_latest_entry() {
    let h = golden_harness().await;
    let first = post_json(&h.app).await;
    let first_id = first["entry_id"].as_str().unwrap().to_string();
    let head = entries_until(&h.store, 1).await;
    assert_eq!(head[0].id, first_id);
    pass_throttle().await;

    let second = post_json(&h.app).await;

    assert_eq!(second["recorded"], "identical", "answer: {second}");
    assert_eq!(second["entry_id"], first_id.as_str());
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(h.store.entries("example.com").await.unwrap().len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn api_v1_runs_c6_incomplete_run_with_hard_fail_is_not_stored() {
    // TLS is unreachable (a hard fail, grade F) and email never answers (incomplete).
    let h = harness(email_incomplete(), tls_golden("tlsight-unreachable.json")).await;

    let answer = post_json(&h.app).await;

    assert_eq!(answer["recorded"], "not_stored", "answer: {answer}");
    assert!(answer["entry_id"].is_null());
    assert_eq!(answer["run"]["complete"], false);
    assert_eq!(answer["run"]["grade"], "F");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(h.store.entries("example.com").await.unwrap().is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn api_v1_runs_c7_failing_store_write_is_not_stored_but_answered() {
    let h = golden_harness().await;
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(h.db.path());
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
    sqlx::query("DROP TABLE entries")
        .execute(&pool)
        .await
        .unwrap();

    let answer = post_json(&h.app).await;

    assert_eq!(answer["recorded"], "not_stored", "answer: {answer}");
    assert!(answer["entry_id"].is_null());
}

#[tokio::test(flavor = "current_thread")]
async fn api_v1_runs_c8_event_stream_has_a_section_event_per_section_then_done() {
    let h = golden_harness().await;

    let (status, content_type, body) = post_runs(&h.app, Some("text/event-stream")).await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(
        content_type.starts_with("text/event-stream"),
        "content-type: {content_type}"
    );
    let events: Vec<(String, Value)> = body
        .split("\n\n")
        .filter(|b| !b.trim().is_empty())
        .map(|b| {
            let name = b
                .lines()
                .find_map(|l| l.strip_prefix("event:"))
                .unwrap_or_else(|| panic!("event without a name: {b}"))
                .trim()
                .to_string();
            let data = b
                .lines()
                .filter_map(|l| l.strip_prefix("data:"))
                .map(str::trim_start)
                .collect::<Vec<_>>()
                .join("\n");
            (name, serde_json::from_str(&data).unwrap())
        })
        .collect();
    let (last_name, done) = events.last().expect("the stream carries events");
    assert_eq!(last_name, "done");
    let sections = events.iter().filter(|(n, _)| n == "section").count();
    assert_eq!(
        events.len(),
        sections + 1,
        "only section events, then one done"
    );
    let run_sections = done["run"]["sections"].as_array().unwrap().len();
    assert_eq!(sections, run_sections);
    assert!(sections > 0);

    let mut keys: Vec<&str> = done
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["diff", "entry_id", "recorded", "run"]);
    assert_eq!(done["recorded"], "first");
    // What a JSON answer carries for this run: the entry it recorded.
    let entries = entries_until(&h.store, 1).await;
    assert_eq!(done["entry_id"], entries[0].id.as_str());
    assert_eq!(done["run"], serde_json::to_value(&entries[0].run).unwrap());
}
