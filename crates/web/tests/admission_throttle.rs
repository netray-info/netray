//! The per-domain throttle (specs/features/v2-admission/spec.md, Phase 4, requirements 12, 16
//! and 20; criteria C1, C5, C8 to C13, C23, C25 to C27).
//!
//! At most one run per domain starts per 60 s, counted from run start on tokio's clock. The
//! timing tests run on a paused clock and no history store (the sqlx pool needs real time);
//! the tests that need a head run on the real clock. The modules answer the committed goldens;
//! the apex runs with the state of `tests/fixtures/lens.production.toml`, driven in-process. The
//! metrics are read from a per-test local recorder; a current-thread runtime keeps the handlers
//! on the recording thread.
//!
//! Copied from crates/lens/tests/admission_throttle.rs (spec v2-cutover, Phase 6); dropped as V1:
//! `POST /api/check`, and every DKIM-selector scenario (both C5 tests, C23, C26's run in flight
//! with selectors). Runs go through API v1's `POST /api/v1/domains/{domain}/runs`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

mod common;

use axum::Extension;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use common::{
    dns_golden_raw, email_golden, email_incomplete, facts_golden_raw, http_module, ip_golden,
    registry_with, slow, tls_golden,
};
use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use netray_engine::Module;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::admission::{Admit, ApexState, Reason, Refusal, RunStream, Trigger, admit};
use tower::ServiceExt;

const REJECTIONS: &str = "netray_admission_rejections_total";
const BADGES: &str = "netray_badge_requests_total";
const RUNS: &str = "netray_runs_total";
const SHORT_CACHE: &str = "public, max-age=300, s-maxage=300";

fn client() -> IpAddr {
    IpAddr::from([127, 0, 0, 1])
}

