//! The `netray_` metrics of the apex application (specs/features/v2-cutover/spec.md, requirements
//! 18 and 19; SDD v2 section 9.2; Phase 4 criteria C3, C4, C7, C9 to C12).
//!
//! Until Phase 6 the apex application is lens's, so the series are read from lens driven
//! in-process on the committed goldens, with a temp-file history store. The metrics come from a
//! per-test local recorder; a current-thread runtime keeps every handler and spawned task on the
//! recording thread. `Snapshotter::snapshot()` zeroes on read, so `Acc` accumulates.

use std::cell::RefCell;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, UNIX_EPOCH};

mod common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
use common::{
    dns_golden_raw, email_golden, facts_golden_raw, facts_with_ips, gated, http_module, ip_golden,
    pass_throttle, registry_with, tls_golden,
};
use lens::admission::{Admit, Reason, RunStream, Trigger, admit, check_limits};
use lens::config::Config;
use lens::state::AppState;
use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use tokio::sync::Notify;
use tower::ServiceExt;

const CLIENT: [u8; 4] = [127, 0, 0, 1];

fn client() -> IpAddr {
    IpAddr::from(CLIENT)
}

/// The TLS golden module with `tls.caa_compliant` forced to Fail until `fixed` is set, then Pass.
struct FixableTls {
    inner: Box<dyn Module>,
    fixed: Arc<AtomicBool>,
}

impl Module for FixableTls {
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
        Box::pin(async move {
            let mut out = self.inner.run(ctx, facts).await;
            if let SectionOutcome::Measured { checks, .. } = &mut out {
                let caa = CheckId::parse("tls.caa_compliant").unwrap();
                for c in checks.iter_mut().filter(|c| c.id == caa) {
                    c.status = if self.fixed.load(Ordering::SeqCst) {
                        Status::Pass
                    } else {
                        Status::Fail
                    };
                }
            }
            out
        })
    }
}

struct World {
    state: AppState,
    store: Arc<HistoryStore>,
    _dir: tempfile::TempDir,
}

/// Production config on golden modules, limits wide open, a history store on a temp file.
/// `facts` is the resolve stage; `tls` the TLS module; `dns_gate` holds the DNS module open.
async fn world(
    tune: impl FnOnce(&mut Config),
    facts: Box<dyn netray_engine::FactsProvider>,
    tls: Box<dyn Module>,
    dns_gate: Option<(Arc<Notify>, Arc<Notify>)>,
) -> World {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    config.rate_limit.per_ip_per_minute = 1000;
    config.rate_limit.per_ip_burst = 1000;
    config.rate_limit.global_per_minute = 10000;
    config.rate_limit.global_burst = 10000;
    tune(&mut config);

    let dns = match dns_gate {
        Some((entered, release)) => gated(dns_golden_raw("prism.sse"), entered, release),
        None => dns_golden_raw("prism.sse"),
    };
    let registry = registry_with(
        dns,
        facts,
        http_module(Some("spectra-inspect.json")),
        email_golden("beacon.sse"),
        ip_golden("ifconfig-json.json"),
        tls,
    );
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    let mut state = AppState::with_registry(config, registry).unwrap();
    state.history_store = Some(store.clone());
    World {
        state,
        store,
        _dir: dir,
    }
}

async fn default_world() -> World {
    world(
        |_| {},
        facts_golden_raw("prism.sse"),
        tls_golden("tlsight-inspect.json"),
        None,
    )
    .await
}

fn app(state: &AppState) -> Router {
    lens::routes(state.clone()).layer(MockConnectInfo(SocketAddr::from((CLIENT, 40000))))
}

async fn get(app: &Router, uri: &str) -> StatusCode {
    let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    to_bytes(resp.into_body(), 8 * 1024 * 1024).await.unwrap();
    status
}

async fn post_check(app: &Router, domain: &str) -> StatusCode {
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
    to_bytes(resp.into_body(), 8 * 1024 * 1024).await.unwrap();
    status
}

