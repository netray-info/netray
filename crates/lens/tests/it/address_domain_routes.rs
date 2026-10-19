//! spec v2-input-limits, Phase 1 (R3; C3, C7 to C10): with lens's real domain rule, a dotted
//! address is refused on every domain route with the route's existing invalid-domain answer; no
//! run starts and the result page does not redirect to the bare address first.
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode};
use lens::config::Config;
use lens::state::AppState;
use netray_store::HistoryStore;
use serde_json::Value;
use tower::ServiceExt;

struct Reply {
    status: StatusCode,
    location: Option<String>,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }
}

async fn app() -> (Router, Arc<HistoryStore>, tempfile::TempDir) {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    config.cache.enabled = false;
    let dir = tempfile::tempdir().unwrap();
    config.history.enabled = true;
    config.history.db_path = dir.path().join("history.db");
    let store = HistoryStore::open(&config.history.db_path).await.unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    let mut state = AppState::new(config).unwrap();
    state.history_store = Some(store.clone());
    let app = lens::app(state).layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
    (app, store, dir)
}

async fn send(app: &Router, method: &str, uri: &str) -> Reply {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("accept", "application/json")
        .body(Body::empty())
        .unwrap();
    // A refusal answers at once; a run that starts on the address would wait on the network.
    let resp = tokio::time::timeout(Duration::from_secs(5), app.clone().oneshot(req))
        .await
        .unwrap_or_else(|_| panic!("{method} {uri}: no refusal within 5 s, a run started"))
        .unwrap();
    let status = resp.status();
    let location = resp
        .headers()
        .get("location")
        .map(|v| v.to_str().unwrap().to_string());
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    Reply {
        status,
        location,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

fn assert_domain_invalid(r: &Reply, what: &str) {
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{what}: {}", r.body);
    assert_eq!(
        r.json()["error"]["code"],
        "DOMAIN_INVALID",
        "{what}: {}",
        r.body
    );
}

#[tokio::test]
async fn c7_posting_a_run_for_a_dotted_address_is_domain_invalid_and_records_nothing() {
    let (app, store, _dir) = app().await;
    let r = send(&app, "POST", "/api/v1/domains/192.0.2.1./runs").await;
    assert_domain_invalid(&r, "POST runs");
    assert!(store.latest("192.0.2.1").await.unwrap().is_none());
    assert!(store.latest("192.0.2.1.").await.unwrap().is_none());
}

#[tokio::test]
async fn c8_reading_a_dotted_address_is_domain_invalid() {
    let (app, _store, _dir) = app().await;
    let r = send(&app, "GET", "/api/v1/domains/192.0.2.1.").await;
    assert_domain_invalid(&r, "GET domain");
}

#[tokio::test]
async fn c9_the_result_page_refuses_a_dotted_address_without_a_redirect() {
    let (app, _store, _dir) = app().await;
    let r = send(&app, "GET", "/192.0.2.1.").await;
    assert_eq!(
        r.status,
        StatusCode::BAD_REQUEST,
        "{:?} {}",
        r.location,
        r.body
    );
    assert_eq!(r.location, None);
}

#[tokio::test]
async fn c10_the_badge_of_a_dotted_address_is_domain_invalid() {
    let (app, _store, _dir) = app().await;
    let r = send(&app, "GET", "/badge/192.0.2.1..svg").await;
    assert_domain_invalid(&r, "badge");
}
