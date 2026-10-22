//! Incomplete results (spec grade-integrity, Phase 2, requirements 3 and 4): the badge and OG
//! card halves.
//!
//! The DNS module runs the committed DNS golden; the TLS, HTTP and IP sections come from golden
//! modules of the engine registry, the email section from one that is incomplete; the apex runs
//! with lens's `tests/fixtures/lens.production.toml` (the cache enabled) and a history store on
//! a temp file. The routes are driven in-process. A result with an Errored section is
//! `incomplete`: never cached, never badged or rendered as a letter.
//!
//! Copied from crates/lens/tests/incomplete_results.rs (spec v2-cutover, Phase 6), the badge and
//! OG cases (C7, C8) only; the other cases are ported apart. Dropped as V1: the follow-up
//! `POST /api/check` and its `X-Cache: MISS`; the follow-up is API v1's
//! `POST /api/v1/domains/{domain}/runs`, which asserts the incomplete run and the empty history.

use std::net::SocketAddr;
use std::sync::Arc;

use crate::common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode};
use common::{
    dns_golden_raw, email_incomplete, facts_golden_raw, http_module, ip_golden, registry_with,
    tls_module,
};
use netray_store::HistoryStore;
use serde_json::Value;
use tower::ServiceExt;

struct Harness {
    app: Router,
    store: Arc<HistoryStore>,
    _history_dir: tempfile::TempDir,
}

impl Harness {
    /// An incomplete run left no history entry (API v1 answers after the record).
    async fn assert_history_stays_empty(&self) {
        assert!(
            self.store.entries("example.com").await.unwrap().is_empty(),
            "an incomplete run is never recorded"
        );
    }
}

/// Production config (cache enabled), the email section incomplete, a history store.
async fn harness() -> Harness {
    let config = common::production_config();
    assert!(config.cache_enabled, "production config enables the cache");

    let registry = registry_with(
        dns_golden_raw("prism.sse"),
        facts_golden_raw("prism.sse"),
        http_module(Some("spectra-inspect.json")),
        email_incomplete(),
        ip_golden("ifconfig-json.json"),
        tls_module(Some("tlsight-inspect.json")),
    );
    let plan = common::production_plan(&registry);
    let mut state = common::state_with(config, registry, plan);
    let (history, history_dir) = common::history_store().await;
    state.history_store = Some(history.clone());
    assert!(state.badge_check_fn.is_none(), "use the real check");

    let app = netray_web::host::apex::routes(state)
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
    Harness {
        app,
        store: history,
        _history_dir: history_dir,
    }
}

/// A fresh API v1 run of example.com; its answer.
async fn post_run(app: &Router) -> Value {
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/domains/example.com/runs")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).expect("JSON answer")
}

async fn get_path(app: &Router, uri: &str) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let (status, headers) = (resp.status(), resp.headers().clone());
    let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    (status, headers, bytes.to_vec())
}

const SHORT_CACHE: &str = "max-age=300";

fn assert_incomplete(answer: &Value) {
    assert_eq!(answer["run"]["complete"], false, "answer: {answer}");
}

// ---------------------------------------------------------------------------

#[tokio::test]
async fn incomplete_c7_badge_first_shows_question_mark_and_leaves_check_uncached() {
    let h = harness().await;

    let (status, headers, body) = get_path(&h.app, "/badge/example.com.svg").await;
    assert_eq!(status, StatusCode::OK);
    let cc = headers["cache-control"].to_str().unwrap();
    assert!(cc.contains(SHORT_CACHE), "short Cache-Control, got: {cc}");
    let svg = String::from_utf8(body).unwrap();
    assert!(svg.contains(">?<"), "badge must show ?, got: {svg}");

    // v2-admission R12/R16: the next run of the domain starts only after the throttle.
    common::pass_throttle().await;
    let answer = post_run(&h.app).await;
    assert_incomplete(&answer);
    // Follows v2-admission Phase 3: no history entry exists for the incomplete run.
    h.assert_history_stays_empty().await;
}

#[tokio::test]
async fn incomplete_c8_og_first_shows_unknown_card_and_leaves_check_uncached() {
    let h = harness().await;

    let (status, headers, body) = get_path(&h.app, "/og/example.com.png").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.starts_with(b"\x89PNG"), "a PNG card is served");
    let cc = headers["cache-control"].to_str().unwrap();
    assert!(cc.contains(SHORT_CACHE), "short Cache-Control, got: {cc}");

    // v2-admission R12/R16: the next run of the domain starts only after the throttle.
    common::pass_throttle().await;
    let answer = post_run(&h.app).await;
    assert_incomplete(&answer);
    // Follows v2-admission Phase 3: no history entry exists for the incomplete run.
    h.assert_history_stays_empty().await;
}