/// Reads the stream to its end, which includes the recording.
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

/// Poll (bounded, 5 s) until the domain has a head entry and its rendering is held.
async fn wait_for_head(w: &World, domain: &str) -> String {
    for _ in 0..250 {
        if let Some(head) = w.store.head(domain).await.unwrap()
            && w.state
                .cache
                .as_ref()
                .unwrap()
                .get(&head.id)
                .await
                .is_some()
        {
            return head.id;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the run of {domain} was not recorded and held within 5 s");
}

/// A series of the accumulator.
#[derive(Clone)]
struct Total {
    name: String,
    labels: Vec<(String, String)>,
    counter: u64,
    /// The last non-zero value a read returned (a read zeroes the gauge).
    gauge: Option<f64>,
    observations: usize,
}

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
                gauge: None,
                observations: 0,
            });
            match v {
                DebugValue::Counter(n) => t.counter += n,
                DebugValue::Gauge(g) => {
                    if g.into_inner() != 0.0 {
                        t.gauge = Some(g.into_inner());
                    }
                }
                DebugValue::Histogram(h) => t.observations += h.len(),
            }
        }
        total.values().cloned().collect()
    }

    /// Sum of the counters `name` whose labels include every pair in `want`.
    fn counter(&self, name: &str, want: &[(&str, &str)]) -> u64 {
        self.series()
            .into_iter()
            .filter(|t| t.name == name && has_labels(t, want))
            .map(|t| t.counter)
            .sum()
    }

    fn observations(&self, name: &str) -> usize {
        self.series()
            .into_iter()
            .filter(|t| t.name == name)
            .map(|t| t.observations)
            .sum()
    }

    fn gauge(&self, name: &str) -> Option<f64> {
        self.series()
            .into_iter()
            .find(|t| t.name == name)
            .and_then(|t| t.gauge)
    }
}

fn has_labels(t: &Total, want: &[(&str, &str)]) -> bool {
    want.iter()
        .all(|(k, v)| t.labels.iter().any(|(tk, tv)| tk == k && tv == v))
}

const RUNS: &str = "netray_runs_total";
const REJECTIONS: &str = "netray_admission_rejections_total";

// C9: a recheck that records a run with a predecessor and one fixed tls transition.
#[tokio::test(flavor = "current_thread")]
async fn netray_metrics_c9_recheck_with_a_fixed_tls_check_counts_run_predecessor_fix_and_interval()
{
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let fixed = Arc::new(AtomicBool::new(false));
    let tls: Box<dyn Module> = Box::new(FixableTls {
        inner: tls_golden("tlsight-inspect.json"),
        fixed: fixed.clone(),
    });
    let w = world(|_| {}, facts_golden_raw("prism.sse"), tls, None).await;

    finish(admitted(admit(
        &w.state,
        "example.com",
        client(),
        Trigger::Visit,
        false,
    )))
    .await;
    assert_eq!(
        snap.counter(RUNS, &[("trigger", "visit"), ("outcome", "complete")]),
        1
    );
    assert_eq!(snap.counter("netray_runs_with_predecessor_total", &[]), 0);

    pass_throttle().await;
    fixed.store(true, Ordering::SeqCst);
    finish(admitted(admit(
        &w.state,
        "example.com",
        client(),
        Trigger::Recheck,
        false,
    )))
    .await;

    assert_eq!(
        snap.counter(RUNS, &[("trigger", "recheck"), ("outcome", "complete")]),
        1
    );
    assert_eq!(snap.counter("netray_runs_with_predecessor_total", &[]), 1);
    assert_eq!(
        snap.counter("netray_fixes_observed_total", &[("section", "tls")]),
        1
    );
    assert_eq!(snap.observations("netray_run_interval_seconds"), 1);
}

