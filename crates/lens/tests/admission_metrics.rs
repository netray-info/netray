//! Admission metrics of the check path (specs/features/lens-admission-metrics/spec.md, R1, R2;
//! criteria C6 to C11).
//!
//! The modules answer the committed goldens; lens runs with `tests/fixtures/lens.production.toml`
//! on them, the cache enabled, driven in-process. The metrics are read from a per-test local
//! recorder; a current-thread runtime keeps the handler on the recording thread.

use std::cell::RefCell;
use std::collections::HashMap;
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
    dns_golden_raw, email_golden, facts_golden_raw, gated, http_module, ip_golden, registry_with,
    tls_golden,
};
use lens::config::Config;
use lens::routes::api_router;
use lens::state::AppState;
use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use tokio::sync::Notify;
use tower::ServiceExt;

const REQUESTS: &str = "lens_check_requests_total";
const IN_FLIGHT: &str = "lens_runs_in_flight";
const DURATION: &str = "lens_run_duration_seconds";

/// Production config, cache enabled; per-IP limit as given (burst equals limit).
fn app(per_ip: u32, dns_gate: Option<(Arc<Notify>, Arc<Notify>)>) -> Router {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    assert!(config.cache.enabled, "production config enables the cache");
    config.rate_limit.per_ip_per_minute = per_ip;
    config.rate_limit.per_ip_burst = per_ip;

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
    let state = AppState::with_registry(config, registry).unwrap();
    let (api, _) = api_router().split_for_parts();
    Router::new()
        .merge(api.with_state(state))
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))))
}

async fn post_check(app: &Router, domain: &str) -> StatusCode {
    let req = Request::builder()
        .method("POST")
        .uri("/api/check")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(r#"{{"domain":"{domain}"}}"#)))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    status
}

/// Value of `lens_check_requests_total{result}`; absent series count 0.
fn requests(s: &Acc, result: &str) -> u64 {
    s.series()
        .into_iter()
        .filter(|t| {
            t.name == REQUESTS && t.labels.iter().any(|(k, v)| k == "result" && v == result)
        })
        .map(|t| t.counter)
        .sum()
}

fn requests_total(s: &Acc) -> u64 {
    s.series()
        .into_iter()
        .filter(|t| t.name == REQUESTS)
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

// Local recorders are thread-local: a current-thread runtime keeps lens on this thread.

#[tokio::test(flavor = "current_thread")]
async fn admission_c6_fresh_run_counts_fresh_only() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let app = app(10, None);

    assert_eq!(post_check(&app, "example.com").await, StatusCode::OK);

    assert_eq!(requests(&snap, "fresh"), 1);
    assert_eq!(requests(&snap, "cache_hit"), 0);
    assert_eq!(requests(&snap, "rate_limited"), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn admission_c7_repeat_within_ttl_counts_cache_hit() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let app = app(10, None);

    post_check(&app, "example.com").await;
    post_check(&app, "example.com").await;

    assert_eq!(requests(&snap, "fresh"), 1);
    assert_eq!(requests(&snap, "cache_hit"), 1);
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

    assert_eq!(requests(&snap, "rate_limited"), 1);
    assert_eq!(requests(&snap, "fresh"), 1);
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
    assert_eq!(requests_total(&snap), 0, "invalid input is not counted");

    // Control: a valid request after it is the only one counted.
    post_check(&app, "example.com").await;
    assert_eq!(requests(&snap, "fresh"), 1);
    assert_eq!(requests_total(&snap), 1);
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
async fn admission_c10_in_flight_gauge_is_0_after_handler_future_is_dropped() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let entered = Arc::new(Notify::new());
    // Never released: the DNS module does not answer before the timeout fires.
    let release = Arc::new(Notify::new());
    let app = app(10, Some((entered.clone(), release)));

    let outcome =
        tokio::time::timeout(Duration::from_millis(500), post_check(&app, "example.com")).await;

    assert!(outcome.is_err(), "the handler future is dropped mid-run");
    // The DNS module was reached, so the run was in flight when the future was dropped.
    tokio::time::timeout(Duration::from_secs(1), entered.notified())
        .await
        .expect("the run reached the DNS module");
    assert_eq!(
        in_flight(&snap),
        Some(0.0),
        "a dropped run is no longer in flight"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn admission_c11_one_fresh_run_observes_one_duration() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let app = app(10, None);

    post_check(&app, "example.com").await;
    post_check(&app, "example.com").await; // cache hit: no run, no observation

    assert_eq!(duration_observations(&snap), 1);
}
