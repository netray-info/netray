//! spec v2-input-edges, Phase 1 (C12, C13): with lens's real domain rule, full-width input is
//! normalised to ASCII before it is judged: a full-width address is refused, a full-width name
//! redirects to its ASCII form.
//!
//! Copied from crates/lens/tests/it/idna_domain_routes.rs (spec v2-cutover, Phase 6); the app is
//! the apex with its sections on the goldens instead of lens's networked modules; nothing dropped.
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Extension;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use netray_store::HistoryStore;
use serde_json::Value;
use tower::ServiceExt;

use crate::common;

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

/// The apex over the state of `tests/fixtures/lens.production.toml` with the cache off and a
/// history store, its sections on the committed goldens: a run that starts records at once.
async fn app() -> (Router, Arc<HistoryStore>, tempfile::TempDir) {
    let mut config = common::production_config();
    config.cache_enabled = false;
    let registry = common::registry(Some("spectra-inspect.json"), "beacon.sse");
    let plan = common::production_plan(&registry);
    let mut state = common::state_with(config, registry, plan);
    let (store, dir) = common::history_store().await;
    state.history_store = Some(store.clone());
    let app = netray_web::host::apex::app(state).layer(Extension(ConnectInfo(SocketAddr::from((
        [127, 0, 0, 1],
        40000,
    )))));
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
async fn c12_posting_a_run_for_a_full_width_address_is_domain_invalid_and_records_nothing() {
    let (app, store, _dir) = app().await;
    // U+FF11 FULLWIDTH DIGIT ONE, percent-encoded UTF-8.
    let r = send(
        &app,
        "POST",
        "/api/v1/domains/%EF%BC%91.%EF%BC%91.%EF%BC%91.%EF%BC%91/runs",
    )
    .await;
    assert_domain_invalid(&r, "POST runs");
    assert!(store.latest("1.1.1.1").await.unwrap().is_none());
}

#[tokio::test]
async fn c13_the_result_page_redirects_a_full_width_name_to_its_ascii_form() {
    let (app, _store, _dir) = app().await;
    // "ＥＸＡＭＰＬＥ.com" with U+FF25 U+FF38 U+FF21 U+FF2D U+FF30 U+FF2C U+FF25.
    let r = send(
        &app,
        "GET",
        "/%EF%BC%A5%EF%BC%B8%EF%BC%A1%EF%BC%AD%EF%BC%B0%EF%BC%AC%EF%BC%A5.com",
    )
    .await;
    assert_eq!(r.status, StatusCode::MOVED_PERMANENTLY, "{}", r.body);
    assert_eq!(r.location.as_deref(), Some("/example.com"));
}

#[tokio::test]
async fn a_recheck_post_of_a_unicode_name_names_its_normal_form() {
    let (app, _store, _dir) = app().await;
    // "bücher.de": the page's own form posts the A-label; a stale or hand-made POST does not.
    let r = send(&app, "POST", "/b%C3%BCcher.de/run").await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.body);
    assert!(r.body.contains("xn--bcher-kva.de"), "{}", r.body);
    assert!(!r.body.contains("lower case"), "{}", r.body);
}
