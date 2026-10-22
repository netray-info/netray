//! Joining a run in flight (specs/features/v2-admission/spec.md, requirements 2 and 5;
//! criteria C1/C6, C7, C8, C9, C4/C15).
//!
//! The DNS module is gated and counts its runs; the apex runs with the state of
//! `tests/fixtures/lens.production.toml` on the committed goldens, driven in-process. The metrics
//! are read from a per-test local recorder; a current-thread runtime keeps the handlers on the
//! recording thread.
//!
//! Copied from crates/lens/tests/admission_join.rs (spec v2-cutover, Phase 6); dropped as V1:
//! `POST /api/check`, its V1 section payloads and snapshot ids, the snapshot store, and C9 whole
//! (DKIM-selector runs). Runs go through API v1's `POST /api/v1/domains/{domain}/runs`; C1/C6
//! compares the two answers' `run`, and C4/C15 runs without selectors.

use std::cell::RefCell;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::common;

use axum::Extension;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use common::{
    dns_golden_raw, email_golden, facts_golden_raw, gated, http_module, ip_golden, registry_with,
    tls_golden,
};
use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol};
use serde_json::Value;
use tokio::sync::Notify;
use tower::ServiceExt;

const RUNS: &str = "netray_runs_total";

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
}

/// Production config (cache on), the DNS module gated and counted.
async fn harness() -> Harness {
    let mut config = common::production_config();
    config.limits.rate_limit_per_ip_per_minute = 100;
    config.limits.rate_limit_per_ip_burst = 100;

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
    let plan = common::production_plan(&registry);
    let state = common::state_with(config, registry, plan);

    let app = netray_web::host::apex::routes(state).layer(Extension(ConnectInfo(
        SocketAddr::from(([127, 0, 0, 1], 40000)),
    )));
    Harness {
        app,
        entered,
        release,
        runs,
    }
}

/// API v1's fresh run of `domain`: the status and the JSON answer.
async fn check(app: &Router, domain: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/domains/{domain}/runs"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 16 * 1024 * 1024).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
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

/// `netray_runs_total{trigger}` summed over `outcome`.
fn runs(s: &Acc, trigger: &str) -> u64 {
    s.series()
        .into_iter()
        .filter(|t| t.name == RUNS && t.labels.iter().any(|(k, v)| k == "trigger" && v == trigger))
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

    let first = check(&h.app, "example.com");
    let second = async {
        later().await;
        check(&h.app, "example.com").await
    };
    let ((s1, b1), (s2, b2), ()) = tokio::join!(
        first,
        second,
        open_gate_after_arrivals(&h.entered, &h.release)
    );

    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(h.runs.load(Ordering::SeqCst), 1, "exactly one run executes");
    assert!(b1["run"].is_object(), "the answer carries the run: {b1}");
    assert_eq!(b1["run"], b2["run"], "both answers carry the same run");
    assert_eq!(b1["recorded"], b2["recorded"]);
    // `joined` has no successor: the joiner starts no run, so one run counts for both.
    assert_eq!(runs(&snap, "api"), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn admission_join_c7_badge_joins_the_run_in_flight() {
    let h = harness().await;

    let first = check(&h.app, "example.com");
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

    let first = check(&h.app, "example.com");
    let second = async {
        later().await;
        check(&h.app, "example.net").await
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
async fn admission_join_c4_c15_sequential_requests_are_both_fresh() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let h = harness().await;
    // The gate stays open: a permit per run.
    h.release.notify_one();
    let (s1, _) = check(&h.app, "example.com").await;
    // v2-admission R12/R16: the second run of the domain starts only after the throttle.
    common::pass_throttle().await;
    h.release.notify_one();
    let (s2, _) = check(&h.app, "example.com").await;

    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(runs(&snap, "api"), 2);
}
