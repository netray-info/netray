//! Slots, rejection metrics and the public admission surface (specs/features/v2-admission/spec.md,
//! Phase 4, requirements 14, 15, 17 and 20; criteria C3, C4, C6, C7, C19 to C22, C24, C30 to C32).
//!
//! The DNS module is gated and counted; lens runs with `tests/fixtures/lens.production.toml` on the
//! committed goldens, driven in-process. A request's client is the router's `MockConnectInfo`
//! address: two routers on one `AppState` are two clients. Metrics are read from a per-test local
//! recorder; a current-thread runtime keeps the handlers on the recording thread.

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
use lens::admission::{Admit, Reason, Refusal, RunEvent, RunStream, Trigger, admit, check_limits};
use lens::config::Config;
use lens::routes::{api_router, badge_router};
use lens::state::AppState;
use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol};
use netray_store::HistoryStore;
use tokio::sync::Notify;
use tower::ServiceExt;

const REJECTIONS: &str = "lens_admission_rejections_total";
const REQUESTS: &str = "lens_check_requests_total";

const A: [u8; 4] = [10, 0, 0, 1];
const B: [u8; 4] = [10, 0, 0, 2];

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
    state: AppState,
    entered: Arc<Notify>,
    release: Arc<Notify>,
    runs: Arc<AtomicUsize>,
    _dir: tempfile::TempDir,
}

/// Production config (cache on) after `tune`, the DNS module gated and counted, optionally a
/// temp-file history store.
async fn harness(history: bool, tune: impl FnOnce(&mut Config)) -> Harness {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    config.rate_limit.per_ip_per_minute = 100;
    config.rate_limit.per_ip_burst = 100;
    tune(&mut config);

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
    let dir = tempfile::tempdir().unwrap();
    if history {
        let store = HistoryStore::open(&dir.path().join("history.db"))
            .await
            .unwrap();
        store.migrate().await.unwrap();
        state.history_store = Some(Arc::new(store));
    }
    Harness {
        state,
        entered,
        release,
        runs,
        _dir: dir,
    }
}

/// The routes as seen from the client `ip`.
fn router(state: &AppState, ip: [u8; 4]) -> Router {
    let (api, _) = api_router().split_for_parts();
    let (badge, _) = badge_router().split_for_parts();
    Router::new()
        .merge(api.with_state(state.clone()))
        .merge(badge.with_state(state.clone()))
        .layer(MockConnectInfo(SocketAddr::from((ip, 40000))))
}

fn ip(octets: [u8; 4]) -> IpAddr {
    IpAddr::from(octets)
}

/// Status and `Retry-After` of a non-streaming `/api/check`.
async fn check(app: &Router, domain: &str) -> (StatusCode, Option<String>) {
    let req = Request::builder()
        .method("POST")
        .uri("/api/check")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(
            r#"{{"domain":"{domain}","stream":false}}"#
        )))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let retry = resp
        .headers()
        .get(header::RETRY_AFTER)
        .map(|v| v.to_str().unwrap().to_string());
    to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    (status, retry)
}

