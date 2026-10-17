//! The per-client run budget (specs/features/v2-admission/spec.md, Phase 4, requirement 13;
//! criteria C2, C14, C15, C16, C17, C18, C28, C29).
//!
//! lens runs with `tests/fixtures/lens.production.toml` on the committed goldens, per-IP limits
//! raised so only the budget refuses. Two clients are two routers on the same `AppState`, each
//! with its own `MockConnectInfo` (the peer is the client: it is no trusted proxy). The DNS
//! module counts the runs and can be gated. Different domains avoid the per-domain throttle.

use std::cell::RefCell;
use std::collections::HashMap;
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
    dns_golden_raw, email_golden, facts_golden_raw, gated, http_module, ip_golden, registry_with,
    tls_golden,
};
use lens::admission::{Admit, Reason, Refusal, Trigger, admit};
use lens::config::Config;
use lens::routes::{api_router, badge_router};
use lens::state::AppState;
use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol};
use netray_store::HistoryStore;
use tokio::sync::Notify;
use tower::ServiceExt;

const X: [u8; 4] = [192, 0, 2, 1];
const Y: [u8; 4] = [192, 0, 2, 2];

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
    fn needs_address_records(&self) -> bool {
        self.inner.needs_address_records()
    }
    fn run<'a>(&'a self, ctx: &'a RunContext, facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        self.inner.run(ctx, facts)
    }
}

struct Harness {
    state: AppState,
    runs: Arc<AtomicUsize>,
    _dir: Option<tempfile::TempDir>,
}

async fn harness(
    budget: u32,
    gate: Option<(Arc<Notify>, Arc<Notify>)>,
    with_store: bool,
) -> Harness {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    config.rate_limit.per_ip_per_minute = 100;
    config.rate_limit.per_ip_burst = 100;
    config.admission.client_budget_per_hour = budget;

    let runs = Arc::new(AtomicUsize::new(0));
    let counted: Box<dyn Module> = Box::new(Counting {
        inner: dns_golden_raw("prism.sse"),
        runs: runs.clone(),
    });
    let dns = match gate {
        Some((entered, release)) => gated(counted, entered, release),
        None => counted,
    };
    let registry = registry_with(
        dns,
        facts_golden_raw("prism.sse"),
        http_module(Some("spectra-inspect.json")),
        email_golden("beacon.sse"),
        ip_golden("ifconfig-json.json"),
        tls_golden("tlsight-inspect.json"),
    );
    let mut state = AppState::with_registry(config, registry).unwrap();
    let mut dir = None;
    if with_store {
        let d = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(&d.path().join("history.db"))
            .await
            .unwrap();
        store.migrate().await.unwrap();
        state.history_store = Some(Arc::new(store));
        dir = Some(d);
    }
    Harness {
        state,
        runs,
        _dir: dir,
    }
}

fn api(state: &AppState, client: [u8; 4]) -> Router {
    let (api, _) = api_router().split_for_parts();
    Router::new()
        .merge(api.with_state(state.clone()))
        .layer(MockConnectInfo(SocketAddr::from((client, 40000))))
}

fn badges(state: &AppState, client: [u8; 4]) -> Router {
    let (badge, _) = badge_router().split_for_parts();
    Router::new()
        .merge(badge.with_state(state.clone()))
        .layer(MockConnectInfo(SocketAddr::from((client, 40000))))
}

struct Answer {
    status: StatusCode,
    x_cache: String,
    retry_after: Option<String>,
}

