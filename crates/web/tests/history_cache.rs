//! The history is the cache (specs/features/v2-admission/spec.md, Phase 3, requirements 6, 8, 9;
//! criteria C1/C7, C3, C4/C14, C8-C13, C15, C16).
//!
//! The apex runs with lens's `tests/fixtures/lens.production.toml` (window 300 s), all five
//! golden modules and a `HistoryStore` on a temp file, driven in-process through the result
//! page's `POST /{domain}/run` with `trigger=visit` (a fresh head answers without a run). A
//! counting wrapper on the DNS module counts the runs. Entries are aged through a second
//! connection on the same file.
//!
//! Copied from crates/lens/tests/history_cache.rs (spec v2-cutover, Phase 6). Dropped as V1: every
//! `X-Cache`, `done.cached` and snapshot-id assertion of `POST /api/check`, and the cases about
//! the V1 moka rendering holder: C3 (moka not keyed by domain), C9 (a fresh head whose rendering
//! is not held starts a run) and C10 (a rendering under a previous entry is not served). C11
//! (cache disabled) runs on the badge, the route that still reads `cache_enabled`; C12 (no
//! store) on API v1's `POST /api/v1/domains/{domain}/runs`, since without a store the result
//! page is not mounted; C16 (failed store writes) records through API v1 and asks the badge,
//! since the result page answers 500 on a store it cannot read.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

mod common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
use common::{
    dns_golden_raw, email_golden, facts_golden_raw, http_module, ip_golden, registry_with,
    tls_golden,
};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol};
use netray_store::{Entry, HistoryStore};
use netray_web::admission::ApexState;
use sqlx::Connection;
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use tempfile::NamedTempFile;
use tower::ServiceExt;

/// Counts runs of the wrapped module; answers `Incomplete` while `incomplete` is set.
struct Probe {
    inner: Box<dyn Module>,
    runs: Arc<AtomicUsize>,
    incomplete: Arc<AtomicBool>,
}

impl Module for Probe {
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
        if self.incomplete.load(Ordering::SeqCst) {
            return Box::pin(async {
                SectionOutcome::Incomplete {
                    reason: "module failed".to_string(),
                }
            });
        }
        self.inner.run(ctx, facts)
    }
}

struct Harness {
    app: Router,
    state: ApexState,
    store: Option<Arc<HistoryStore>>,
    /// Owns the temp db; `None` when the harness has no store.
    db: Option<NamedTempFile>,
    db_path: PathBuf,
    runs: Arc<AtomicUsize>,
    incomplete: Arc<AtomicBool>,
}

struct Opts {
    cache: bool,
    store: bool,
    incomplete: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            cache: true,
            store: true,
            incomplete: false,
        }
    }
}

