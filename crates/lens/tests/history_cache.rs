//! The history is the cache (specs/features/v2-admission/spec.md, Phase 3, requirements 6, 8, 9;
//! criteria C1/C7, C3, C4/C14, C8-C13, C15, C16).
//!
//! lens runs with `tests/fixtures/lens.production.toml` (window 300 s), all five golden modules
//! and a `HistoryStore` on a temp file, driven in-process through `POST /api/check` with
//! `"stream": false`. A counting wrapper on the DNS module counts the runs. Entries are aged
//! through a second connection on the same file; the run task records after the response, so
//! tests poll the store with a bound.

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
use lens::config::Config;
use lens::routes::api_router;
use lens::snapshot::SnapshotStore;
use lens::state::AppState;
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol, Status};
use netray_store::{Entry, HistoryStore, Recorded};
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
    state: AppState,
    store: Option<Arc<HistoryStore>>,
    /// Owns the temp db; `None` when the harness reuses another's file or has no store.
    db: Option<NamedTempFile>,
    db_path: PathBuf,
    runs: Arc<AtomicUsize>,
    incomplete: Arc<AtomicBool>,
    _snapshots: Option<NamedTempFile>,
}

struct Opts {
    cache: bool,
    store: bool,
    /// An existing, migrated db to open instead of a fresh one.
    reuse: Option<PathBuf>,
    incomplete: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            cache: true,
            store: true,
            reuse: None,
            incomplete: false,
        }
    }
}