async fn badge_svg(app: &Router, domain: &str) -> String {
    let req = Request::builder()
        .uri(format!("/badge/{domain}.svg"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// Lets the run that holds the gate be reached and the later requests arrive, then releases it.
async fn release_after_arrivals(entered: &Notify, release: &Notify) {
    entered.notified().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    release.notify_one();
}

/// A request that arrives while the first one holds the gate.
async fn later() {
    tokio::time::sleep(Duration::from_millis(200)).await;
}

fn admitted(a: Admit) -> RunStream {
    match a {
        Admit::Admitted(s) => s,
        Admit::Refused(r) => panic!("expected a run, got {r:?}"),
        Admit::NotStarted => panic!("expected a run, got NotStarted"),
    }
}

fn refused(a: Admit) -> Refusal {
    match a {
        Admit::Refused(r) => r,
        Admit::Admitted(_) => panic!("expected a refusal, got a run"),
        Admit::NotStarted => panic!("expected a refusal, got NotStarted"),
    }
}

/// Admits a run and waits until it holds the gate.
async fn start_gated(h: &Harness, domain: &str, client: [u8; 4], trigger: Trigger) -> RunStream {
    let stream = admitted(admit(&h.state, domain, ip(client), trigger, false));
    h.entered.notified().await;
    stream
}

async fn finish(stream: &mut RunStream) {
    while stream.next().await.is_some() {}
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

fn counter(s: &Acc, name: &str, key: &str, value: &str) -> u64 {
    s.series()
        .into_iter()
        .filter(|t| t.name == name && t.labels.iter().any(|(k, v)| k == key && v == value))
        .map(|t| t.counter)
        .sum()
}

fn rejections(s: &Acc, reason: &str) -> u64 {
    counter(s, REJECTIONS, "reason", reason)
}

fn requests(s: &Acc, result: &str) -> u64 {
    counter(s, REQUESTS, "result", result)
}

/// Accumulates snapshots: `Snapshotter::snapshot()` resets counters to 0 on read.
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

// --- criteria ---------------------------------------------------------------------------------

/// C3 (R14): a fresh run holds a slot for its whole run, a join takes none, none free refuses
/// `slots` with 1; the slot is free again once the run ended.
#[tokio::test(flavor = "current_thread")]
async fn admission_slots_c3_a_run_holds_a_slot_for_its_whole_run() {
    let h = harness(false, |c| c.admission.domain_run_slots = 1).await;

    let mut first = start_gated(&h, "example.com", A, Trigger::Api).await;
    let mut joined = admitted(admit(&h.state, "example.com", ip(B), Trigger::Api, false));
    assert!(joined.joined(), "a join takes no slot");
    let r = refused(admit(&h.state, "example.net", ip(B), Trigger::Api, false));
    assert_eq!(
        r,
        Refusal {
            reason: Reason::Slots,
            retry_after_s: 1,
            last_run_ago_s: None,
            last_incomplete: false
        }
    );

    h.release.notify_one();
    finish(&mut first).await;
    finish(&mut joined).await;
    h.state.runs.drain().await;

    h.release.notify_one();
    let mut next = admitted(admit(&h.state, "example.net", ip(B), Trigger::Api, false));
    finish(&mut next).await;
}

/// C4 (R15): a refusal spends no budget token and starts no throttle minute.
#[tokio::test(flavor = "current_thread")]
async fn admission_slots_c4_a_refusal_spends_nothing() {
    let h = harness(false, |c| {
        c.admission.domain_run_slots = 1;
        c.admission.client_budget_per_hour = 1;
    })
    .await;

    let mut first = start_gated(&h, "example.com", A, Trigger::Api).await;
    let r = refused(admit(&h.state, "example.net", ip(B), Trigger::Api, false));
    assert_eq!(r.reason, Reason::Slots);
    h.release.notify_one();
    finish(&mut first).await;
    h.state.runs.drain().await;

    // B's single token is intact and example.net has no throttle minute.
    h.release.notify_one();
    let mut run = admitted(admit(&h.state, "example.net", ip(B), Trigger::Api, false));
    finish(&mut run).await;
    h.state.runs.drain().await;

    // That run spent the token, exactly once.
    let r = refused(admit(&h.state, "example.org", ip(B), Trigger::Api, false));
    assert_eq!(r.reason, Reason::Budget);
}

/// C6 (R17): every refusal counts its reason; a refused `/api/check` also counts `rate_limited`.
#[tokio::test(flavor = "current_thread")]
async fn admission_slots_c6_every_refusal_counts_its_reason() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let h = harness(false, |c| {
        c.admission.domain_run_slots = 1;
        c.admission.client_budget_per_hour = 2;
        c.rate_limit.per_ip_per_minute = 1;
        c.rate_limit.per_ip_burst = 5;
    })
    .await;
    let app = router(&h.state, A);

    // 1: the run holds the slot; 2: another domain is refused `slots`.
    let first = check(&app, "example.com");
    let second = async {
        later().await;
        check(&app, "example.net").await
    };
    let ((s1, _), (s2, _), ()) = tokio::join!(
        first,
        second,
        release_after_arrivals(&h.entered, &h.release)
    );
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::TOO_MANY_REQUESTS);
    h.state.runs.drain().await;

    // 3: example.com started a minute a moment ago.
    assert_eq!(
        check(&app, "example.com").await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    // 4: the second token of the budget.
    h.release.notify_one();
    assert_eq!(check(&app, "example.net").await.0, StatusCode::OK);
    h.state.runs.drain().await;
    // 5: the budget is spent.
    assert_eq!(
        check(&app, "example.org").await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    // 6: the per-IP burst of five is spent.
    assert_eq!(
        check(&app, "example.org").await.0,
        StatusCode::TOO_MANY_REQUESTS
    );

    assert_eq!(rejections(&snap, "slots"), 1);
    assert_eq!(rejections(&snap, "throttle"), 1);
    assert_eq!(rejections(&snap, "budget"), 1);
    assert_eq!(rejections(&snap, "limit"), 1);
    assert_eq!(requests(&snap, "rate_limited"), 4);
}

/// C7 (R20): `check_limits`, `admit`, `Trigger`, `Reason` and `Refusal` are public in `admission`.
#[tokio::test(flavor = "current_thread")]
async fn admission_slots_c7_the_admission_surface_is_public() {
    let h = harness(false, |_| {}).await;

    assert!(check_limits(&h.state, ip(A)).is_ok());

    let triggers = [
        Trigger::Visit,
        Trigger::Recheck,
        Trigger::Api,
        Trigger::Badge,
        Trigger::Og,
    ];
    assert_eq!(triggers.len(), 5);
    let refusal = Refusal {
        reason: Reason::Throttle,
        retry_after_s: 10,
        last_run_ago_s: Some(50),
        last_incomplete: true,
    };
    assert_eq!(refusal.clone(), refusal);
    assert_ne!(Reason::IpLimit, Reason::GlobalLimit);
    assert_ne!(Reason::Budget, Reason::Slots);

    h.release.notify_one();
    let mut run = admitted(admit(&h.state, "example.com", ip(A), Trigger::Badge, false));
    assert!(!run.joined());
    let mut seen_done = false;
    while let Some(ev) = run.next().await {
        seen_done |= matches!(ev, RunEvent::Done(_));
    }
    assert!(seen_done);
    h.state.runs.drain().await;
    h.release.notify_one();
    let mut run = admitted(admit(&h.state, "example.net", ip(A), Trigger::Og, false));
    finish(&mut run).await;
}

/// C19: slots exhausted: `/api/check` of another domain is 429 with `Retry-After: 1`, counted
/// `slots` and `rate_limited`; its badge shows `?`.
#[tokio::test(flavor = "current_thread")]
async fn admission_slots_c19_no_free_slot_refuses_check_and_badge() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let h = harness(false, |c| c.admission.domain_run_slots = 1).await;
    let app = router(&h.state, A);

    let first = check(&app, "example.com");
    let second = async {
        later().await;
        let refused = check(&app, "example.net").await;
        let counted = (rejections(&snap, "slots"), requests(&snap, "rate_limited"));
        let svg = badge_svg(&app, "example.net").await;
        (refused, counted, svg)
    };
    let ((s1, _), ((status, retry), counted, svg), ()) = tokio::join!(
        first,
        second,
        release_after_arrivals(&h.entered, &h.release)
    );

    assert_eq!(s1, StatusCode::OK);
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(retry.as_deref(), Some("1"));
    assert_eq!(counted, (1, 1));
    assert!(svg.contains(">?</text>"), "badge must show ?, got: {svg}");
}

/// C20: a request for the domain whose run holds the only slot joins it.
#[tokio::test(flavor = "current_thread")]
async fn admission_slots_c20_the_domain_in_flight_joins_without_a_slot() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let h = harness(false, |c| c.admission.domain_run_slots = 1).await;
    let app = router(&h.state, A);

    let first = check(&app, "example.com");
    let second = async {
        later().await;
        check(&app, "example.com").await
    };
    let ((s1, _), (s2, _), ()) = tokio::join!(
        first,
        second,
        release_after_arrivals(&h.entered, &h.release)
    );

    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(h.runs.load(Ordering::SeqCst), 1);
    assert_eq!(requests(&snap, "joined"), 1);
}

/// C21: a slots refusal leaves nothing behind: once the slot is free, the same domain runs, is
/// not throttled, and the client's budget (2) still covers it.
#[tokio::test(flavor = "current_thread")]
async fn admission_slots_c21_a_refused_domain_runs_once_a_slot_is_free() {
    let h = harness(false, |c| {
        c.admission.domain_run_slots = 1;
        c.admission.client_budget_per_hour = 2;
    })
    .await;
    let app = router(&h.state, A);

    let first = check(&app, "example.com");
    let second = async {
        later().await;
        check(&app, "example.net").await
    };
    let ((s1, _), (s2, _), ()) = tokio::join!(
        first,
        second,
        release_after_arrivals(&h.entered, &h.release)
    );
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::TOO_MANY_REQUESTS);
    h.state.runs.drain().await;

    // A refusal that had spent a token would leave one: this run would be the third.
    h.release.notify_one();
    let (status, _) = check(&app, "example.net").await;
    assert_eq!(status, StatusCode::OK, "no throttle, budget intact");
    assert_eq!(h.runs.load(Ordering::SeqCst), 2);
}

/// C22: a hit still spends the per-IP limit: with 1/1 a client's second request is 429 `limit`.
#[tokio::test(flavor = "current_thread")]
async fn admission_slots_c22_a_hit_spends_the_per_ip_limit() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let h = harness(true, |c| {
        c.rate_limit.per_ip_per_minute = 1;
        c.rate_limit.per_ip_burst = 1;
    })
    .await;

    // Another address seeds the head and the held rendering.
    h.release.notify_one();
    assert_eq!(
        check(&router(&h.state, B), "example.com").await.0,
        StatusCode::OK
    );
    wait_for_head_and_rendering(&h.state, "example.com").await;

    let app = router(&h.state, A);
    assert_eq!(check(&app, "example.com").await.0, StatusCode::OK);
    assert_eq!(requests(&snap, "cache_hit"), 1);
    let (status, retry) = check(&app, "example.com").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(retry.is_some());
    assert_eq!(rejections(&snap, "limit"), 1);
}