async fn harness(o: Opts) -> Harness {
    let mut config = common::production_config();
    config.cache_enabled = o.cache;
    config.limits.rate_limit_per_ip_per_minute = 100;
    config.limits.rate_limit_per_ip_burst = 100;

    let (db, db_path) = if o.store {
        let f = NamedTempFile::new().unwrap();
        let p = f.path().to_path_buf();
        (Some(f), p)
    } else {
        (None, PathBuf::new())
    };
    let mut store = None;
    if o.store {
        let s = HistoryStore::open(&db_path).await.unwrap();
        s.migrate().await.unwrap();
        store = Some(Arc::new(s));
    }

    let runs = Arc::new(AtomicUsize::new(0));
    let incomplete = Arc::new(AtomicBool::new(o.incomplete));
    let dns = Box::new(Probe {
        inner: dns_golden_raw("prism.sse"),
        runs: runs.clone(),
        incomplete: incomplete.clone(),
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
    let mut state = common::state_with(config, registry, plan);
    state.history_store = store.clone();

    let app = netray_web::host::apex::routes(state.clone())
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
    Harness {
        app,
        state,
        store,
        db,
        db_path,
        runs,
        incomplete,
    }
}

/// A visit of the result page: `POST /example.com/run` with `trigger=visit`, a plain form post.
/// A fresh head and a finished run both answer 303 to the page.
async fn visit(app: &Router) -> StatusCode {
    let req = Request::builder()
        .method("POST")
        .uri("/example.com/run")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("trigger=visit"))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let _ = to_bytes(resp.into_body(), 8 * 1024 * 1024).await.unwrap();
    status
}

/// A fresh API v1 run of example.com.
async fn api_run(app: &Router) -> StatusCode {
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/domains/example.com/runs")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let _ = to_bytes(resp.into_body(), 8 * 1024 * 1024).await.unwrap();
    status
}

/// The badge of example.com.
async fn badge(app: &Router) -> StatusCode {
    let req = Request::builder()
        .uri("/badge/example.com.svg")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let _ = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    status
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Polls the store for `example.com` entries until `want` exist (up to 5 s), then waits until
/// the run task is done.
async fn entries_until(h: &Harness, want: usize) -> Vec<Entry> {
    let store = h.store.as_ref().unwrap();
    let mut last = Vec::new();
    for _ in 0..250 {
        last = store.entries("example.com").await.unwrap();
        if last.len() >= want {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    h.state.runs.drain().await;
    last
}

/// Moves every entry's `last_seen_at` by `-secs` (negative `secs` moves it forward).
async fn age(path: &std::path::Path, secs: i64) {
    let mut conn = SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(path))
        .await
        .unwrap();
    sqlx::query("UPDATE entries SET last_seen_at = last_seen_at - ?")
        .bind(secs)
        .execute(&mut conn)
        .await
        .unwrap();
}

/// First complete run, recorded.
async fn first_run(h: &Harness) {
    assert_eq!(visit(&h.app).await, StatusCode::SEE_OTHER);
    assert_eq!(h.runs.load(Ordering::SeqCst), 1);
    let entries = entries_until(h, 1).await;
    assert_eq!(entries.len(), 1, "the complete run is recorded");
}

#[tokio::test(flavor = "multi_thread")]
async fn history_cache_c1_c7_fresh_head_is_a_hit() {
    let h = harness(Opts::default()).await;
    first_run(&h).await;
    age(&h.db_path, 10).await;

    assert_eq!(visit(&h.app).await, StatusCode::SEE_OTHER);
    assert_eq!(h.runs.load(Ordering::SeqCst), 1, "no run executes");
}

#[tokio::test(flavor = "current_thread")]
async fn history_cache_c8_head_older_than_the_window_starts_a_run() {
    let h = harness(Opts::default()).await;
    first_run(&h).await;
    age(&h.db_path, 301).await;

    // v2-admission R12/R16: a second run of the domain starts only after the throttle.
    common::pass_throttle().await;
    assert_eq!(visit(&h.app).await, StatusCode::SEE_OTHER);
    assert_eq!(h.runs.load(Ordering::SeqCst), 2, "a run starts");
}

#[tokio::test(flavor = "current_thread")]
async fn history_cache_c11_cache_disabled_starts_a_run_on_a_fresh_head() {
    let h = harness(Opts {
        cache: false,
        ..Opts::default()
    })
    .await;
    first_run(&h).await;

    // v2-admission R12/R16: a second run of the domain starts only after the throttle.
    common::pass_throttle().await;
    assert_eq!(badge(&h.app).await, StatusCode::OK);
    assert_eq!(h.runs.load(Ordering::SeqCst), 2, "a run starts");
}

/// Without a store nothing is a hit. The clock is paused only so the 61 s pass instantly.
#[tokio::test(start_paused = true)]
async fn history_cache_c12_no_store_two_requests_start_two_runs() {
    let h = harness(Opts {
        store: false,
        ..Opts::default()
    })
    .await;

    assert_eq!(api_run(&h.app).await, StatusCode::OK);
    tokio::time::advance(Duration::from_secs(61)).await;
    assert_eq!(api_run(&h.app).await, StatusCode::OK);
    assert_eq!(h.runs.load(Ordering::SeqCst), 2, "both start runs");
}

#[tokio::test(flavor = "current_thread")]
async fn history_cache_c13_identical_run_refreshes_the_head() {
    let h = harness(Opts::default()).await;
    let store = h.store.as_ref().unwrap();
    first_run(&h).await;
    age(&h.db_path, 400).await;

    // Stale head: the second run records Identical and moves last_seen_at to now.
    // v2-admission R12/R16: a second run of the domain starts only after the throttle.
    common::pass_throttle().await;
    assert_eq!(visit(&h.app).await, StatusCode::SEE_OTHER);
    assert_eq!(h.runs.load(Ordering::SeqCst), 2);
    let mut fresh = false;
    for _ in 0..250 {
        let head = store.latest("example.com").await.unwrap().unwrap();
        if head.last_seen_at > now_unix() - 100 {
            fresh = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(fresh, "the second run recorded Identical");
    h.state.runs.drain().await;

    assert_eq!(visit(&h.app).await, StatusCode::SEE_OTHER);
    assert_eq!(h.runs.load(Ordering::SeqCst), 2, "no third run");
}

/// Observable shape: run 1 is complete (head); the head is aged 400 s and the DNS module is
/// switched to incomplete, so request 2 runs and ends incomplete (not recorded). The head is then
/// made fresh again (moved forward 400 s): request 3 is the head's hit and starts no run, though
/// the module would still answer incomplete.
#[tokio::test(flavor = "current_thread")]
async fn history_cache_c4_c14_an_incomplete_run_never_replaces_the_head() {
    let h = harness(Opts::default()).await;
    first_run(&h).await;
    age(&h.db_path, 400).await;
    h.incomplete.store(true, Ordering::SeqCst);

    // v2-admission R12/R16: a second run of the domain starts only after the throttle.
    common::pass_throttle().await;
    assert_eq!(visit(&h.app).await, StatusCode::SEE_OTHER);
    assert_eq!(
        h.runs.load(Ordering::SeqCst),
        2,
        "a stale head starts a run"
    );
    h.state.runs.drain().await;
    let entries = h
        .store
        .as_ref()
        .unwrap()
        .entries("example.com")
        .await
        .unwrap();
    assert_eq!(entries.len(), 1, "the incomplete run is not stored");

    age(&h.db_path, -400).await;
    assert_eq!(visit(&h.app).await, StatusCode::SEE_OTHER);
    assert_eq!(h.runs.load(Ordering::SeqCst), 2, "the fresh head answers");
}

#[tokio::test(flavor = "current_thread")]
async fn history_cache_c15_incomplete_run_without_entry_is_not_a_hit() {
    let h = harness(Opts {
        incomplete: true,
        ..Opts::default()
    })
    .await;

    assert_eq!(visit(&h.app).await, StatusCode::SEE_OTHER);
    h.state.runs.drain().await;
    // v2-admission R12/R16: a second run of the domain starts only after the throttle.
    common::pass_throttle().await;
    assert_eq!(visit(&h.app).await, StatusCode::SEE_OTHER);
    assert_eq!(h.runs.load(Ordering::SeqCst), 2, "the second request runs");
}

#[tokio::test(flavor = "current_thread")]
async fn history_cache_c16_failed_store_writes_leave_no_hit() {
    let h = harness(Opts::default()).await;
    let mut conn =
        SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&h.db_path))
            .await
            .unwrap();
    sqlx::query("DROP TABLE entries")
        .execute(&mut conn)
        .await
        .unwrap();

    assert_eq!(
        api_run(&h.app).await,
        StatusCode::OK,
        "the request works without a store"
    );
    h.state.runs.drain().await;
    // v2-admission R12/R16: a second run of the domain starts only after the throttle.
    common::pass_throttle().await;
    assert_eq!(badge(&h.app).await, StatusCode::OK);
    assert_eq!(
        h.runs.load(Ordering::SeqCst),
        2,
        "nothing recorded, so no hit"
    );
    drop(h.db);
}
