//! Joining a run in flight (specs/features/v2-admission/spec.md, requirements 2 and 5;
//! criteria C1/C6, C7, C8, C9, C4/C15).
//!
//! The DNS module is gated and counts its runs; lens runs with `tests/fixtures/lens.production.toml`
//! on the committed goldens, driven in-process. The metrics are read from a per-test local
//! recorder; a current-thread runtime keeps the handlers on the recording thread.

use std::cell::RefCell;
use std::collections::HashMap;
use std::net::SocketAddr;
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
    dns_golden_raw, email_golden, facts_golden_raw, gated, http_module, ip_golden, registry_with,
    tls_golden,
};
use lens::config::Config;
use lens::routes::{api_router, badge_router};
use lens::snapshot::SnapshotStore;
use lens::state::AppState;
use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol};
use serde_json::Value;
use tokio::sync::Notify;
use tower::ServiceExt;

const REQUESTS: &str = "lens_check_requests_total";

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
    entered: Arc<Notify>,
    release: Arc<Notify>,
    runs: Arc<AtomicUsize>,
    _snapshots: tempfile::NamedTempFile,
}

/// Production config (cache on), a snapshot store, the DNS module gated and counted.
async fn harness() -> Harness {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    config.rate_limit.per_ip_per_minute = 100;
    config.rate_limit.per_ip_burst = 100;

    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let runs = Arc::new(AtomicUsize::new(0));
    let dns = Box::new(Counting {
        inner: gated(
            dns_golden_raw("prism.sse"),
            entered.clone(),
            release.clone(),
        ),
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

    let tmp = tempfile::NamedTempFile::new().unwrap();
    let store = SnapshotStore::new(tmp.path()).await.unwrap();
    store.migrate().await.unwrap();
    state.snapshot_store = Some(Arc::new(store));

    let (api, _) = api_router().split_for_parts();
    let (badge, _) = badge_router().split_for_parts();
    let app = Router::new()
        .merge(api.with_state(state.clone()))
        .merge(badge.with_state(state))
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
    Harness {
        app,
        entered,
        release,
        runs,
        _snapshots: tmp,
    }
}

async fn check(app: &Router, domain: &str, dkim: Option<&str>) -> (StatusCode, Value) {
    let body = match dkim {
        Some(s) => format!(r#"{{"domain":"{domain}","stream":false,"dkim_selectors":"{s}"}}"#),
        None => format!(r#"{{"domain":"{domain}","stream":false}}"#),
    };
    let req = Request::builder()
        .method("POST")
        .uri("/api/check")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn badge(app: &Router, domain: &str) -> StatusCode {
    let req = Request::builder()
        .uri(format!("/badge/{domain}.svg"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    status
}

/// Lets the first request reach the gate and the later ones arrive, then opens the gate for
/// every run that waits (a permit left over is harmless).
async fn open_gate_after_arrivals(entered: &Notify, release: &Notify) {
    entered.notified().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    for _ in 0..10 {
        release.notify_one();
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// A request that arrives while the first one holds the gate.
async fn later() {
    tokio::time::sleep(Duration::from_millis(200)).await;
}

fn requests(s: &Acc, result: &str) -> u64 {
    s.series()
        .into_iter()
        .filter(|t| {
            t.name == REQUESTS && t.labels.iter().any(|(k, v)| k == "result" && v == result)
        })
        .map(|t| t.counter)
        .sum()
}

/// Accumulates snapshots: `Snapshotter::snapshot()` resets counters to 0 on read, so every
/// read adds its delta to a running total.
struct Acc {
    snap: Snapshotter,
    total: RefCell<HashMap<String, Total>>,
}

#[derive(Clone)]
struct Total {
    name: String,
    labels: Vec<(String, String)>,
    counter: u64,
}

impl Acc {
    fn new(snap: Snapshotter) -> Self {
        Self {
            snap,
            total: RefCell::new(HashMap::new()),
        }
    }

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
            });
            if let DebugValue::Counter(n) = v {
                t.counter += n;
            }
        }
        total.values().cloned().collect()
    }
}

#[tokio::test(flavor = "current_thread")]
async fn admission_join_c1_c6_second_check_joins_the_run_in_flight() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let h = harness().await;

    let first = check(&h.app, "example.com", None);
    let second = async {
        later().await;
        check(&h.app, "example.com", None).await
    };
    let ((s1, b1), (s2, b2), ()) = tokio::join!(
        first,
        second,
        open_gate_after_arrivals(&h.entered, &h.release)
    );

    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(h.runs.load(Ordering::SeqCst), 1, "exactly one run executes");
    for section in ["dns", "tls", "http", "email", "ip", "summary"] {
        assert_eq!(b1[section], b2[section], "section {section} is the same");
    }
    let id1 = b1["done"]["snapshot_id"].as_str().expect("snapshot id");
    assert_eq!(b2["done"]["snapshot_id"].as_str(), Some(id1));
    assert_eq!(requests(&snap, "joined"), 1);
    assert_eq!(requests(&snap, "fresh"), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn admission_join_c7_badge_joins_the_run_in_flight() {
    let h = harness().await;

    let first = check(&h.app, "example.com", None);
    let second = async {
        later().await;
        badge(&h.app, "example.com").await
    };
    let ((s1, _), s2, ()) = tokio::join!(
        first,
        second,
        open_gate_after_arrivals(&h.entered, &h.release)
    );

    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(h.runs.load(Ordering::SeqCst), 1, "the badge joins the run");
}

#[tokio::test(flavor = "current_thread")]
async fn admission_join_c8_another_domain_runs_on_its_own() {
    let h = harness().await;

    let first = check(&h.app, "example.com", None);
    let second = async {
        later().await;
        check(&h.app, "example.net", None).await
    };
    let ((s1, _), (s2, _), ()) = tokio::join!(
        first,
        second,
        open_gate_after_arrivals(&h.entered, &h.release)
    );

    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(h.runs.load(Ordering::SeqCst), 2, "one run per domain");
}

#[tokio::test(flavor = "current_thread")]
async fn admission_join_c9_dkim_selectors_never_join_and_are_never_joined() {
    // A request with options during a run without options.
    let h = harness().await;
    let first = check(&h.app, "example.com", None);
    let second = async {
        later().await;
        check(&h.app, "example.com", Some("sel1")).await
    };
    let ((s1, _), (s2, _), ()) = tokio::join!(
        first,
        second,
        open_gate_after_arrivals(&h.entered, &h.release)
    );
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(
        h.runs.load(Ordering::SeqCst),
        2,
        "a request with dkim_selectors starts its own run"
    );

    // A request without options during a run with options.
    let h = harness().await;
    let first = check(&h.app, "example.com", Some("sel1"));
    let second = async {
        later().await;
        check(&h.app, "example.com", None).await
    };
    let ((s1, _), (s2, _), ()) = tokio::join!(
        first,
        second,
        open_gate_after_arrivals(&h.entered, &h.release)
    );
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(
        h.runs.load(Ordering::SeqCst),
        2,
        "a run with dkim_selectors is not joined"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn admission_join_c4_c15_sequential_requests_are_both_fresh() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let h = harness().await;
    // The gate stays open: a permit per run.
    h.release.notify_one();
    let (s1, _) = check(&h.app, "example.com", Some("sel1")).await;
    h.release.notify_one();
    let (s2, _) = check(&h.app, "example.com", Some("sel2")).await;

    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(requests(&snap, "fresh"), 2);
    assert_eq!(requests(&snap, "joined"), 0);
}
