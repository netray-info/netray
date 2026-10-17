//! A prefetch starts nothing (specs/features/v2-admission/spec.md, Phase 5, requirements 18
//! and 20; criteria C1, C3 to C13).
//!
//! A request whose `Sec-Purpose` names `prefetch` or `prerender` joins a run in flight or is
//! answered without one: `/api/check` 503 `PREFETCH_REFUSED`, badge and OG the head or `?`.
//! lens runs with `tests/fixtures/lens.production.toml` on the committed goldens; the DNS
//! module counts the runs and can be gated. Metrics are read from a per-test local recorder,
//! so those tests run on a current-thread runtime.

use std::cell::RefCell;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, UNIX_EPOCH};

mod common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode, header};
use common::{
    dns_golden_raw, email_golden, facts_golden_raw, gated, http_module, ip_golden, registry_with,
    tls_golden,
};
use lens::admission::{Admit, RunStream, Trigger, admit};
use lens::config::Config;
use lens::routes::{api_router, badge_router, og_router};
use lens::state::AppState;
use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use tokio::sync::Notify;
use tower::ServiceExt;

const SHORT_CACHE: &str = "public, max-age=300, s-maxage=300";

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
    app: Router,
    runs: Arc<AtomicUsize>,
    _dir: Option<tempfile::TempDir>,
}

impl Harness {
    fn runs(&self) -> usize {
        self.runs.load(Ordering::SeqCst)
    }

    fn store(&self) -> &HistoryStore {
        self.state.history_store.as_ref().expect("store wired")
    }
}

async fn harness(
    budget: u32,
    gate: Option<(Arc<Notify>, Arc<Notify>)>,
    with_store: bool,
) -> Harness {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    assert!(config.cache.enabled, "production config enables the cache");
    config.rate_limit.per_ip_per_minute = 1000;
    config.rate_limit.per_ip_burst = 1000;
    config.rate_limit.global_per_minute = 10000;
    config.rate_limit.global_burst = 10000;
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
    let (api, _) = api_router().split_for_parts();
    let (badge, _) = badge_router().split_for_parts();
    let app = Router::new()
        .merge(api.with_state(state.clone()))
        .merge(badge.with_state(state.clone()))
        .merge(og_router().with_state(state.clone()))
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
    Harness {
        state,
        app,
        runs,
        _dir: dir,
    }
}

