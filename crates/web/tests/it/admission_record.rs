//! Recording moves into the run (specs/features/v2-admission/spec.md, Phase 2, requirements 3
//! and 4; criteria C2/C10, C11, C12, C3/C13, C14).
//!
//! The modules answer the committed goldens; the apex runs with the state of
//! `tests/fixtures/lens.production.toml` and a `HistoryStore` on a temp file, driven in-process.
//! Entries are read back with `store.entries`, polled with a bound because the run task records
//! after it publishes.
//!
//! Copied from crates/lens/tests/admission_record.rs (spec v2-cutover, Phase 6); dropped as V1:
//! `POST /api/check`, its V1 SSE events and snapshot id, and C14 whole (DKIM-selector runs).
//! C2/C10 drives API v1's `POST /api/v1/domains/{domain}/runs`; C11 and C12 drive `admit` and its
//! `Done`, because API v1 and the result page answer after the record by design (their answer
//! carries how the history recorded the run).

use std::cell::RefCell;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
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
use netray_engine::domain_run::{CheckOutput, OverallScore};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol};
use netray_store::{Entry, HistoryStore};
use netray_web::admission::{
    Admit, ApexState, BadgeCheckFn, RunStream, Trigger, admit, until_done,
};
use sqlx::Connection;
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use tempfile::NamedTempFile;
use tokio::sync::Notify;
use tower::ServiceExt;

const IN_FLIGHT: &str = "netray_runs_in_flight";

/// Counts how often the wrapped module runs.
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
    app: Router,
    state: ApexState,
    store: Arc<HistoryStore>,
    db: NamedTempFile,
}

/// Production config with a migrated store on a temp file. `dns` replaces the DNS module when
/// given; `cache` switches the cache; `badge` injects a `badge_check_fn`.
async fn harness(
    dns: Option<Box<dyn Module>>,
    cache: bool,
    badge: Option<BadgeCheckFn>,
) -> Harness {
    let mut config = common::production_config();
    config.cache_enabled = cache;
    config.limits.rate_limit_per_ip_per_minute = 100;
    config.limits.rate_limit_per_ip_burst = 100;
    let db = NamedTempFile::new().unwrap();

    let store = HistoryStore::open(db.path()).await.unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);

    let registry = registry_with(
        dns.unwrap_or_else(|| dns_golden_raw("prism.sse")),
        facts_golden_raw("prism.sse"),
        http_module(Some("spectra-inspect.json")),
        email_golden("beacon.sse"),
        ip_golden("ifconfig-json.json"),
        tls_golden("tlsight-inspect.json"),
    );
    let plan = common::production_plan(&registry);
    let mut state = common::state_with(config, registry, plan);
    state.history_store = Some(store.clone());
    state.badge_check_fn = badge;

    let app = netray_web::host::apex::routes(state.clone()).layer(Extension(ConnectInfo(
        SocketAddr::from(([127, 0, 0, 1], 40000)),
    )));
    Harness {
        app,
        state,
        store,
        db,
    }
}

/// API v1's fresh run of `example.com`.
async fn post_check(app: &Router) -> StatusCode {
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/domains/example.com/runs")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    to_bytes(resp.into_body(), 16 * 1024 * 1024).await.unwrap();
    status
}

fn client() -> IpAddr {
    IpAddr::from([127, 0, 0, 1])
}

fn admitted(a: Admit) -> RunStream {
    match a {
        Admit::Admitted(s) => s,
        Admit::Refused(r) => panic!("expected a run, got {r:?}"),
        Admit::NotStarted => panic!("expected a run, got NotStarted"),
    }
}

/// Polls the store for `example.com` entries until `want` hold (up to 5 s); returns the last read.
async fn entries_until(store: &HistoryStore, want: usize) -> Vec<Entry> {
    let mut last = Vec::new();
    for _ in 0..250 {
        last = store.entries("example.com").await.unwrap();
        if last.len() >= want {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    last
}

/// A second connection that holds the store's write lock until `rollback`.
async fn hold_writes(db: &NamedTempFile) -> SqliteConnection {
    let mut conn = SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(db.path()))
        .await
        .unwrap();
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut conn)
        .await
        .unwrap();
    conn
}

async fn release_writes(mut conn: SqliteConnection) {
    sqlx::query("ROLLBACK").execute(&mut conn).await.unwrap();
}

/// Accumulates snapshots (`Snapshotter::snapshot()` resets values on read) into the gauge's value.
struct Gauge {
    snap: Snapshotter,
    value: RefCell<Option<f64>>,
}

impl Gauge {
    fn read(&self) -> Option<f64> {
        let mut value = self.value.borrow_mut();
        for (k, _, _, v) in self.snap.snapshot().into_vec() {
            if k.key().name() == IN_FLIGHT
                && let DebugValue::Gauge(g) = v
            {
                *value = Some(value.unwrap_or(0.0) + g.into_inner());
            }
        }
        *value
    }
}

