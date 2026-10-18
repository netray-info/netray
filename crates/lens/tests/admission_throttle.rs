//! The per-domain throttle (specs/features/v2-admission/spec.md, Phase 4, requirements 12, 16
//! and 20; criteria C1, C5, C8 to C13, C23, C25 to C27).
//!
//! At most one run per domain starts per 60 s, counted from run start on tokio's clock. The
//! timing tests run on a paused clock and no history store (the sqlx pool needs real time);
//! the tests that need a head run on the real clock. The modules answer the committed goldens;
//! lens runs with `tests/fixtures/lens.production.toml`, driven in-process. The metrics are
//! read from a per-test local recorder; a current-thread runtime keeps the handlers on the
//! recording thread.

use std::cell::RefCell;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

mod common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
use common::{
    dns_golden_raw, email_golden, email_incomplete, facts_golden_raw, gated, http_module,
    ip_golden, registry_with, slow, tls_golden,
};
use lens::admission::{Admit, Reason, Refusal, RunStream, Trigger, admit};
use lens::config::Config;
use lens::routes::{api_router, badge_router};
use lens::state::AppState;
use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use netray_engine::Module;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use tokio::sync::Notify;
use tower::ServiceExt;

const REJECTIONS: &str = "netray_admission_rejections_total";
const BADGES: &str = "netray_badge_requests_total";
const RUNS: &str = "netray_runs_total";
const SHORT_CACHE: &str = "public, max-age=300, s-maxage=300";

fn client() -> IpAddr {
    IpAddr::from([127, 0, 0, 1])
}

/// Production config on golden modules, limits wide open; `email` is the email module (an
/// incomplete one ends its run incomplete), `dns_gate` holds the DNS module open.
fn build(
    email: Box<dyn Module>,
    dns_gate: Option<(Arc<Notify>, Arc<Notify>)>,
    store: Option<Arc<HistoryStore>>,
) -> (Router, AppState) {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    assert!(config.cache.enabled, "production config enables the cache");
    config.rate_limit.per_ip_per_minute = 1000;
    config.rate_limit.per_ip_burst = 1000;
    config.rate_limit.global_per_minute = 10000;
    config.rate_limit.global_burst = 10000;

    let dns = match dns_gate {
        Some((entered, release)) => gated(dns_golden_raw("prism.sse"), entered, release),
        None => dns_golden_raw("prism.sse"),
    };
    let registry = registry_with(
        dns,
        facts_golden_raw("prism.sse"),
        http_module(Some("spectra-inspect.json")),
        email,
        ip_golden("ifconfig-json.json"),
        tls_golden("tlsight-inspect.json"),
    );
    let mut state = AppState::with_registry(config, registry).unwrap();
    state.history_store = store;
    let (api, _) = api_router().split_for_parts();
    let (badge, _) = badge_router().split_for_parts();
    let router = Router::new()
        .merge(api.with_state(state.clone()))
        .merge(badge.with_state(state.clone()))
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
    (router, state)
}

fn default_app() -> (Router, AppState) {
    build(email_golden("beacon.sse"), None, None)
}