/// C24: the four rejection series and `joined` render at zero before any request.
#[test]
fn admission_slots_c24_rejection_series_render_at_zero() {
    let recorder = netray_common::server::metrics_recorder(lens::metrics::HISTOGRAM_BUCKETS);
    let handle = recorder.handle();
    let _guard = metrics::set_default_local_recorder(&recorder);

    lens::metrics::init_zero_series();
    let out = handle.render();

    for line in [
        r#"lens_admission_rejections_total{reason="throttle"} 0"#,
        r#"lens_admission_rejections_total{reason="budget"} 0"#,
        r#"lens_admission_rejections_total{reason="limit"} 0"#,
        r#"lens_admission_rejections_total{reason="slots"} 0"#,
        r#"lens_check_requests_total{result="joined"} 0"#,
    ] {
        assert!(out.lines().any(|l| l == line), "missing {line} in:\n{out}");
    }
}

/// C30: `check_limits` refuses over the per-IP limit (`IpLimit`) and over the global limit
/// (`GlobalLimit`), each with a wait; `/api/check` counts both as `limit`.
#[tokio::test(flavor = "current_thread")]
async fn admission_slots_c30_check_limits_refuses_per_ip_and_global() {
    let per_ip = harness(false, |c| {
        c.rate_limit.per_ip_per_minute = 1;
        c.rate_limit.per_ip_burst = 1;
    })
    .await;
    assert!(check_limits(&per_ip.state, ip(A)).is_ok());
    match check_limits(&per_ip.state, ip(A)) {
        Err(r) => {
            assert_eq!(r.reason, Reason::IpLimit);
            assert!(r.retry_after_s > 0);
            assert_eq!(r.last_run_ago_s, None);
            assert!(!r.last_incomplete);
        }
        Ok(()) => panic!("the second call must be refused"),
    }

    let global = harness(false, |c| {
        c.rate_limit.global_per_minute = 1;
        c.rate_limit.global_burst = 1;
    })
    .await;
    assert!(check_limits(&global.state, ip(A)).is_ok());
    match check_limits(&global.state, ip(B)) {
        Err(r) => {
            assert_eq!(r.reason, Reason::GlobalLimit);
            assert!(r.retry_after_s > 0);
        }
        Ok(()) => panic!("the global limit must refuse"),
    }

    // Through the route, both count `limit`.
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let per_ip = harness(false, |c| {
        c.rate_limit.per_ip_per_minute = 1;
        c.rate_limit.per_ip_burst = 1;
    })
    .await;
    per_ip.release.notify_one();
    let app = router(&per_ip.state, A);
    assert_eq!(check(&app, "example.com").await.0, StatusCode::OK);
    assert_eq!(
        check(&app, "example.net").await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(rejections(&snap, "limit"), 1);

    let global = harness(false, |c| {
        c.rate_limit.global_per_minute = 1;
        c.rate_limit.global_burst = 1;
    })
    .await;
    global.release.notify_one();
    assert_eq!(
        check(&router(&global.state, A), "example.com").await.0,
        StatusCode::OK
    );
    assert_eq!(
        check(&router(&global.state, B), "example.net").await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(rejections(&snap, "limit"), 2);
}

/// C31: `admit` for another domain while the only slot is held is `Refused{Slots, 1, None, false}`.
#[tokio::test(flavor = "current_thread")]
async fn admission_slots_c31_admit_refuses_slots() {
    let h = harness(false, |c| c.admission.domain_run_slots = 1).await;

    let mut first = start_gated(&h, "example.com", A, Trigger::Visit).await;
    let r = refused(admit(&h.state, "example.net", ip(A), Trigger::Visit, false));
    assert_eq!(
        r,
        Refusal {
            reason: Reason::Slots,
            retry_after_s: 1,
            last_run_ago_s: None,
            last_incomplete: false
        }
    );
    h.release.notify_one();
    finish(&mut first).await;
}

/// C32: with the badge recompute limiter exhausted, a badge still joins a run in flight.
#[tokio::test(flavor = "current_thread")]
async fn admission_slots_c32_a_badge_joins_despite_the_recompute_limiter() {
    let h = harness(false, |_| {}).await;
    let key = "example.com".to_string();
    let mut exhausted = false;
    for _ in 0..1000 {
        if h.state.badge_recompute_limiter.check_key(&key).is_err() {
            exhausted = true;
            break;
        }
    }
    assert!(exhausted, "the recompute limiter must run out");

    let mut stream = start_gated(&h, "example.com", A, Trigger::Visit).await;
    let app = router(&h.state, B);
    let badge = async {
        later().await;
        badge_svg(&app, "example.com").await
    };
    let driver = async {
        let mut grade = None;
        while let Some(ev) = stream.next().await {
            if let RunEvent::Done(p) = ev {
                grade = Some(p.output.score.grade.clone());
            }
        }
        grade.expect("the run published")
    };
    let opener = async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        h.release.notify_one();
    };
    let (svg, grade, ()) = tokio::join!(badge, driver, opener);

    assert!(!svg.contains(">?</text>"), "badge must not show ?: {svg}");
    assert!(
        svg.contains(&format!(">{grade}<")),
        "badge must show {grade}: {svg}"
    );
    assert_eq!(h.runs.load(Ordering::SeqCst), 1, "one run executes");
}
