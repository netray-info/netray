//! Admission metrics of the check path (specs/features/lens-admission-metrics/spec.md, R1, R2;
//! criteria C6 to C11).
//!
//! Stub backends serve the committed goldens; lens runs with `tests/fixtures/lens.production.toml`
//! on them, the cache enabled, driven in-process. The metrics are read from a per-test local
//! recorder; a current-thread runtime keeps the handler on the recording thread.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
use axum::routing::{get, post};
use lens::config::Config;
use lens::routes::api_router;
use lens::state::AppState;
use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use tokio::sync::Notify;
use tower::ServiceExt;

const REQUESTS: &str = "lens_check_requests_total";
const IN_FLIGHT: &str = "lens_runs_in_flight";
const DURATION: &str = "lens_run_duration_seconds";

fn golden(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("golden {} unreadable: {e}", path.display()))
}

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    format!("http://{addr}")
}

/// A stub answering the golden; when `gate` is set it announces the request on `.0` and
/// holds the answer until `.1` is notified.
async fn stub(
    path: &'static str,
    is_post: bool,
    content_type: &'static str,
    golden_name: &'static str,
    gate: Option<(Arc<Notify>, Arc<Notify>)>,
) -> String {
    let body = golden(golden_name);
    let handler = move || {
        let body = body.clone();
        let gate = gate.clone();
        async move {
            if let Some((entered, release)) = gate {
                entered.notify_one();
                release.notified().await;
            }
            ([(header::CONTENT_TYPE, content_type)], body)
        }
    };
    let route = if is_post { post(handler) } else { get(handler) };
    serve(Router::new().route(path, route)).await
}

/// Production config on stubs, cache enabled; per-IP limit as given (burst equals limit).
async fn app(per_ip: u32, dns_gate: Option<(Arc<Notify>, Arc<Notify>)>) -> Router {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    config.backends.dns.url = Some(
        stub(
            "/api/check",
            true,
            "text/event-stream",
            "prism.sse",
            dns_gate,
        )
        .await,
    );
    config.backends.tls.url = Some(
        stub(
            "/api/inspect",
            false,
            "application/json",
            "tlsight-inspect.json",
            None,
        )
        .await,
    );
    config.backends.ip.url = Some(
        stub(
            "/json",
            false,
            "application/json",
            "ifconfig-json.json",
            None,
        )
        .await,
    );
    config.backends.http.as_mut().unwrap().url = Some(
        stub(
            "/api/inspect",
            false,
            "application/json",
            "spectra-inspect.json",
            None,
        )
        .await,
    );
    config.backends.email.as_mut().unwrap().url =
        Some(stub("/inspect", true, "text/event-stream", "beacon.sse", None).await);
    assert!(config.cache.enabled, "production config enables the cache");
    config.rate_limit.per_ip_per_minute = per_ip;
    config.rate_limit.per_ip_burst = per_ip;

    let state = AppState::new(config).unwrap();
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
fn requests(s: &Snapshotter, result: &str) -> u64 {
    s.snapshot()
        .into_vec()
        .into_iter()
        .filter(|(k, ..)| {
            k.key().name() == REQUESTS
                && k.key()
                    .labels()
                    .any(|l| l.key() == "result" && l.value() == result)
        })
        .map(|(_, _, _, v)| match v {
            DebugValue::Counter(n) => n,
            _ => 0,
        })
        .sum()
}

fn requests_total(s: &Snapshotter) -> u64 {
    s.snapshot()
        .into_vec()
        .into_iter()
        .filter(|(k, ..)| k.key().name() == REQUESTS)
        .map(|(_, _, _, v)| match v {
            DebugValue::Counter(n) => n,
            _ => 0,
        })
        .sum()
}

/// The gauge's value; `None` when the series was never touched.
fn in_flight(s: &Snapshotter) -> Option<f64> {
    s.snapshot()
        .into_vec()
        .into_iter()
        .find(|(k, ..)| k.key().name() == IN_FLIGHT)
        .and_then(|(_, _, _, v)| match v {
            DebugValue::Gauge(g) => Some(g.into_inner()),
            _ => None,
        })
}

fn duration_observations(s: &Snapshotter) -> usize {
    s.snapshot()
        .into_vec()
        .into_iter()
        .filter(|(k, ..)| k.key().name() == DURATION)
        .map(|(_, _, _, v)| match v {
            DebugValue::Histogram(h) => h.len(),
            _ => 0,
        })
        .sum()
}

// Local recorders are thread-local: a current-thread runtime keeps lens on this thread.

#[tokio::test(flavor = "current_thread")]
async fn admission_c6_fresh_run_counts_fresh_only() {
    let recorder = DebuggingRecorder::new();
    let snap = recorder.snapshotter();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let app = app(10, None).await;

    assert_eq!(post_check(&app, "example.com").await, StatusCode::OK);

    assert_eq!(requests(&snap, "fresh"), 1);
    assert_eq!(requests(&snap, "cache_hit"), 0);
    assert_eq!(requests(&snap, "rate_limited"), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn admission_c7_repeat_within_ttl_counts_cache_hit() {
    let recorder = DebuggingRecorder::new();
    let snap = recorder.snapshotter();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let app = app(10, None).await;

    post_check(&app, "example.com").await;
    post_check(&app, "example.com").await;

    assert_eq!(requests(&snap, "fresh"), 1);
    assert_eq!(requests(&snap, "cache_hit"), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn admission_c8_second_request_over_per_ip_limit_counts_rate_limited() {
    let recorder = DebuggingRecorder::new();
    let snap = recorder.snapshotter();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let app = app(1, None).await;

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
    let snap = recorder.snapshotter();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let app = app(10, None).await;

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
    let snap = recorder.snapshotter();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let app = app(10, Some((entered.clone(), release.clone()))).await;

    let run = post_check(&app, "example.com");
    let observe = async {
        entered.notified().await;
        let during = in_flight(&snap);
        release.notify_one();
        during
    };
    let (status, during) = tokio::join!(run, observe);

    assert_eq!(status, StatusCode::OK);
    assert_eq!(during, Some(1.0), "one run is held open by the stub");
    assert_eq!(in_flight(&snap), Some(0.0), "the run returned");
}

#[tokio::test(flavor = "current_thread")]
async fn admission_c10_in_flight_gauge_is_0_after_handler_future_is_dropped() {
    let recorder = DebuggingRecorder::new();
    let snap = recorder.snapshotter();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let entered = Arc::new(Notify::new());
    // Never released: the stub does not answer before the timeout fires.
    let release = Arc::new(Notify::new());
    let app = app(10, Some((entered.clone(), release))).await;

    let outcome =
        tokio::time::timeout(Duration::from_millis(500), post_check(&app, "example.com")).await;

    assert!(outcome.is_err(), "the handler future is dropped mid-run");
    // The stub was reached, so the run was in flight when the future was dropped.
    tokio::time::timeout(Duration::from_secs(1), entered.notified())
        .await
        .expect("the run reached the backend stub");
    assert_eq!(
        in_flight(&snap),
        Some(0.0),
        "a dropped run is no longer in flight"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn admission_c11_one_fresh_run_observes_one_duration() {
    let recorder = DebuggingRecorder::new();
    let snap = recorder.snapshotter();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let app = app(10, None).await;

    post_check(&app, "example.com").await;
    post_check(&app, "example.com").await; // cache hit: no run, no observation

    assert_eq!(duration_observations(&snap), 1);
}