// C10: each run trigger counts its own series.
#[tokio::test(flavor = "current_thread")]
async fn netray_metrics_c10_badge_og_and_visit_runs_count_their_trigger() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let w = default_world().await;
    let app = app(&w.state);

    assert_eq!(get(&app, "/badge/example.com.svg").await, StatusCode::OK);
    assert_eq!(snap.counter(RUNS, &[("trigger", "badge")]), 1);

    assert_eq!(get(&app, "/og/example.net.png").await, StatusCode::OK);
    assert_eq!(snap.counter(RUNS, &[("trigger", "og")]), 1);

    // V1's `/api/check` is an API client's run: it counts under `trigger="api"` (the visit
    // trigger is exercised through `admit(Trigger::Visit)` in C9).
    assert_eq!(post_check(&app, "example.org").await, StatusCode::OK);
    assert_eq!(snap.counter(RUNS, &[("trigger", "api")]), 1);
}

// C11: every refusal reason counts under `netray_admission_rejections_total{reason}`.
#[tokio::test(flavor = "current_thread")]
async fn netray_metrics_c11_each_refusal_reason_counts_its_series() {
    for reason in ["throttle", "budget", "limit", "slots"] {
        let recorder = DebuggingRecorder::new();
        let snap = Acc::new(recorder.snapshotter());
        let _guard = metrics::set_default_local_recorder(&recorder);
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());

        match reason {
            "throttle" => {
                let w = default_world().await;
                finish(admitted(admit(
                    &w.state,
                    "example.com",
                    client(),
                    Trigger::Visit,
                    false,
                )))
                .await;
                match admit(&w.state, "example.com", client(), Trigger::Visit, false) {
                    Admit::Refused(r) => assert_eq!(r.reason, Reason::Throttle),
                    _ => panic!("the second run within a minute must be refused"),
                }
            }
            "budget" => {
                let w = world(
                    |c| c.admission.client_budget_per_hour = 0,
                    facts_golden_raw("prism.sse"),
                    tls_golden("tlsight-inspect.json"),
                    None,
                )
                .await;
                match admit(&w.state, "example.com", client(), Trigger::Visit, false) {
                    Admit::Refused(r) => assert_eq!(r.reason, Reason::Budget),
                    _ => panic!("a budget of 0 must refuse"),
                }
            }
            "limit" => {
                let w = world(
                    |c| {
                        c.rate_limit.per_ip_per_minute = 1;
                        c.rate_limit.per_ip_burst = 1;
                    },
                    facts_golden_raw("prism.sse"),
                    tls_golden("tlsight-inspect.json"),
                    None,
                )
                .await;
                let app = app(&w.state);
                assert_eq!(post_check(&app, "example.com").await, StatusCode::OK);
                assert_eq!(
                    post_check(&app, "example.net").await,
                    StatusCode::TOO_MANY_REQUESTS
                );
                // The limiter alone, for the record of what `limit` means.
                assert!(check_limits(&w.state, client()).is_err());
            }
            _ => {
                let w = world(
                    |c| c.admission.domain_run_slots = 1,
                    facts_golden_raw("prism.sse"),
                    tls_golden("tlsight-inspect.json"),
                    Some((entered.clone(), release.clone())),
                )
                .await;
                let held = admitted(admit(
                    &w.state,
                    "example.com",
                    client(),
                    Trigger::Visit,
                    false,
                ));
                entered.notified().await;
                match admit(&w.state, "example.net", client(), Trigger::Visit, false) {
                    Admit::Refused(r) => assert_eq!(r.reason, Reason::Slots),
                    _ => panic!("no free slot must refuse"),
                }
                release.notify_one();
                finish(held).await;
            }
        }

        assert_eq!(
            snap.counter(REJECTIONS, &[("reason", reason)]),
            1,
            "reason {reason}"
        );
    }
}

fn stored_run(profile_version: u32) -> Run {
    Run {
        profile_version,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("dns.dnssec_valid").unwrap(),
            status: Status::Pass,
        }],
        blocks: vec![],
    }
}

