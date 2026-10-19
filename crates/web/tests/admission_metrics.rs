//! Admission metrics of the check path (specs/features/lens-admission-metrics/spec.md, R1, R2;
//! criteria C6 to C11).
//!
//! The modules answer the committed goldens; the apex runs with the state of
//! `tests/fixtures/lens.production.toml` on them, the cache enabled, driven in-process. The
//! metrics are read from a per-test local recorder; a current-thread runtime keeps the handler on
//! the recording thread.
//!
//! Copied from crates/lens/tests/admission_metrics.rs (spec v2-cutover, Phase 6); dropped as V1:
//! `POST /api/check` and its V1 cache hit. Runs go through API v1's
//! `POST /api/v1/domains/{domain}/runs`; the repeat within the window (C7, C11) is the result
//! page's visit of a fresh head (`POST /{domain}/run`, a stored answer).

use std::cell::RefCell;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

mod common;

use axum::Extension;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use common::{
    dns_golden_raw, email_golden, facts_golden_raw, gated, http_module, ip_golden, registry_with,
    tls_golden,
};
use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use netray_store::HistoryStore;
use netray_web::admission::ApexState;
use tokio::sync::Notify;
use tower::ServiceExt;

const RUNS: &str = "netray_runs_total";
const IN_FLIGHT: &str = "netray_runs_in_flight";
const DURATION: &str = "netray_run_duration_seconds";

/// Production config, cache enabled; per-IP limit as given (burst equals limit).
fn app(per_ip: u32, dns_gate: Option<(Arc<Notify>, Arc<Notify>)>) -> Router {
    build(per_ip, dns_gate, None).0
}

/// Like `app` with a temp-file history store: a repeat visit is a stored answer only once the
/// run task has recorded the run.
async fn app_with_history(per_ip: u32) -> (Router, ApexState, tempfile::TempDir) {
    let (store, dir) = common::history_store().await;
    let (router, state) = build(per_ip, None, Some(store));
    (router, state, dir)
}

/// Poll (bounded, 5 s) until the domain has a head entry.
async fn wait_for_head(state: &ApexState, domain: &str) {
    let store = state.history_store.as_ref().expect("history store wired");
    for _ in 0..250 {
        if store.head(domain).await.unwrap().is_some() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the run was not recorded within 5 s");
}

fn build(
    per_ip: u32,
    dns_gate: Option<(Arc<Notify>, Arc<Notify>)>,
    store: Option<Arc<HistoryStore>>,
) -> (Router, ApexState) {
    let mut config = common::production_config();
    assert!(config.cache_enabled, "production config enables the cache");
    config.limits.rate_limit_per_ip_per_minute = per_ip;
    config.limits.rate_limit_per_ip_burst = per_ip;

    // The DNS module is the one held open when a gate is given.
    let dns = match dns_gate {
        Some((entered, release)) => gated(dns_golden_raw("prism.sse"), entered, release),
        None => dns_golden_raw("prism.sse"),
    };
    let registry = registry_with(
        dns,
        facts_golden_raw("prism.sse"),
        http_module(Some("spectra-inspect.json")),
        email_golden("beacon.sse"),
        ip_golden("ifconfig-json.json"),
        tls_golden("tlsight-inspect.json"),
    );
    let plan = common::production_plan(&registry);
    let mut state = common::state_with(config, registry, plan);
    state.history_store = store;
    let router = netray_web::host::apex::routes(state.clone()).layer(Extension(ConnectInfo(
        SocketAddr::from(([127, 0, 0, 1], 40000)),
    )));
    (router, state)
}

async fn send(app: &Router, req: Request<Body>) -> StatusCode {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    to_bytes(resp.into_body(), 16 * 1024 * 1024).await.unwrap();
    status
}

/// API v1's fresh run of `domain`.
async fn post_check(app: &Router, domain: &str) -> StatusCode {
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/domains/{domain}/runs"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

/// The result page's visit of `domain`: a fresh head answers stored, without a run.
async fn visit(app: &Router, domain: &str) -> StatusCode {
    let req = Request::builder()
        .method("POST")
        .uri(format!("/{domain}/run"))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::ACCEPT, "text/event-stream")
        .body(Body::from("trigger=visit"))
        .unwrap();
    send(app, req).await
}

/// Value of `netray_runs_total{trigger}` summed over `outcome`; absent series count 0.
fn runs(s: &Acc, trigger: &str) -> u64 {
    s.series()
        .into_iter()
        .filter(|t| t.name == RUNS && t.labels.iter().any(|(k, v)| k == "trigger" && v == trigger))
        .map(|t| t.counter)
        .sum()
}

fn runs_total(s: &Acc) -> u64 {
    s.series()
        .into_iter()
        .filter(|t| t.name == RUNS)
        .map(|t| t.counter)
        .sum()
}

/// The gauge's value; `None` when the series was never touched.
fn in_flight(s: &Acc) -> Option<f64> {
    s.series()
        .into_iter()
        .find(|t| t.name == IN_FLIGHT && t.is_gauge)
        .map(|t| t.gauge)
}

fn duration_observations(s: &Acc) -> usize {
    s.series()
        .into_iter()
        .filter(|t| t.name == DURATION)
        .map(|t| t.observations)
        .sum()
}

/// Accumulates snapshots: `Snapshotter::snapshot()` resets counters and gauges to 0 and
/// drains histograms, so every read adds its delta to a running total (for a gauge, the
/// running sum of increments and decrements is its value).
struct Acc {
    snap: Snapshotter,
    total: RefCell<HashMap<String, Total>>,
}

#[derive(Clone)]
struct Total {
    name: String,
    labels: Vec<(String, String)>,
    counter: u64,
    gauge: f64,
    observations: usize,
    is_gauge: bool,
}

impl Acc {
    fn new(snap: Snapshotter) -> Self {
        Self {
            snap,
            total: RefCell::new(HashMap::new()),
        }
    }

    /// Drain the recorder into the running totals and return all series.
    fn series(&self) -> Vec<Total> {
        let mut total = self.total.borrow_mut();
        for (k, _, _, v) in self.snap.snapshot().into_vec() {
            let labels: Vec<(String, String)> = k
                .key()
                .labels()
                .map(|l| (l.key().to_string(), l.value().to_string()))
                .collect();
            let id = format!("{}{:?}", k.key().name(), labels);
            let t = total.entry(id).or_insert_with(|| Total {
                name: k.key().name().to_string(),
                labels,
                counter: 0,
                gauge: 0.0,
                observations: 0,
                is_gauge: false,
            });
            match v {
                DebugValue::Counter(n) => t.counter += n,
                DebugValue::Gauge(g) => {
                    t.gauge += g.into_inner();
                    t.is_gauge = true;
                }
                DebugValue::Histogram(h) => t.observations += h.len(),
            }
        }
        total.values().cloned().collect()
    }
}

// Local recorders are thread-local: a current-thread runtime keeps the apex on this thread.

#[tokio::test(flavor = "current_thread")]
async fn admission_c6_fresh_run_counts_fresh_only() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let app = app(10, None);

    assert_eq!(post_check(&app, "example.com").await, StatusCode::OK);

    assert_eq!(runs(&snap, "api"), 1);
    assert_eq!(runs_total(&snap), 1, "no other trigger counts a run");
}