/// `POST /api/check`, no stream; the status and the `Retry-After` header.
async fn check(app: &Router, domain: &str, dkim: Option<&str>) -> (StatusCode, Option<String>) {
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
    let retry = resp
        .headers()
        .get(header::RETRY_AFTER)
        .map(|v| v.to_str().unwrap().to_string());
    to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
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

/// Records a complete run graded `grade` as the head of `domain`, last seen `age` s ago.
async fn seed_head(store: &HistoryStore, state: &AppState, domain: &str, grade: Grade, age: i64) {
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

// Local recorders are thread-local: a current-thread runtime keeps lens on this thread.

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
async fn admission_throttle_c5_dkim_request_does_not_join_a_run_in_flight() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let (app, _state) = build(
        email_golden("beacon.sse"),
        Some((entered.clone(), release.clone())),
        None,
    );

    let plain = check(&app, "example.com", None);
    let observe = async {
        entered.notified().await;
        // A dkim run never joins; it needs the throttle minute the plain run started.
        let dkim = check(&app, "example.com", Some("x")).await;
        release.notify_one();
        dkim
    };
    let (plain, dkim) = tokio::join!(plain, observe);

    assert_eq!(plain.0, StatusCode::OK);
    assert_eq!(
        dkim,
        (StatusCode::TOO_MANY_REQUESTS, Some("60".to_string()))
    );
    assert_eq!(rejections(&snap, "throttle"), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn admission_throttle_c5_dkim_request_is_never_a_hit_and_is_bound_by_the_minute() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let (store, _dir) = temp_store().await;
    let (app, state) = build(email_golden("beacon.sse"), None, Some(store));

    assert_eq!(check(&app, "example.com", None).await.0, StatusCode::OK);
    wait_for_head_and_rendering(&state, "example.com").await;

    // A plain repeat would be a hit; the dkim request is none, and the minute binds it.
    let (status, retry) = check(&app, "example.com", Some("x")).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(retry.is_some(), "429 carries Retry-After");
    // The refused dkim request starts no run: only the first one counts.
    assert_eq!(runs_total(&snap), 1);
    assert_eq!(rejections(&snap, "throttle"), 1);
}

#[tokio::test(start_paused = true)]
async fn admission_throttle_c8_a_run_ended_incomplete_10s_ago_answers_429_with_50() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let (app, _state) = build(email_incomplete(), None, None);

    assert_eq!(check(&app, "example.com", None).await.0, StatusCode::OK);
    tokio::time::advance(Duration::from_secs(10)).await;

    assert_eq!(
        check(&app, "example.com", None).await,
        (StatusCode::TOO_MANY_REQUESTS, Some("50".to_string()))
    );
    assert_eq!(rejections(&snap, "throttle"), 1);
    // The refused request starts no run: only the first one counts.
    assert_eq!(runs(&snap, "api"), 1);
    assert_eq!(runs_total(&snap), 1);
}

#[tokio::test(start_paused = true)]
async fn admission_throttle_c9_at_60s_after_the_start_a_run_starts() {
    let (app, _state) = build(email_incomplete(), None, None);

    assert_eq!(check(&app, "example.com", None).await.0, StatusCode::OK);
    tokio::time::advance(Duration::from_secs(10)).await;
    assert_eq!(
        check(&app, "example.com", None).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    tokio::time::advance(Duration::from_secs(50)).await;

    assert_eq!(check(&app, "example.com", None).await.0, StatusCode::OK);
}

#[tokio::test(start_paused = true)]
async fn admission_throttle_c10_at_59s_after_the_start_it_is_still_429() {
    let (app, _state) = build(email_incomplete(), None, None);

    assert_eq!(check(&app, "example.com", None).await.0, StatusCode::OK);
    tokio::time::advance(Duration::from_secs(10)).await;
    tokio::time::advance(Duration::from_secs(49)).await;

    assert_eq!(
        check(&app, "example.com", None).await,
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
        check(&app, "example.com", None).await,
        (StatusCode::TOO_MANY_REQUESTS, Some("50".to_string()))
    );
}

/// A throttled domain (its minute started by `/api/check`) asked for a badge: the SVG, its
/// `Cache-Control` and the count of `netray_badge_requests_total{cache="throttled"}`.
async fn throttled_badge(head_grade: Option<Grade>) -> (String, String, u64) {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let (store, _dir) = temp_store().await;
    let (app, state) = build(email_incomplete(), None, Some(store.clone()));

    // The incomplete run starts the minute and leaves no head of its own.
    assert_eq!(check(&app, "example.com", None).await.0, StatusCode::OK);
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
    let (app, _state) = build(email_incomplete(), None, None);

    assert_eq!(check(&app, "example.com", None).await.0, StatusCode::OK);
    tokio::time::advance(Duration::from_secs(10)).await;

    assert_eq!(check(&app, "example.net", None).await.0, StatusCode::OK);
    assert_eq!(
        check(&app, "example.com", None).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test(start_paused = true)]
async fn admission_throttle_c23_a_dkim_run_binds_dkim_and_plain_requests() {
    let (app, _state) = default_app();

    assert_eq!(
        check(&app, "example.com", Some("x")).await.0,
        StatusCode::OK
    );
    tokio::time::advance(Duration::from_secs(10)).await;

    assert_eq!(
        check(&app, "example.com", Some("x")).await,
        (StatusCode::TOO_MANY_REQUESTS, Some("50".to_string()))
    );
    assert_eq!(
        check(&app, "example.com", None).await,
        (StatusCode::TOO_MANY_REQUESTS, Some("50".to_string()))
    );
}

#[tokio::test(start_paused = true)]
async fn admission_throttle_c25_a_run_that_ended_incomplete_40s_ago_is_reported() {
    // Email answers after 10 s of clock and then incomplete: the run ends 10 s after its start.
    let email = slow(email_incomplete(), Some(Duration::from_secs(10)));
    let (_app, state) = build(email, None, None);

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
async fn admission_throttle_c26_complete_run_and_dkim_in_flight_are_not_incomplete() {
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

    // A dkim run in flight.
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let (app, state) = build(
        email_golden("beacon.sse"),
        Some((entered.clone(), release.clone())),
        None,
    );
    let dkim = check(&app, "example.com", Some("x"));
    let observe = async {
        entered.notified().await;
        let refused = admit(&state, "example.com", client(), Trigger::Visit, false);
        release.notify_one();
        refusal(refused)
    };
    let (dkim, refused) = tokio::join!(dkim, observe);

    assert_eq!(dkim.0, StatusCode::OK);
    assert_eq!(refused.reason, Reason::Throttle);
    assert!(!refused.last_incomplete, "in flight is not incomplete");
}

#[tokio::test(flavor = "current_thread")]
async fn admission_throttle_c27_the_throttle_never_reads_the_cache_window() {
    let (store, _dir) = temp_store().await;
    let (_app, state) = build(email_golden("beacon.sse"), None, Some(store.clone()));
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
