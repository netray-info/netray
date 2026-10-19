//! spec v2-result-page, Phase 3, requirement 19 (criterion C15): `POST /{domain}/run` goes
//! through lens's `LensRuns` and so through v2-admission's throttle and limits.
//!
//! Lens runs with `tests/fixtures/lens.production.toml` on the committed goldens and a history
//! store, driven in-process through `lens::app`. The throttle test pauses tokio's clock after the
//! store is open; the client of a request is 127.0.0.1 (`MockConnectInfo`).

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

mod common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
use common::{
    dns_golden_raw, email_golden, facts_golden_raw, http_module, ip_golden, registry_with,
    tls_golden,
};
use lens::admission::{Admit, Trigger, admit, check_limits};
use lens::config::Config;
use lens::state::AppState;
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol};
use netray_store::HistoryStore;
use tower::ServiceExt;

fn client() -> IpAddr {
    IpAddr::from([127, 0, 0, 1])
}

/// Counts the runs of the module it wraps.
struct Counting {
    inner: Box<dyn Module>,
    runs: Arc<AtomicUsize>,
}

impl Module for Counting {
    fn protocol(&self) -> Protocol {
        self.inner.protocol()
    }
    fn checks(&self) -> &'static [CheckId] {
        self.inner.checks()
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        self.inner.volatile()
    }
    fn run<'a>(&'a self, ctx: &'a RunContext, facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        self.inner.run(ctx, facts)
    }
}

struct Harness {
    app: Router,
    state: AppState,
    runs: Arc<AtomicUsize>,
    _dir: tempfile::TempDir,
}

async fn harness(tune: impl FnOnce(&mut Config)) -> Harness {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    config.rate_limit.per_ip_per_minute = 1000;
    config.rate_limit.per_ip_burst = 1000;
    config.rate_limit.global_per_minute = 10000;
    config.rate_limit.global_burst = 10000;
    tune(&mut config);
    let dir = tempfile::tempdir().unwrap();
    config.history.enabled = true;
    config.history.db_path = dir.path().join("history.db");
    config.snapshots.enabled = false;
    let history = HistoryStore::open(&config.history.db_path).await.unwrap();
    history.migrate().await.unwrap();

    let runs = Arc::new(AtomicUsize::new(0));
    let dns = Box::new(Counting {
        inner: dns_golden_raw("prism.sse"),
        runs: runs.clone(),
    });
    let registry = registry_with(
        dns,
        facts_golden_raw("prism.sse"),
        http_module(Some("spectra-inspect.json")),
        email_golden("beacon.sse"),
        ip_golden("ifconfig-json.json"),
        tls_golden("tlsight-inspect.json"),
    );
    let mut state = AppState::with_registry(config, registry).unwrap();
    state.history_store = Some(Arc::new(history));
    let app =
        lens::app(state.clone()).layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
    Harness {
        app,
        state,
        runs,
        _dir: dir,
    }
}

/// `POST /example.com/run` as the page's stream does; status, `Retry-After` and the text of the
/// body with tags stripped.
async fn recheck(app: &Router) -> (StatusCode, Option<String>, String) {
    let req = Request::builder()
        .method("POST")
        .uri("/example.com/run")
        .header(header::ACCEPT, "text/event-stream")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("trigger=recheck"))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let retry = resp
        .headers()
        .get(header::RETRY_AFTER)
        .map(|v| v.to_str().unwrap().to_string());
    let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    let body = String::from_utf8_lossy(&bytes).into_owned();
    (status, retry, strip_tags(&body))
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// A run 20 s ago: the route refuses `throttle` with the run line naming 20 s and the wait.
#[tokio::test(flavor = "current_thread")]
async fn result_run_c15_a_run_20s_ago_is_refused_throttle_with_the_run_line() {
    let h = harness(|_| {}).await;
    // The clock pauses only after the run is recorded: a paused clock auto-advances through the
    // store's sqlx acquires (CLAUDE.md), which moved the first run hours past its throttle.
    let started = tokio::time::Instant::now();

    let mut stream = match admit(&h.state, "example.com", client(), Trigger::Api, false) {
        Admit::Admitted(s) => s,
        _ => panic!("expected a run for example.com"),
    };
    while stream.next().await.is_some() {}
    h.state.runs.drain().await;
    assert_eq!(h.runs.load(Ordering::SeqCst), 1);
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(20)).await;

    let (status, retry, text) = recheck(&h.app).await;

    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{text}");
    // The throttle window depends on whether the prior run was complete; the wait is the window
    // minus the 20 s that passed, and the run line names the same wait as Retry-After.
    let retry = retry.expect("Retry-After");
    // "N s ago" counts from the prior run's start: its own duration plus the 20 s waited after it.
    let ago = started.elapsed().as_secs();
    assert!(ago >= 20);
    assert!(text.contains(&format!("{ago} s ago")), "run line: {text}");
    assert!(
        text.contains(&format!("a new run is possible in {retry} s")),
        "run line: {text}"
    );
    assert_eq!(h.runs.load(Ordering::SeqCst), 1, "no run started");
}

/// A spent per-IP limit refuses before `admit`: the run line names the address, no run starts.
#[tokio::test(flavor = "current_thread")]
async fn result_run_c15_a_spent_ip_limit_is_refused_before_admit() {
    let h = harness(|c| {
        c.rate_limit.per_ip_per_minute = 600;
        c.rate_limit.per_ip_burst = 1;
    })
    .await;
    check_limits(&h.state, client()).expect("the first call passes the limiter");

    let (status, retry, text) = recheck(&h.app).await;

    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{text}");
    assert!(retry.is_some(), "429 carries Retry-After");
    assert!(
        text.contains("too many checks from your address; try again in "),
        "run line: {text}"
    );
    assert_eq!(h.runs.load(Ordering::SeqCst), 0, "no run started");
}
