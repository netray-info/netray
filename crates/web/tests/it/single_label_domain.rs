//! spec v2-correctness, Phase 5 (C8-C12, requirement 11): no domain run starts for a
//! single-label name; every entry that starts runs refuses it before admission. Real domain rule,
//! production config: a run that starts waits on the network, so each request is bounded.
//!
//! Copied from crates/lens/tests/it/single_label_domain.rs (spec v2-cutover, Phase 6); the app is
//! the apex with its sections on the goldens instead of lens's networked modules, and
//! `validate_domain` is `netray_web::error::validate_domain`; dropped the reply's unused
//! `location` and `json`.
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Extension;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use netray_store::HistoryStore;
use tower::ServiceExt;

use crate::common;

struct Reply {
    status: StatusCode,
    body: String,
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
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    Reply {
        status,
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
    // A segment without a dot is no domain to the page router: it goes to the fallback (the
    // site's other pages, `result_page_csp`'s routing contract), which starts no run itself.
    let (app, store, _dir) = app().await;
    let r = send(&app, "GET", "/landing").await;
    assert!(
        !r.body.contains("result-hero"),
        "GET /landing rendered a result page: {}",
        r.body.chars().take(200).collect::<String>()
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        store.latest("landing").await.unwrap().is_none(),
        "GET /landing: a run was recorded"
    );
}

#[test]
fn c11_two_and_three_label_names_stay_valid_for_a_domain_run() {
    assert_eq!(
        netray_web::error::validate_domain("example.com").ok(),
        Some("example.com".to_string())
    );
    assert_eq!(
        netray_web::error::validate_domain("www.example.com").ok(),
        Some("www.example.com".to_string())
    );
}

#[test]
fn c12_the_dns_inspector_query_com_ns_still_parses() {
    assert!(netray_dns::parser::parse("com NS").is_ok());
}