/// Production config on golden modules, limits wide open; `email` is the email module (an
/// incomplete one ends its run incomplete).
fn build(email: Box<dyn Module>, store: Option<Arc<HistoryStore>>) -> (Router, ApexState) {
    let mut config = common::production_config();
    assert!(config.cache_enabled, "production config enables the cache");
    config.limits.rate_limit_per_ip_per_minute = 1000;
    config.limits.rate_limit_per_ip_burst = 1000;
    config.limits.rate_limit_global_per_minute = 10000;
    config.limits.rate_limit_global_burst = 10000;

    let registry = registry_with(
        dns_golden_raw("prism.sse"),
        facts_golden_raw("prism.sse"),
        http_module(Some("spectra-inspect.json")),
        email,
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

fn default_app() -> (Router, ApexState) {
    build(email_golden("beacon.sse"), None)
}

/// API v1's fresh run of `domain` (JSON answer); the status and the `Retry-After` header.
async fn check(app: &Router, domain: &str) -> (StatusCode, Option<String>) {
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/domains/{domain}/runs"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let retry = resp
        .headers()
        .get(header::RETRY_AFTER)
        .map(|v| v.to_str().unwrap().to_string());
    to_bytes(resp.into_body(), 16 * 1024 * 1024).await.unwrap();
    (status, retry)
}

/// `GET /badge/<domain>.svg`; the SVG and the `Cache-Control` header.
async fn badge(app: &Router, domain: &str) -> (String, String) {
    let req = Request::builder()
        .uri(format!("/badge/{domain}.svg"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cc = resp
        .headers()
        .get(header::CACHE_CONTROL)
        .expect("cache-control present")
        .to_str()
        .unwrap()
        .to_string();
    let bytes = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    (String::from_utf8(bytes.to_vec()).unwrap(), cc)
}

/// Reads the stream to its end.
async fn finish(mut stream: RunStream) {
    while stream.next().await.is_some() {}
}

fn admitted(a: Admit) -> RunStream {
    match a {
        Admit::Admitted(s) => s,
        Admit::Refused(r) => panic!("expected a run, got a refusal: {r:?}"),
        Admit::NotStarted => panic!("expected a run, got NotStarted"),
    }
}

fn refusal(a: Admit) -> Refusal {
    match a {
        Admit::Refused(r) => r,
        Admit::Admitted(_) => panic!("expected a refusal, got a run"),
        Admit::NotStarted => panic!("expected a refusal, got NotStarted"),
    }
}

fn throttle(retry_after_s: u64, ago: Option<u64>, last_incomplete: bool) -> Refusal {
    Refusal {
        reason: Reason::Throttle,
        retry_after_s,
        last_run_ago_s: ago,
        last_incomplete,
    }
}

fn rejections(s: &Acc, reason: &str) -> u64 {
    counter(s, REJECTIONS, "reason", reason)
}

/// `netray_runs_total{trigger}` summed over `outcome`.
fn runs(s: &Acc, trigger: &str) -> u64 {
    counter(s, RUNS, "trigger", trigger)
}

/// `netray_runs_total` over every trigger and outcome.
fn runs_total(s: &Acc) -> u64 {
    s.series()
        .into_iter()
        .filter(|t| t.name == RUNS)
        .map(|t| t.counter)
        .sum()
}

fn counter(s: &Acc, name: &str, label: &str, value: &str) -> u64 {
    s.series()
        .into_iter()
        .filter(|t| t.name == name && t.labels.iter().any(|(k, v)| k == label && v == value))
        .map(|t| t.counter)
        .sum()
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
                findings: vec![],
                evidence: vec![],
            },
            RunCheck {
                id: CheckId::parse("tls.caa_compliant").unwrap(),
                status: Status::Pass,
                findings: vec![],
                evidence: vec![],
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

/// Records a complete run graded `grade` as the head of `domain`, last seen `age` s ago.
async fn seed_head(store: &HistoryStore, state: &ApexState, domain: &str, grade: Grade, age: i64) {
    let run = stored_run(grade, state.scoring_profile.version);
    store
        .record(domain, &run, &[], now_unix() - age, |p, c| {
            netray_model::diff::diff(p, c, &[], None)
        })
        .await
        .unwrap();
}

async fn temp_store() -> (Arc<HistoryStore>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    (Arc::new(store), dir)
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

// Local recorders are thread-local: a current-thread runtime keeps the apex on this thread.

#[tokio::test(start_paused = true)]
async fn admission_throttle_c1_one_run_per_domain_per_minute_with_the_refusal_fields() {
    let (_app, state) = default_app();

    let first = admitted(admit(
        &state,
        "example.com",
        client(),
        Trigger::Visit,
        false,
    ));
    // The run has ended and left the map, so nothing can be joined; only the minute holds.
    finish(first).await;
    tokio::time::advance(Duration::from_secs(10)).await;

    assert_eq!(
        refusal(admit(&state, "example.com", client(), Trigger::Api, false)),
        throttle(50, Some(10), false)
    );
    // Another domain has its own minute.
    finish(admitted(admit(
        &state,
        "example.net",
        client(),
        Trigger::Visit,
        false,
    )))
    .await;
}

#[tokio::test(start_paused = true)]
async fn admission_throttle_c8_a_run_ended_incomplete_10s_ago_answers_429_with_50() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let (app, _state) = build(email_incomplete(), None);

    assert_eq!(check(&app, "example.com").await.0, StatusCode::OK);
    tokio::time::advance(Duration::from_secs(10)).await;

    assert_eq!(
        check(&app, "example.com").await,
        (StatusCode::TOO_MANY_REQUESTS, Some("50".to_string()))
    );
    assert_eq!(rejections(&snap, "throttle"), 1);
    // The refused request starts no run: only the first one counts.
    assert_eq!(runs(&snap, "api"), 1);
    assert_eq!(runs_total(&snap), 1);
}

#[tokio::test(start_paused = true)]
async fn admission_throttle_c9_at_60s_after_the_start_a_run_starts() {
    let (app, _state) = build(email_incomplete(), None);

    assert_eq!(check(&app, "example.com").await.0, StatusCode::OK);
    tokio::time::advance(Duration::from_secs(10)).await;
    assert_eq!(
        check(&app, "example.com").await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    tokio::time::advance(Duration::from_secs(50)).await;

    assert_eq!(check(&app, "example.com").await.0, StatusCode::OK);
}

#[tokio::test(start_paused = true)]
async fn admission_throttle_c10_at_59s_after_the_start_it_is_still_429() {
    let (app, _state) = build(email_incomplete(), None);

    assert_eq!(check(&app, "example.com").await.0, StatusCode::OK);
    tokio::time::advance(Duration::from_secs(10)).await;
    tokio::time::advance(Duration::from_secs(49)).await;

    assert_eq!(
        check(&app, "example.com").await,
        (StatusCode::TOO_MANY_REQUESTS, Some("1".to_string()))
    );
}

#[tokio::test(start_paused = true)]
async fn admission_throttle_c11_a_run_started_by_the_badge_throttles_the_api() {
    let (app, _state) = default_app();

    let (svg, _) = badge(&app, "example.com").await;
    assert!(svg.contains("<svg"), "the badge run answered: {svg}");
    tokio::time::advance(Duration::from_secs(10)).await;

    assert_eq!(
        check(&app, "example.com").await,
        (StatusCode::TOO_MANY_REQUESTS, Some("50".to_string()))
    );
}

/// A throttled domain (its minute started by API v1's run) asked for a badge: the SVG, its
/// `Cache-Control` and the count of `netray_badge_requests_total{cache="throttled"}`.
async fn throttled_badge(head_grade: Option<Grade>) -> (String, String, u64) {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let (store, _dir) = temp_store().await;
    let (app, state) = build(email_incomplete(), Some(store.clone()));

    // The incomplete run starts the minute and leaves no head of its own.
    assert_eq!(check(&app, "example.com").await.0, StatusCode::OK);
    // The run task records after the response: wait until it has ended, so the badge meets the
    // throttle and not the run in flight (which it would join).
    state.runs.drain().await;
    if let Some(grade) = head_grade {
        seed_head(&store, &state, "example.com", grade, 400).await;
    }

    let (svg, cc) = badge(&app, "example.com").await;
    (svg, cc, counter(&snap, BADGES, "cache", "throttled"))
}

#[tokio::test(flavor = "current_thread")]
async fn admission_throttle_c12_a_throttled_badge_shows_the_head_short_or_a_question_mark() {
    let (svg, cc, throttled) = throttled_badge(Some(Grade::C)).await;
    assert!(svg.contains(">C<"), "the head's grade, got: {svg}");
    assert_eq!(cc, SHORT_CACHE);
    assert_eq!(throttled, 1);

    let (svg, cc, throttled) = throttled_badge(None).await;
    assert!(svg.contains(">?</text>"), "no head: ?, got: {svg}");
    assert_eq!(cc, SHORT_CACHE);
    assert_eq!(throttled, 1);
}

#[tokio::test(start_paused = true)]
async fn admission_throttle_c13_another_domain_has_its_own_minute() {
    let (app, _state) = build(email_incomplete(), None);

    assert_eq!(check(&app, "example.com").await.0, StatusCode::OK);
    tokio::time::advance(Duration::from_secs(10)).await;

    assert_eq!(check(&app, "example.net").await.0, StatusCode::OK);
    assert_eq!(
        check(&app, "example.com").await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test(start_paused = true)]
async fn admission_throttle_c25_a_run_that_ended_incomplete_40s_ago_is_reported() {
    // Email answers after 10 s of clock and then incomplete: the run ends 10 s after its start.
    let email = slow(email_incomplete(), Some(Duration::from_secs(10)));
    let (_app, state) = build(email, None);

    finish(admitted(admit(
        &state,
        "example.com",
        client(),
        Trigger::Visit,
        false,
    )))
    .await;
    tokio::time::advance(Duration::from_secs(40)).await;

    assert_eq!(
        refusal(admit(
            &state,
            "example.com",
            client(),
            Trigger::Recheck,
            false
        )),
        throttle(10, Some(50), true)
    );
}

#[tokio::test(start_paused = true)]
async fn admission_throttle_c26_complete_run_is_not_incomplete() {
    // A run that ended complete.
    let (_app, state) = default_app();
    finish(admitted(admit(
        &state,
        "example.com",
        client(),
        Trigger::Visit,
        false,
    )))
    .await;
    tokio::time::advance(Duration::from_secs(10)).await;
    assert_eq!(
        refusal(admit(
            &state,
            "example.com",
            client(),
            Trigger::Recheck,
            false
        )),
        throttle(50, Some(10), false)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn admission_throttle_c27_the_throttle_never_reads_the_cache_window() {
    let (store, _dir) = temp_store().await;
    let (_app, state) = build(email_golden("beacon.sse"), Some(store.clone()));
    seed_head(&store, &state, "example.com", Grade::B, 10).await;
    seed_head(&store, &state, "example.net", Grade::B, 10).await;

    // Heads seen 10 s ago, no throttle record: a run starts, whatever the trigger.
    let recheck = admitted(admit(
        &state,
        "example.com",
        client(),
        Trigger::Recheck,
        false,
    ));
    assert!(!recheck.joined());
    let visit = admitted(admit(
        &state,
        "example.net",
        client(),
        Trigger::Visit,
        false,
    ));
    assert!(!visit.joined());

    finish(recheck).await;
    finish(visit).await;
}