struct Answer {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl Answer {
    fn header(&self, name: &str) -> String {
        self.headers
            .get(name)
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default()
    }
}

async fn get(app: &Router, uri: String, purpose: Option<&str>) -> Answer {
    let mut req = Request::builder().uri(uri);
    if let Some(p) = purpose {
        req = req.header("sec-purpose", p);
    }
    send(app, req.body(Body::empty()).unwrap()).await
}

async fn check(app: &Router, domain: &str, purpose: Option<&str>) -> Answer {
    get(app, format!("/api/check/{domain}?stream=false"), purpose).await
}

async fn send(app: &Router, req: Request<Body>) -> Answer {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = to_bytes(resp.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap()
        .to_vec();
    Answer {
        status,
        headers,
        body,
    }
}

async fn badge(app: &Router, domain: &str, purpose: Option<&str>) -> (Answer, String) {
    let a = get(app, format!("/badge/{domain}.svg"), purpose).await;
    let svg = String::from_utf8(a.body.clone()).unwrap();
    (a, svg)
}

fn assert_prefetch_refused(a: &Answer) {
    assert_eq!(a.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(a.header("cache-control"), "no-store");
    let json: serde_json::Value = serde_json::from_slice(&a.body).expect("JSON error body");
    assert_eq!(json["error"]["code"], "PREFETCH_REFUSED");
}

fn stored_run(grade: Grade, profile_version: u32) -> Run {
    Run {
        profile_version,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![
            RunCheck {
                id: CheckId::parse("dns.dnssec_valid").unwrap(),
                status: Status::Pass,
            },
            RunCheck {
                id: CheckId::parse("tls.caa_compliant").unwrap(),
                status: Status::Pass,
            },
        ],
        blocks: vec![],
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Records a complete run graded `grade` as the head of `domain`, last seen `age` s ago;
/// returns the run and its last-seen time.
async fn seed_head(h: &Harness, domain: &str, grade: Grade, age: i64) -> (Run, i64) {
    let run = stored_run(grade, h.state.scoring_profile.version);
    let at = now_unix() - age;
    h.store()
        .record(domain, &run, &[], at, |p, c| {
            netray_model::diff::diff(p, c, &[], None)
        })
        .await
        .unwrap();
    (run, at)
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

async fn finish(mut stream: RunStream) {
    while stream.next().await.is_some() {}
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

#[tokio::test(flavor = "multi_thread")]
async fn admission_prefetch_c1_no_api_badge_or_og_prefetch_starts_a_run() {
    let h = harness(100, None, true).await;

    let api = check(&h.app, "example.com", Some("prefetch")).await;
    assert_prefetch_refused(&api);

    let (b, svg) = badge(&h.app, "example.net", Some("prerender")).await;
    assert_eq!(b.status, StatusCode::OK);
    assert!(svg.contains(">?</text>"), "no entry: ?, got: {svg}");

    let og = get(
        &h.app,
        "/og/example.org.png".to_string(),
        Some("prefetch;prerender"),
    )
    .await;
    assert_eq!(og.status, StatusCode::OK);
    assert_eq!(og.header("content-type"), "image/png");

    assert_eq!(h.runs(), 0, "a prefetch starts no run");
}

#[tokio::test(flavor = "current_thread")]
async fn admission_prefetch_c3_admit_with_prefetch_answers_not_started() {
    let h = harness(100, None, false).await;

    let answer = admit(&h.state, "example.com", client(), Trigger::Visit, true);

    assert!(matches!(answer, Admit::NotStarted), "expected NotStarted");
    assert_eq!(h.runs(), 0);
}

/// C4, C5 and C6 are rows of one table: the header value, then the same answer.
#[tokio::test(flavor = "multi_thread")]
async fn admission_prefetch_c4_c5_c6_a_prefetch_header_is_refused_and_starts_no_minute() {
    for value in ["prefetch", "prefetch;prerender", "PREFETCH"] {
        let h = harness(100, None, false).await;

        let refused = check(&h.app, "example.com", Some(value)).await;
        assert_prefetch_refused(&refused);
        assert_eq!(h.runs(), 0, "{value}: no run executed");

        // No minute was started, so a plain request is not throttled.
        let plain = check(&h.app, "example.com", None).await;
        assert_eq!(plain.status, StatusCode::OK, "{value}: plain request runs");
        assert_eq!(h.runs(), 1, "{value}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn admission_prefetch_c7_a_fresh_head_with_its_rendering_held_is_a_hit() {
    let h = harness(100, None, true).await;
    assert_eq!(
        check(&h.app, "example.com", None).await.status,
        StatusCode::OK
    );
    wait_for_head_and_rendering(&h.state, "example.com").await;

    let hit = check(&h.app, "example.com", Some("prefetch")).await;

    assert_eq!(hit.status, StatusCode::OK);
    assert_eq!(hit.header("x-cache"), "HIT");
    assert_eq!(h.runs(), 1, "the hit starts no run");
}

#[tokio::test(flavor = "current_thread")]
async fn admission_prefetch_c8_a_prerender_request_joins_the_run_in_flight() {
    let recorder = DebuggingRecorder::new();
    let acc = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let h = harness(100, Some((entered.clone(), release.clone())), false).await;

    let plain = check(&h.app, "example.com", None);
    let joiner = async {
        entered.notified().await;
        check(&h.app, "example.com", Some("prerender")).await
    };
    let opener = async {
        // Let the joiner reach the run in flight, then open the gate.
        tokio::time::sleep(Duration::from_millis(300)).await;
        release.notify_one();
    };
    let (plain, joined, ()) = tokio::join!(plain, joiner, opener);

    assert_eq!(plain.status, StatusCode::OK);
    assert_eq!(joined.status, StatusCode::OK, "the prefetch joined the run");
    assert_eq!(h.runs(), 1, "one run served both");
    assert_eq!(
        acc.counter("lens_check_requests_total", "result", "joined"),
        1
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn admission_prefetch_c9_a_prefetched_badge_shows_the_stale_head_short_or_a_question_mark() {
    let h = harness(100, None, true).await;
    seed_head(&h, "example.com", Grade::A, 400).await;

    let (a, svg) = badge(&h.app, "example.com", Some("prefetch")).await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(svg.contains(">A<"), "the head's grade, got: {svg}");
    assert_eq!(a.header("cache-control"), SHORT_CACHE);
    assert_eq!(h.runs(), 0, "the badge started no run");

    let (a, svg) = badge(&h.app, "example.net", Some("prefetch")).await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(svg.contains(">?</text>"), "no entry: ?, got: {svg}");
    assert_eq!(a.header("cache-control"), SHORT_CACHE);
    assert_eq!(h.runs(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn admission_prefetch_c10_a_prefetched_og_card_renders_the_stale_entry_short() {
    let h = harness(100, None, true).await;
    let (run, seen) = seed_head(&h, "example.com", Grade::A, 400).await;
    let pct = netray_engine::history::score_run(&run, &h.state.scoring_profile).overall_percentage;
    let svg = lens::og::render::svg_for_grade(
        "example.com",
        "A",
        lens::og::layout::DEFAULT_LABEL,
        pct,
        UNIX_EPOCH + Duration::from_secs(seen as u64),
    );
    let expected = lens::og::render::svg_to_png(&svg, h.state.font_db.clone()).unwrap();

    let og = get(&h.app, "/og/example.com.png".to_string(), Some("prefetch")).await;

    assert_eq!(og.status, StatusCode::OK);
    assert_eq!(og.header("cache-control"), SHORT_CACHE);
    assert!(
        og.body == expected,
        "PNG differs from the stale entry's card"
    );
    assert_eq!(h.runs(), 0, "the card started no run");
}

#[tokio::test(flavor = "multi_thread")]
async fn admission_prefetch_c11_none_and_no_header_start_a_run() {
    let h = harness(100, None, false).await;

    let none = check(&h.app, "example.com", Some("none")).await;
    let absent = check(&h.app, "example.net", None).await;

    assert_eq!(none.status, StatusCode::OK);
    assert_eq!(absent.status, StatusCode::OK);
    assert_eq!(h.runs(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn admission_prefetch_c12_a_refused_prefetch_spends_no_budget_token() {
    let h = harness(1, None, false).await;

    let refused = check(&h.app, "example.com", Some("prefetch")).await;
    assert_prefetch_refused(&refused);

    let normal = check(&h.app, "example.com", None).await;
    assert_eq!(normal.status, StatusCode::OK, "the token is intact");
    assert_eq!(h.runs(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn admission_prefetch_c13_admit_joins_or_answers_not_started_never_refuses() {
    // No run: NotStarted, and no throttle record, so a normal admit starts a run.
    let h = harness(100, None, false).await;
    let first = admit(&h.state, "example.com", client(), Trigger::Recheck, true);
    assert!(matches!(first, Admit::NotStarted), "no run: NotStarted");
    match admit(&h.state, "example.com", client(), Trigger::Recheck, false) {
        Admit::Admitted(s) => {
            assert!(!s.joined());
            finish(s).await;
        }
        _ => panic!("a normal admit starts a run: no throttle record was written"),
    }

    // The run ended less than 60 s ago: the domain is throttled, still NotStarted.
    let throttled = admit(&h.state, "example.com", client(), Trigger::Recheck, true);
    assert!(
        matches!(throttled, Admit::NotStarted),
        "a throttled domain: NotStarted, never Refused"
    );

    // A run in flight: the prefetch joins it.
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let g = harness(100, Some((entered, release.clone())), false).await;
    let Admit::Admitted(running) = admit(&g.state, "example.net", client(), Trigger::Visit, false)
    else {
        panic!("the first admit starts a run");
    };
    match admit(&g.state, "example.net", client(), Trigger::Recheck, true) {
        Admit::Admitted(s) => {
            assert!(s.joined(), "the prefetch joined the run in flight");
            release.notify_one();
            finish(s).await;
        }
        _ => panic!("a run in flight: Admitted"),
    }
    finish(running).await;
    assert_eq!(g.runs(), 1);
}