#[tokio::test(flavor = "current_thread")]
async fn admission_c7_repeat_within_ttl_counts_cache_hit() {
    // The history decides the hit, so the repeat waits for the recorded head.
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let (app, state, _dir) = app_with_history(10).await;

    post_check(&app, "example.com").await;
    wait_for_head(&state, "example.com").await;
    assert_eq!(visit(&app, "example.com").await, StatusCode::OK);

    // `cache_hit` has no successor: the hit starts no run, so only the first counts.
    assert_eq!(runs(&snap, "api"), 1);
    assert_eq!(runs_total(&snap), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn admission_c8_second_request_over_per_ip_limit_counts_rate_limited() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let app = app(1, None);

    assert_eq!(post_check(&app, "example.com").await, StatusCode::OK);
    assert_eq!(
        post_check(&app, "example.com").await,
        StatusCode::TOO_MANY_REQUESTS
    );

    // `rate_limited` has no successor: the refused request starts no run.
    assert_eq!(runs(&snap, "api"), 1);
    assert_eq!(runs_total(&snap), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn admission_c9_invalid_domain_increments_no_request_series() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let app = app(10, None);

    assert_eq!(
        post_check(&app, "*.example.com").await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(runs_total(&snap), 0, "invalid input starts no run");

    // Control: a valid request after it is the only one counted.
    post_check(&app, "example.com").await;
    assert_eq!(runs(&snap, "api"), 1);
    assert_eq!(runs_total(&snap), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn admission_c10_in_flight_gauge_is_1_during_run_and_0_after_return() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let app = app(10, Some((entered.clone(), release.clone())));

    let run = post_check(&app, "example.com");
    let observe = async {
        entered.notified().await;
        let during = in_flight(&snap);
        release.notify_one();
        during
    };
    let (status, during) = tokio::join!(run, observe);

    assert_eq!(status, StatusCode::OK);
    assert_eq!(during, Some(1.0), "one run is held open by the DNS module");
    assert_eq!(in_flight(&snap), Some(0.0), "the run returned");
}

#[tokio::test(flavor = "current_thread")]
async fn admission_c10_in_flight_gauge_stays_up_after_handler_future_is_dropped() {
    // v2-admission requirement 5: the run task holds the gauge, so a dropped requester leaves the
    // run in flight until it ends.
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let app = app(10, Some((entered.clone(), release.clone())));

    let outcome =
        tokio::time::timeout(Duration::from_millis(500), post_check(&app, "example.com")).await;

    assert!(outcome.is_err(), "the handler future is dropped mid-run");
    // The DNS module was reached, so the run was in flight when the future was dropped.
    tokio::time::timeout(Duration::from_secs(1), entered.notified())
        .await
        .expect("the run reached the DNS module");
    assert_eq!(
        in_flight(&snap),
        Some(1.0),
        "a run whose requester has gone is still in flight"
    );

    release.notify_one();
    let mut ended = false;
    for _ in 0..250 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        if in_flight(&snap) == Some(0.0) {
            ended = true;
            break;
        }
    }
    assert!(ended, "the run ended and left the gauge at 0");
}

#[tokio::test(flavor = "current_thread")]
async fn admission_c11_one_fresh_run_observes_one_duration() {
    // The repeat is a stored answer only after the head is recorded.
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let (app, state, _dir) = app_with_history(10).await;

    post_check(&app, "example.com").await;
    wait_for_head(&state, "example.com").await;
    visit(&app, "example.com").await; // stored answer: no run, no observation

    assert_eq!(duration_observations(&snap), 1);
}
