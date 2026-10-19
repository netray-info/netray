//! spec v2-api, Phase 3, requirements 14 (criteria C9 to C13) and the refusal half of C19:
//! `POST /api/v1/domains/{domain}/runs` goes through v2-admission's limits.
//!
//! The apex runs with lens's `tests/fixtures/lens.production.toml` (`common::production_config`)
//! on the committed goldens, driven in-process through `netray_web::host::apex::app`. The DNS
//! module is counted and, where a test needs a run in flight, gated. The client of a request without `ConnectInfo` is 127.0.0.1. Timing tests run on
//! tokio's paused clock and no history store; metrics are read from a per-test local recorder on
//! a current-thread runtime.
//!
//! Copied from crates/lens/tests/api_v1_runs_limits.rs (spec v2-cutover, Phase 6); nothing
//! dropped.

use std::cell::RefCell;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
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
use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol};
use netray_web::admission::{
    Admit, AdmittedRuns, ApexState, RunStream, Trigger, admit, check_limits,
};
use netray_web::api::{DomainRuns, RefusalReason, RunStart, RunTrigger};
use netray_web::config::ApexConfig;
use tokio::sync::Notify;
use tower::ServiceExt;

const REJECTIONS: &str = "netray_admission_rejections_total";

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
    state: ApexState,
    entered: Arc<Notify>,
    release: Arc<Notify>,
    runs: Arc<AtomicUsize>,
}