#[tokio::test(flavor = "current_thread")]
async fn admission_c2_c10_dropped_requester_does_not_stop_the_run() {
    let recorder = DebuggingRecorder::new();
    let gauge = Gauge {
        snap: recorder.snapshotter(),
        value: RefCell::new(None),
    };
    let _guard = metrics::set_default_local_recorder(&recorder);
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let dns = gated(
        dns_golden_raw("prism.sse"),
        entered.clone(),
        release.clone(),
    );
    let h = harness(Some(dns), true, None).await;

    let outcome = tokio::time::timeout(Duration::from_millis(500), post_check(&h.app)).await;
    assert!(
        outcome.is_err(),
        "the requester's future is dropped mid-run"
    );
    tokio::time::timeout(Duration::from_secs(1), entered.notified())
        .await
        .expect("the run reached the DNS module");
    assert_eq!(
        gauge.read(),
        Some(1.0),
        "the run outlives its requester while the gate is closed"
    );

    release.notify_one();
    let entries = entries_until(&h.store, 1).await;
    assert_eq!(entries.len(), 1, "the completed run is recorded");
    assert!(entries[0].run.complete);

    let mut after = gauge.read();
    for _ in 0..100 {
        if after == Some(0.0) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
        after = gauge.read();
    }
    assert_eq!(after, Some(0.0), "no run is in flight after it ended");
}

#[tokio::test(flavor = "current_thread")]
async fn admission_c11_response_does_not_wait_for_the_record() {
    let h = harness(None, true, None).await;
    let hold = hold_writes(&h.db).await;

    let mut stream = admitted(admit(
        &h.state,
        "example.com",
        client(),
        Trigger::Visit,
        false,
    ));
    let answered = tokio::time::timeout(Duration::from_secs(2), until_done(&mut stream)).await;
    let answered_in_time = answered.is_ok();
    // Release in every case so a failure does not leave the store locked.
    release_writes(hold).await;

    let published = answered.expect("the result arrives while the store write path is held");
    assert!(answered_in_time);
    assert!(published.is_some(), "the run published its result");
    let entries = entries_until(&h.store, 1).await;
    assert_eq!(
        entries.len(),
        1,
        "the entry appears once the hold is released"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn admission_c12_request_during_pending_record_gets_the_published_result() {
    let runs = Arc::new(AtomicUsize::new(0));
    let dns = Box::new(Counting {
        inner: dns_golden_raw("prism.sse"),
        runs: runs.clone(),
    });
    // The cache is off: only the join of the published run can answer the second request.
    let h = harness(Some(dns), false, None).await;
    let hold = hold_writes(&h.db).await;

    let mut starter = admitted(admit(
        &h.state,
        "example.com",
        client(),
        Trigger::Visit,
        false,
    ));
    let first = tokio::time::timeout(Duration::from_secs(2), until_done(&mut starter)).await;
    let second = match first {
        Ok(_) => {
            let mut joiner = admitted(admit(
                &h.state,
                "example.com",
                client(),
                Trigger::Visit,
                false,
            ));
            let joined = joiner.joined();
            Some((
                joined,
                tokio::time::timeout(Duration::from_secs(2), until_done(&mut joiner)).await,
            ))
        }
        Err(_) => None,
    };
    release_writes(hold).await;

    let p1 = first
        .expect("the first result arrives while the record is pending")
        .expect("the run published");
    let (joined, p2) = second.unwrap();
    let p2 = p2
        .expect("the second result arrives while the record is pending")
        .expect("the joined run published");
    assert!(joined, "the second request joins the run in flight");
    assert_eq!(runs.load(Ordering::SeqCst), 1, "no second run executes");
    assert!(
        Arc::ptr_eq(&p1, &p2),
        "the second request gets the published result"
    );
    assert_eq!(entries_until(&h.store, 1).await.len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn admission_c3_c13_badge_check_fn_run_is_recorded() {
    let mock: BadgeCheckFn = Arc::new(|_domain: String| {
        let fut: Pin<Box<dyn std::future::Future<Output = CheckOutput> + Send>> = Box::pin(async {
            CheckOutput {
                domain: "example.com".to_string(),
                sections: HashMap::new(),
                score: OverallScore {
                    sections: HashMap::new(),
                    overall_percentage: 0.0,
                    grade: "B".to_string(),
                    hard_fail_triggered: false,
                    hard_fail_checks: vec![],
                    not_applicable: HashMap::new(),
                    complete: true,
                },
                duration_ms: 1,
            }
        });
        fut
    });
    let h = harness(None, true, Some(mock)).await;

    let req = Request::builder()
        .uri("/badge/example.com.svg")
        .body(Body::empty())
        .unwrap();
    let resp = h.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();

    let entries = entries_until(&h.store, 1).await;
    assert_eq!(entries.len(), 1, "the badge run is recorded once");
    assert!(
        entries[0].run.blocks.is_empty(),
        "no facts blocks beyond what the mock output carries"
    );
}