async fn check(app: &Router, domain: &str) -> Answer {
    let req = Request::builder()
        .method("POST")
        .uri("/api/check")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(r#"{{"domain":"{domain}","stream":false}}"#)))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let x_cache = resp
        .headers()
        .get("x-cache")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let retry_after = resp
        .headers()
        .get("retry-after")
        .map(|v| v.to_str().unwrap().to_string());
    to_bytes(resp.into_body(), 8 * 1024 * 1024).await.unwrap();
    Answer {
        status,
        x_cache,
        retry_after,
    }
}

async fn badge(app: &Router, domain: &str) -> StatusCode {
    let req = Request::builder()
        .uri(format!("/badge/{domain}.svg"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    to_bytes(resp.into_body(), 8 * 1024 * 1024).await.unwrap();
    status
}

/// Poll (bounded, 5 s) until the domain has a head entry and its V1 rendering is held.
async fn wait_for_head_and_rendering(state: &AppState, domain: &str) {
    let store = state.history_store.as_ref().expect("history store wired");
    for _ in 0..250 {
        if let Some(head) = store.head(domain).await.unwrap()
            && state.cache.as_ref().unwrap().get(&head.id).await.is_some()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the run was not recorded and held within 5 s");
}

/// Accumulates snapshots (`Snapshotter::snapshot()` resets counters on read).
struct Acc {
    snap: Snapshotter,
    total: RefCell<HashMap<String, u64>>,
}

impl Acc {
    fn new(snap: Snapshotter) -> Self {
        Self {
            snap,
            total: RefCell::new(HashMap::new()),
        }
    }

    /// Value of `name{label=value}`; an absent series counts 0.
    fn counter(&self, name: &str, label: &str, value: &str) -> u64 {
        let mut total = self.total.borrow_mut();
        for (k, _, _, v) in self.snap.snapshot().into_vec() {
            if let DebugValue::Counter(n) = v {
                let labels: Vec<String> = k
                    .key()
                    .labels()
                    .map(|l| format!("{}={}", l.key(), l.value()))
                    .collect();
                *total
                    .entry(format!("{}{{{}}}", k.key().name(), labels.join(",")))
                    .or_insert(0) += n;
            }
        }
        total
            .get(&format!("{name}{{{label}={value}}}"))
            .copied()
            .unwrap_or(0)
    }
}

// Local recorders are thread-local: a current-thread runtime keeps lens on this thread.

#[tokio::test(flavor = "current_thread")]
async fn admission_budget_c2_r13_fresh_runs_spend_a_token_each_per_client() {
    let h = harness(1, None, false).await;
    let x = api(&h.state, X);

    assert_eq!(check(&x, "example.com").await.status, StatusCode::OK);
    let second = check(&x, "example.net").await;
    assert_eq!(second.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(second.retry_after.is_some(), "a refusal names when to ask again");
    assert_eq!(h.runs.load(Ordering::SeqCst), 1, "the refused request starts no run");

    // Another client has its own bucket.
    let y = api(&h.state, Y);
    assert_eq!(check(&y, "example.net").await.status, StatusCode::OK);
}

#[tokio::test(flavor = "current_thread")]
async fn admission_budget_c14_third_run_of_a_budget_of_two_is_refused() {
    let recorder = DebuggingRecorder::new();
    let acc = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let h = harness(2, None, false).await;
    let x = api(&h.state, X);

    assert_eq!(check(&x, "example.com").await.status, StatusCode::OK);
    assert_eq!(check(&x, "example.net").await.status, StatusCode::OK);
    let third = check(&x, "example.org").await;

    assert_eq!(third.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(third.retry_after.is_some(), "429 carries Retry-After");
    assert_eq!(
        acc.counter("lens_admission_rejections_total", "reason", "budget"),
        1
    );
    assert_eq!(h.runs.load(Ordering::SeqCst), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn admission_budget_c15_hits_spend_no_token() {
    let h = harness(1, None, true).await;
    // Y seeds example.com: a complete run, recorded, rendering held.
    let y = api(&h.state, Y);
    assert_eq!(check(&y, "example.com").await.status, StatusCode::OK);
    wait_for_head_and_rendering(&h.state, "example.com").await;

    let x = api(&h.state, X);
    for _ in 0..2 {
        let hit = check(&x, "example.com").await;
        assert_eq!(hit.status, StatusCode::OK);
        assert_eq!(hit.x_cache, "HIT");
    }
    let run = check(&x, "example.net").await;
    assert_eq!(run.status, StatusCode::OK);
    assert_eq!(run.x_cache, "MISS", "the untouched token pays for a run");
    assert_eq!(h.runs.load(Ordering::SeqCst), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn admission_budget_c16_joining_spends_no_token() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let h = harness(1, Some((entered.clone(), release.clone())), false).await;
    let x = api(&h.state, X);
    let y = api(&h.state, Y);

    let run_x = tokio::spawn(async move { check(&x, "example.com").await.status });
    entered.notified().await;
    let join_y = {
        let y = y.clone();
        tokio::spawn(async move { check(&y, "example.com").await.status })
    };
    // Let Y reach the run in flight, then open the gate.
    tokio::time::sleep(Duration::from_millis(200)).await;
    release.notify_one();
    assert_eq!(run_x.await.unwrap(), StatusCode::OK);
    assert_eq!(join_y.await.unwrap(), StatusCode::OK, "Y joined X's run");
    assert_eq!(h.runs.load(Ordering::SeqCst), 1, "one run served both");

    let next_y = tokio::spawn(async move { check(&y, "example.net").await.status });
    entered.notified().await;
    release.notify_one();
    assert_eq!(next_y.await.unwrap(), StatusCode::OK, "Y's token is intact");
    assert_eq!(h.runs.load(Ordering::SeqCst), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn admission_budget_c17_each_client_has_its_own_budget() {
    let h = harness(1, None, false).await;
    let x = api(&h.state, X);
    let y = api(&h.state, Y);

    assert_eq!(check(&x, "example.com").await.status, StatusCode::OK);
    assert_eq!(check(&y, "example.net").await.status, StatusCode::OK);
    assert_eq!(h.runs.load(Ordering::SeqCst), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn admission_budget_c18_badges_never_spend_a_token() {
    let h = harness(1, None, false).await;
    let b = badges(&h.state, X);

    for domain in ["example.com", "example.net", "example.org"] {
        assert_eq!(badge(&b, domain).await, StatusCode::OK);
    }
    assert_eq!(h.runs.load(Ordering::SeqCst), 3, "all three badges ran");
}

#[tokio::test(flavor = "multi_thread")]
async fn admission_budget_c28_budget_zero_refuses_every_visit_recheck_and_api_run() {
    let h = harness(0, None, false).await;
    let client: IpAddr = IpAddr::from(X);
    let want = Refusal {
        reason: Reason::Budget,
        retry_after_s: 3600,
        last_run_ago_s: None,
        last_incomplete: false,
    };

    for trigger in [Trigger::Visit, Trigger::Recheck] {
        match admit(&h.state, "example.com", client, trigger, false) {
            Admit::Refused(got) => assert_eq!(got, want),
            _ => panic!("budget 0 must refuse a {trigger:?} run"),
        }
    }

    let a = check(&api(&h.state, X), "example.net").await;
    assert_eq!(a.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(a.retry_after.as_deref(), Some("3600"));
    assert_eq!(h.runs.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn admission_budget_c29_budget_zero_still_runs_badges_and_serves_hits() {
    let h = harness(0, None, true).await;

    let b = badges(&h.state, X);
    assert_eq!(badge(&b, "example.com").await, StatusCode::OK);
    assert_eq!(h.runs.load(Ordering::SeqCst), 1, "the badge started a run");
    wait_for_head_and_rendering(&h.state, "example.com").await;

    let hit = check(&api(&h.state, X), "example.com").await;
    assert_eq!(hit.status, StatusCode::OK);
    assert_eq!(hit.x_cache, "HIT");
    assert_eq!(h.runs.load(Ordering::SeqCst), 1, "the hit starts no run");
}