/// Production config after `tune`, limits wide open unless `tune` narrows them; the DNS module
/// is counted, and gated when `gate` is set.
fn harness(gate: bool, tune: impl FnOnce(&mut ApexConfig)) -> Harness {
    let mut config = common::production_config();
    config.limits.rate_limit_per_ip_per_minute = 1000;
    config.limits.rate_limit_per_ip_burst = 1000;
    config.limits.rate_limit_global_per_minute = 10000;
    config.limits.rate_limit_global_burst = 10000;
    tune(&mut config);

    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let runs = Arc::new(AtomicUsize::new(0));
    let dns = if gate {
        gated(
            dns_golden_raw("prism.sse"),
            entered.clone(),
            release.clone(),
        )
    } else {
        dns_golden_raw("prism.sse")
    };
    let dns = Box::new(Counting {
        inner: dns,
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
    let app = netray_web::host::apex::app(state.clone())
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
    Harness {
        app,
        state,
        entered,
        release,
        runs,
    }
}

struct Answer {
    status: StatusCode,
    retry_after: Option<String>,
    code: Option<String>,
}

/// `POST /api/v1/domains/<domain>/runs`, no body, no `Accept`.
async fn post(app: &Router, domain: &str) -> Answer {
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/domains/{domain}/runs"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let retry_after = resp
        .headers()
        .get(header::RETRY_AFTER)
        .map(|v| v.to_str().unwrap().to_string());
    let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    let code = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .and_then(|v| v["error"]["code"].as_str().map(str::to_string));
    Answer {
        status,
        retry_after,
        code,
    }
}

fn assert_refused(a: &Answer, code: &str) {
    assert_eq!(a.status, StatusCode::TOO_MANY_REQUESTS, "code {:?}", a.code);
    assert_eq!(a.code.as_deref(), Some(code));
    assert!(a.retry_after.is_some(), "429 carries Retry-After");
}

/// Starts a run through the coordinator and reads it to its end.
async fn run_to_end(state: &ApexState, domain: &str) {
    let mut stream: RunStream = match admit(state, domain, client(), Trigger::Api, false) {
        Admit::Admitted(s) => s,
        _ => panic!("expected a run for {domain}"),
    };
    while stream.next().await.is_some() {}
    state.runs.drain().await;
}

/// A counter's name, its labels and its accumulated total.
type Total = (String, Vec<(String, String)>, u64);

struct Acc {
    snap: Snapshotter,
    total: RefCell<HashMap<String, Total>>,
}

impl Acc {
    fn new(snap: Snapshotter) -> Self {
        Self {
            snap,
            total: RefCell::new(HashMap::new()),
        }
    }

    /// Counter total for `name{label=value}`; snapshots reset on read, so deltas accumulate.
    fn counter(&self, name: &str, label: &str, value: &str) -> u64 {
        let mut total = self.total.borrow_mut();
        for (k, _, _, v) in self.snap.snapshot().into_vec() {
            let labels: Vec<(String, String)> = k
                .key()
                .labels()
                .map(|l| (l.key().to_string(), l.value().to_string()))
                .collect();
            let id = format!("{}{:?}", k.key().name(), labels);
            let t = total
                .entry(id)
                .or_insert_with(|| (k.key().name().to_string(), labels, 0));
            if let DebugValue::Counter(n) = v {
                t.2 += n;
            }
        }
        total
            .values()
            .filter(|t| t.0 == name && t.1.iter().any(|(k, v)| k == label && v == value))
            .map(|t| t.2)
            .sum()
    }
}

#[tokio::test(start_paused = true)]
async fn api_v1_runs_c9_a_run_10s_ago_answers_429_throttled_with_50_and_starts_nothing() {
    let recorder = DebuggingRecorder::new();
    let acc = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let h = harness(false, |_| {});

    run_to_end(&h.state, "example.com").await;
    assert_eq!(h.runs.load(Ordering::SeqCst), 1);
    tokio::time::advance(Duration::from_secs(10)).await;

    let a = post(&h.app, "example.com").await;

    assert_refused(&a, "THROTTLED");
    assert_eq!(a.retry_after.as_deref(), Some("50"));
    assert_eq!(acc.counter(REJECTIONS, "reason", "throttle"), 1);
    assert_eq!(h.runs.load(Ordering::SeqCst), 1, "no run started");
}

#[tokio::test(start_paused = true)]
async fn api_v1_runs_c10_a_run_60s_ago_lets_a_run_start() {
    let h = harness(false, |_| {});

    run_to_end(&h.state, "example.com").await;
    tokio::time::advance(Duration::from_secs(60)).await;

    let a = post(&h.app, "example.com").await;

    assert_eq!(a.status, StatusCode::OK);
    assert_eq!(h.runs.load(Ordering::SeqCst), 2);
}

#[tokio::test(start_paused = true)]
async fn api_v1_runs_c11_an_exhausted_client_budget_answers_429_budget_exhausted() {
    let h = harness(false, |c| c.limits.client_budget_per_hour = 1);

    run_to_end(&h.state, "example.com").await;

    let a = post(&h.app, "example.net").await;

    assert_refused(&a, "BUDGET_EXHAUSTED");
    assert_eq!(h.runs.load(Ordering::SeqCst), 1, "no second run started");
}

#[tokio::test(start_paused = true)]
async fn api_v1_runs_c12_no_free_slot_answers_429_busy_with_1() {
    let h = harness(true, |c| c.limits.domain_run_slots = 1);

    let first = post(&h.app, "example.com");
    let observe = async {
        tokio::time::timeout(Duration::from_secs(5), h.entered.notified())
            .await
            .expect("the first run entered the gated module");
        let busy = post(&h.app, "example.net").await;
        h.release.notify_one();
        busy
    };
    let (first, busy) = tokio::join!(first, observe);

    assert_eq!(first.status, StatusCode::OK);
    assert_refused(&busy, "BUSY");
    assert_eq!(busy.retry_after.as_deref(), Some("1"));
    assert_eq!(h.runs.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn api_v1_runs_c13_the_rate_limit_guards_runs_and_the_api_limiter_does_not() {
    // [rate_limit] per IP at 1/min, burst 1: the second post is limited.
    let h = harness(false, |c| {
        c.limits.rate_limit_per_ip_per_minute = 1;
        c.limits.rate_limit_per_ip_burst = 1;
    });
    let first = post(&h.app, "example.com").await;
    let second = post(&h.app, "example.net").await;
    assert_eq!(first.status, StatusCode::OK);
    assert_refused(&second, "RATE_LIMITED");
    assert_eq!(h.runs.load(Ordering::SeqCst), 1);

    // [api] per_ip_burst = 1 with the default [rate_limit]: runs are not behind it.
    let h = harness(false, |c| {
        // lens's default [rate_limit].
        let defaults = ApexConfig::default().limits;
        c.limits.rate_limit_per_ip_per_minute = defaults.rate_limit_per_ip_per_minute;
        c.limits.rate_limit_per_ip_burst = defaults.rate_limit_per_ip_burst;
        c.limits.rate_limit_global_per_minute = defaults.rate_limit_global_per_minute;
        c.limits.rate_limit_global_burst = defaults.rate_limit_global_burst;
        c.api.per_ip_burst = 1;
    });
    let first = post(&h.app, "example.com").await;
    let second = post(&h.app, "example.net").await;
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(second.status, StatusCode::OK);
    assert_eq!(h.runs.load(Ordering::SeqCst), 2);
}

/// C19: `AdmittedRuns::start` (lens's `LensRuns::start`) refuses on a spent per-IP or global
/// limit before `admit`: no run, no throttle record; through the route both answer 429
/// `RATE_LIMITED`.
#[tokio::test(flavor = "current_thread")]
async fn api_v1_runs_c19_a_spent_limit_refuses_before_admit() {
    // Quotas refill every 100 ms on governor's real clock.
    let cases: [(&str, RefusalReason); 2] = [
        ("ip", RefusalReason::IpLimit),
        ("global", RefusalReason::GlobalLimit),
    ];
    for (scope, reason) in cases {
        let tune = |c: &mut ApexConfig| {
            if scope == "ip" {
                c.limits.rate_limit_per_ip_per_minute = 600;
                c.limits.rate_limit_per_ip_burst = 1;
            } else {
                c.limits.rate_limit_global_per_minute = 600;
                c.limits.rate_limit_global_burst = 1;
            }
        };

        // Through AdmittedRuns::start.
        let h = harness(false, tune);
        check_limits(&h.state, client()).expect("the first call passes the limiter");
        let runs = AdmittedRuns::new(h.state.clone());
        match runs
            .start("example.com", client(), RunTrigger::Api, false)
            .await
        {
            RunStart::Refused(r) => assert_eq!(r.reason, reason, "scope {scope}"),
            RunStart::Admitted(_) => panic!("scope {scope}: expected a refusal, got a run"),
            RunStart::NotStarted => panic!("scope {scope}: expected a refusal, got NotStarted"),
        }
        assert_eq!(h.runs.load(Ordering::SeqCst), 0, "scope {scope}: no run");
        // No throttle record: once the limiter refills, a post is not THROTTLED.
        std::thread::sleep(Duration::from_millis(300));
        let a = post(&h.app, "example.com").await;
        assert_eq!(a.status, StatusCode::OK, "scope {scope}: code {:?}", a.code);
        assert_eq!(h.runs.load(Ordering::SeqCst), 1, "scope {scope}");

        // Through the route.
        let h = harness(false, tune);
        check_limits(&h.state, client()).expect("the first call passes the limiter");
        let a = post(&h.app, "example.com").await;
        assert_refused(&a, "RATE_LIMITED");
        assert_eq!(h.runs.load(Ordering::SeqCst), 0, "scope {scope}: no run");
    }
}