// C12: the history gauges are set at startup and again after a sweep.
#[tokio::test(flavor = "current_thread")]
async fn netray_metrics_c12_history_gauges_are_set_at_startup_and_after_a_sweep() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);

    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let now = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    {
        let store = HistoryStore::open(&db).await.unwrap();
        store.migrate().await.unwrap();
        let run = stored_run(
            AppState::new(Config::load(None).unwrap())
                .unwrap()
                .scoring_profile
                .version,
        );
        // One domain unseen for 100 days (the sweep drops it), one seen now.
        for (domain, seen) in [
            ("old.example.com", now - 100 * 86_400),
            ("new.example.com", now),
        ] {
            store
                .record(domain, &run, &[], seen, |p, c| {
                    netray_model::diff::diff(p, c, &[], None)
                })
                .await
                .unwrap();
        }
    }

    let mut config = Config::load(None).unwrap();
    config.history.enabled = true;
    config.history.db_path = db;
    config.snapshots.enabled = false;
    let registry = registry_with(
        dns_golden_raw("prism.sse"),
        facts_with_ips(&["192.0.2.1"]),
        http_module(Some("spectra-inspect.json")),
        email_golden("beacon.sse"),
        ip_golden("ifconfig-json.json"),
        tls_golden("tlsight-inspect.json"),
    );
    let _prepared = lens::prepare(&config, registry).await;

    assert_eq!(snap.gauge("netray_history_entries"), Some(2.0));
    assert_eq!(snap.gauge("netray_history_domains"), Some(2.0));

    // The hourly sweep fires on tokio's clock.
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(3601)).await;
    tokio::time::resume();
    let mut domains = snap.gauge("netray_history_domains");
    for _ in 0..250 {
        if domains == Some(1.0) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
        domains = snap.gauge("netray_history_domains");
    }
    assert_eq!(domains, Some(1.0), "the sweep dropped one domain");
    assert_eq!(snap.gauge("netray_history_entries"), Some(1.0));
}

const FORBIDDEN_LABEL_NAMES: [&str; 10] = [
    "domain", "host", "address", "ip", "client", "target", "url", "query", "name", "id",
];

// C3 and C7: one stub run of example.com at 192.0.2.1 leaks no domain, address or entry id into
// any series, and route labels are templates.
#[tokio::test(flavor = "current_thread")]
async fn netray_metrics_c3_c7_no_series_carries_the_domain_the_address_or_the_entry_id() {
    let recorder = DebuggingRecorder::new();
    let snap = Acc::new(recorder.snapshotter());
    let _guard = metrics::set_default_local_recorder(&recorder);
    let w = world(
        |_| {},
        facts_with_ips(&["192.0.2.1"]),
        tls_golden("tlsight-inspect.json"),
        None,
    )
    .await;
    let app = app(&w.state);

    assert_eq!(get(&app, "/example.com").await, StatusCode::OK);
    assert_eq!(post_check(&app, "example.com").await, StatusCode::OK);
    let id = wait_for_head(&w, "example.com").await;
    assert_eq!(get(&app, "/badge/example.com.svg").await, StatusCode::OK);
    assert_eq!(get(&app, "/og/example.com.png").await, StatusCode::OK);
    get(&app, &format!("/r/{id}")).await;

    let series = snap.series();
    assert!(
        series.iter().any(|t| t.name.starts_with("netray_")),
        "no netray_ series exists"
    );
    for t in &series {
        for (k, v) in &t.labels {
            assert!(
                !FORBIDDEN_LABEL_NAMES.contains(&k.as_str()),
                "{} carries the label name {k}",
                t.name
            );
            assert!(
                v != "example.com" && v != "192.0.2.1" && *v != id,
                "{} carries the label value {v}",
                t.name
            );
            if k == "endpoint" {
                assert!(
                    v.starts_with('/') && !v.contains("example.com") && !v.contains(&id),
                    "{} endpoint {v} is not a route template",
                    t.name
                );
            }
        }
    }
}
