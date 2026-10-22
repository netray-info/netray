//! spec v2-correctness, Phase 5 (C8-C12, requirement 11): no domain run starts for a
//! single-label name; every entry that starts runs refuses it before admission. Real domain rule,
//! production config: a run that starts waits on the network, so each request is bounded.
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

const MESSAGE: &str = "a domain needs at least two labels";

async fn assert_refused(app: &Router, store: &HistoryStore, method: &str, uri: &str) -> Reply {
    let r = send(app, method, uri).await;
    assert_eq!(
        r.status,
        StatusCode::BAD_REQUEST,
        "{method} {uri}: {}",
        r.body
    );
    assert!(
        r.body.contains(MESSAGE),
        "{method} {uri}: {}",
        r.body.chars().take(200).collect::<String>()
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        store.latest("landing").await.unwrap().is_none(),
        "{method} {uri}: a run was recorded"
    );
    r
}

#[tokio::test]
async fn c8_posting_a_run_for_a_single_label_name_is_refused_with_two_labels_message() {
    let (app, store, _dir) = app().await;
    assert_refused(&app, &store, "POST", "/api/v1/domains/landing/runs").await;
}

#[tokio::test]
async fn c9_the_og_card_and_the_badge_refuse_a_single_label_name() {
    let (app, store, _dir) = app().await;
    assert_refused(&app, &store, "GET", "/og/landing.png").await;
    assert_refused(&app, &store, "GET", "/badge/landing.svg").await;
}

#[tokio::test]
async fn c10_the_result_page_for_a_single_label_name_starts_no_run() {
    let (app, store, _dir) = app().await;
    assert_refused(&app, &store, "GET", "/landing").await;
}

#[test]
fn c11_two_and_three_label_names_stay_valid_for_a_domain_run() {
    assert_eq!(
        lens::input::validate_domain("example.com").ok(),
        Some("example.com".to_string())
    );
    assert_eq!(
        lens::input::validate_domain("www.example.com").ok(),
        Some("www.example.com".to_string())
    );
}

#[test]
fn c12_the_dns_inspector_query_com_ns_still_parses() {
    assert!(netray_dns::parser::parse("com NS").is_ok());
}