async fn harness(o: Opts) -> Harness {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    config.cache.enabled = o.cache;
    config.rate_limit.per_ip_per_minute = 100;
    config.rate_limit.per_ip_burst = 100;

    let (db, db_path) = match (&o.reuse, o.store) {
        (Some(p), _) => (None, p.clone()),
        (None, true) => {
            let f = NamedTempFile::new().unwrap();
            let p = f.path().to_path_buf();
            (Some(f), p)
        }
        (None, false) => (None, PathBuf::new()),
    };
    let mut store = None;
    if o.store {
        config.history.enabled = true;
        config.history.db_path = db_path.clone();
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
    let mut state = AppState::with_registry(config, registry).unwrap();
    state.history_store = store.clone();
    // Complete runs carry a snapshot id only when a snapshot store exists. The storeless
    // harness (paused clock, where a sqlx pool times out) has none.
    let mut snapshots = None;
    if o.store {
        let f = NamedTempFile::new().unwrap();
        let snapshot_store = SnapshotStore::new(f.path()).await.unwrap();
        snapshot_store.migrate().await.unwrap();
        state.snapshot_store = Some(Arc::new(snapshot_store));
        snapshots = Some(f);
    }

    let (api, _) = api_router().split_for_parts();
    let app = Router::new()
        .merge(api.with_state(state.clone()))
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
    Harness {
        app,
        state,
        store,
        db,
        db_path,
        runs,
        incomplete,
        _snapshots: snapshots,
    }
}

struct Answer {
    status: StatusCode,
    x_cache: String,
    cached: bool,
    snapshot_id: Option<String>,
}

async fn check(app: &Router) -> Answer {
    let req = Request::builder()
        .method("POST")
        .uri("/api/check")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"domain":"example.com","stream":false}"#))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let x_cache = resp
        .headers()
        .get("x-cache")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let bytes = to_bytes(resp.into_body(), 8 * 1024 * 1024).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).expect("sync JSON body");
    Answer {
        status,
        x_cache,
        cached: body["done"]["cached"].as_bool().unwrap(),
        snapshot_id: body["done"]["snapshot_id"].as_str().map(str::to_string),
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Polls the store for `example.com` entries until `want` exist (up to 5 s), then lets the run
/// task finish its post-record work (the rendering insert).
async fn entries_until(store: &HistoryStore, want: usize) -> Vec<Entry> {
    let mut last = Vec::new();
    for _ in 0..250 {
        last = store.entries("example.com").await.unwrap();
        if last.len() >= want {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
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

/// First complete run, recorded and held: returns its snapshot id.
async fn first_run(h: &Harness) -> String {
    let a = check(&h.app).await;
    assert_eq!(a.status, StatusCode::OK);
    assert_eq!(a.x_cache, "MISS");
    assert_eq!(h.runs.load(Ordering::SeqCst), 1);
    let entries = entries_until(h.store.as_ref().unwrap(), 1).await;
    assert_eq!(entries.len(), 1, "the complete run is recorded");
    a.snapshot_id.expect("a run carries a snapshot id")
}

#[tokio::test(flavor = "multi_thread")]
async fn history_cache_c1_c7_fresh_head_with_held_rendering_is_a_hit() {
    let h = harness(Opts::default()).await;
    let first = first_run(&h).await;
    age(&h.db_path, 10).await;

    let a = check(&h.app).await;
    assert_eq!(h.runs.load(Ordering::SeqCst), 1, "no run executes");
    assert_eq!(a.x_cache, "HIT");
    assert!(a.cached);
    assert_eq!(a.snapshot_id.as_deref(), Some(first.as_str()));
}

#[tokio::test(flavor = "multi_thread")]
async fn history_cache_c3_moka_is_not_keyed_by_domain() {
    let h = harness(Opts::default()).await;
    first_run(&h).await;

    let cache = h.state.cache.as_ref().expect("cache is enabled");
    cache.run_pending_tasks().await;
    assert!(
        cache.get("example.com").await.is_none(),
        "the rendering is held under the entry id, not the domain"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn history_cache_c8_head_older_than_the_window_starts_a_run() {
    let h = harness(Opts::default()).await;
    first_run(&h).await;
    age(&h.db_path, 301).await;

    let a = check(&h.app).await;
    assert_eq!(a.x_cache, "MISS");
    assert_eq!(h.runs.load(Ordering::SeqCst), 2, "a run starts");
}

#[tokio::test(flavor = "multi_thread")]
async fn history_cache_c9_fresh_head_without_held_rendering_starts_a_run() {
    let first = harness(Opts::default()).await;
    first_run(&first).await;

    // A restart: fresh AppState (nothing held) on the same history.db.
    let h = harness(Opts {
        reuse: Some(first.db_path.clone()),
        ..Opts::default()
    })
    .await;
    let a = check(&h.app).await;
    assert_eq!(a.x_cache, "MISS");
    assert_eq!(h.runs.load(Ordering::SeqCst), 1, "a run starts");
}

#[tokio::test(flavor = "multi_thread")]
async fn history_cache_c10_rendering_under_a_previous_entry_is_not_served() {
    let h = harness(Opts::default()).await;
    let first = first_run(&h).await;
    let store = h.store.as_ref().unwrap();

    // A newer head whose rendering is not held: the first run's run with one status flipped.
    let head = store.latest("example.com").await.unwrap().unwrap();
    let mut run = head.run.clone();
    run.checks[0].status = if run.checks[0].status == Status::Fail {
        Status::Pass
    } else {
        Status::Fail
    };
    let recorded = store
        .record("example.com", &run, &[], now_unix(), |p, c| {
            netray_model::diff::diff(p, c, &[], None)
        })
        .await
        .unwrap();
    assert!(matches!(recorded, Recorded::Changed { .. }));

    let a = check(&h.app).await;
    assert_eq!(a.x_cache, "MISS", "the previous rendering is not served");
    assert_eq!(h.runs.load(Ordering::SeqCst), 2, "a run starts");
    assert_ne!(a.snapshot_id.as_deref(), Some(first.as_str()));
}

#[tokio::test(flavor = "multi_thread")]
async fn history_cache_c11_cache_disabled_starts_a_run_on_a_fresh_head() {
    let h = harness(Opts {
        cache: false,
        ..Opts::default()
    })
    .await;
    first_run(&h).await;

    let a = check(&h.app).await;
    assert_eq!(a.x_cache, "MISS");
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

    let a = check(&h.app).await;
    assert_eq!(a.status, StatusCode::OK);
    tokio::time::advance(Duration::from_secs(61)).await;
    let b = check(&h.app).await;
    assert_eq!(b.status, StatusCode::OK);
    assert_eq!(b.x_cache, "MISS");
    assert_eq!(h.runs.load(Ordering::SeqCst), 2, "both start runs");
}

#[tokio::test(flavor = "multi_thread")]
async fn history_cache_c13_identical_run_refreshes_the_held_rendering() {
    let h = harness(Opts::default()).await;
    let store = h.store.as_ref().unwrap();
    let first = first_run(&h).await;
    age(&h.db_path, 400).await;

    // Stale head: the second run records Identical and moves last_seen_at to now.
    let second = check(&h.app).await;
    assert_eq!(second.x_cache, "MISS");
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
    tokio::time::sleep(Duration::from_millis(200)).await;
    let second_id = second.snapshot_id.clone().expect("snapshot id");
    assert_ne!(second_id, first, "two runs, two snapshots");

    let a = check(&h.app).await;
    assert_eq!(h.runs.load(Ordering::SeqCst), 2, "no third run");
    assert_eq!(a.x_cache, "HIT");
    assert_eq!(a.snapshot_id.as_deref(), Some(second_id.as_str()));
}

/// Observable shape: run 1 is complete (head, rendering held); the head is aged 400 s and the
/// DNS module is switched to incomplete, so request 2 runs and ends incomplete (not recorded).
/// Request 2 is a MISS, not run 1's rendering. The head is then made fresh again (moved forward
/// 400 s): request 3 is the head's hit with run 1's snapshot id and starts no run, though the
/// module would still answer incomplete.
#[tokio::test(flavor = "multi_thread")]
async fn history_cache_c4_c14_an_incomplete_run_never_replaces_the_head() {
    let h = harness(Opts::default()).await;
    let first = first_run(&h).await;
    age(&h.db_path, 400).await;
    h.incomplete.store(true, Ordering::SeqCst);

    let second = check(&h.app).await;
    assert_eq!(second.x_cache, "MISS", "a stale head starts a run");
    assert_eq!(h.runs.load(Ordering::SeqCst), 2);
    assert_ne!(second.snapshot_id.as_deref(), Some(first.as_str()));
    tokio::time::sleep(Duration::from_millis(400)).await;
    let entries = h
        .store
        .as_ref()
        .unwrap()
        .entries("example.com")
        .await
        .unwrap();
    assert_eq!(entries.len(), 1, "the incomplete run is not stored");

    age(&h.db_path, -400).await;
    let third = check(&h.app).await;
    assert_eq!(h.runs.load(Ordering::SeqCst), 2, "the fresh head answers");
    assert_eq!(third.x_cache, "HIT");
    assert_eq!(third.snapshot_id.as_deref(), Some(first.as_str()));
}

#[tokio::test(flavor = "multi_thread")]
async fn history_cache_c15_incomplete_run_without_entry_is_not_a_hit() {
    let h = harness(Opts {
        incomplete: true,
        ..Opts::default()
    })
    .await;

    let a = check(&h.app).await;
    assert_eq!(a.x_cache, "MISS");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let b = check(&h.app).await;
    assert_eq!(b.x_cache, "MISS");
    assert!(!b.cached);
    assert_eq!(h.runs.load(Ordering::SeqCst), 2, "the second request runs");
}

#[tokio::test(flavor = "multi_thread")]
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

    let a = check(&h.app).await;
    assert_eq!(
        a.status,
        StatusCode::OK,
        "the request works without a store"
    );
    tokio::time::sleep(Duration::from_millis(400)).await;
    let b = check(&h.app).await;
    assert_eq!(b.status, StatusCode::OK);
    assert_eq!(b.x_cache, "MISS", "nothing recorded, so no hit");
    assert_eq!(h.runs.load(Ordering::SeqCst), 2);
    drop(h.db);
}
