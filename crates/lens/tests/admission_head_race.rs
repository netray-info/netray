//! `/api/check` against a request that races the store commit of the domain's run
//! (specs/features/v2-api/spec.md, requirement 37; criterion C2).
//!
//! lens runs with `tests/fixtures/lens.production.toml` (window 300 s), all five golden modules
//! and a `HistoryStore` on a temp file, on the real clock (the sqlx pool needs it; the throttle
//! minute is passed with `common::pass_throttle`, so the runtime is current-thread). The DNS
//! module answers `prism.sse` on its first run and `prism-no-address.sse` on every later one, so
//! run 2 records a new entry. Request R is held between its head read and `admit` by the
//! `AppState::before_admit` hook; run 2 is started with `admit` directly so the hook does not
//! hold it. The metrics are read from a local recorder (thread-local, hence current-thread).

use std::cell::RefCell;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
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
use lens::admission::{Admit, RunEvent, RunStream, Trigger, admit};
use lens::config::Config;
use lens::routes::api_router;
use lens::snapshot::SnapshotStore;
use lens::state::AppState;
use metrics_util::debugging::{DebugValue, DebuggingRecorder};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol};
use netray_store::HistoryStore;
use sqlx::Connection;
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use tempfile::NamedTempFile;
use tokio::sync::Notify;
use tower::ServiceExt;

const REJECTIONS: &str = "netray_admission_rejections_total";

/// The first run answers `first`, every later run `later`.
struct Switch {
    first: Box<dyn Module>,
    later: Box<dyn Module>,
    runs: AtomicUsize,
}

impl Module for Switch {
    fn protocol(&self) -> Protocol {
        self.first.protocol()
    }
    fn checks(&self) -> &'static [CheckId] {
        self.first.checks()
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        self.first.volatile()
    }
    fn needs_address_records(&self) -> bool {
        self.first.needs_address_records()
    }
    fn run<'a>(&'a self, ctx: &'a RunContext, facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        if self.runs.fetch_add(1, Ordering::SeqCst) == 0 {
            self.first.run(ctx, facts)
        } else {
            self.later.run(ctx, facts)
        }
    }
}

struct Harness {
    app: Router,
    state: AppState,
    store: Arc<HistoryStore>,
    db_path: PathBuf,
    /// While set, the next request to reach the hook announces itself on `at_hook` and waits
    /// for `release`.
    armed: Arc<AtomicBool>,
    at_hook: Arc<Notify>,
    release: Arc<Notify>,
    _db: NamedTempFile,
    _snapshots: NamedTempFile,
}

async fn harness() -> Harness {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    config.cache.enabled = true;
    config.rate_limit.per_ip_per_minute = 100;
    config.rate_limit.per_ip_burst = 100;

    let db = NamedTempFile::new().unwrap();
    let db_path = db.path().to_path_buf();
    config.history.enabled = true;
    config.history.db_path = db_path.clone();
    let store = Arc::new(HistoryStore::open(&db_path).await.unwrap());
    store.migrate().await.unwrap();

    let dns = Box::new(Switch {
        first: dns_golden_raw("prism.sse"),
        later: dns_golden_raw("prism-no-address.sse"),
        runs: AtomicUsize::new(0),
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
    state.history_store = Some(store.clone());
    let snapshots = NamedTempFile::new().unwrap();
    let snapshot_store = SnapshotStore::new(snapshots.path()).await.unwrap();
    snapshot_store.migrate().await.unwrap();
    state.snapshot_store = Some(Arc::new(snapshot_store));

    let armed = Arc::new(AtomicBool::new(false));
    let at_hook = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    {
        let (armed, at_hook, release) = (armed.clone(), at_hook.clone(), release.clone());
        state.before_admit = Some(Arc::new(move || {
            let (armed, at_hook, release) = (armed.clone(), at_hook.clone(), release.clone());
            Box::pin(async move {
                if armed.swap(false, Ordering::SeqCst) {
                    at_hook.notify_one();
                    release.notified().await;
                }
            })
        }));
    }

    let (api, _) = api_router().split_for_parts();
    let app = Router::new()
        .merge(api.with_state(state.clone()))
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
    Harness {
        app,
        state,
        store,
        db_path,
        armed,
        at_hook,
        release,
        _db: db,
        _snapshots: snapshots,
    }
}

struct Answer {
    status: StatusCode,
    cached: Option<bool>,
    snapshot_id: Option<String>,
}

async fn answer(resp: axum::response::Response) -> Answer {
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 8 * 1024 * 1024).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).expect("JSON body");
    Answer {
        status,
        cached: body["done"]["cached"].as_bool(),
        snapshot_id: body["done"]["snapshot_id"].as_str().map(str::to_string),
    }
}

/// `POST /api/check` for example.com, synchronous.
async fn post_check(app: &Router) -> Answer {
    let req = Request::builder()
        .method("POST")
        .uri("/api/check")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"domain":"example.com","stream":false}"#))
        .unwrap();
    answer(app.clone().oneshot(req).await.unwrap()).await
}

/// `GET /api/check/example.com`, synchronous: request R.
async fn get_check(app: Router) -> Answer {
    let req = Request::builder()
        .uri("/api/check/example.com")
        .header(header::ACCEPT, "application/json")
        .body(Body::empty())
        .unwrap();
    answer(app.oneshot(req).await.unwrap()).await
}

/// Polls the store until `want` entries of example.com exist (5 s), then lets the run task
/// finish its post-record work (the rendering insert).
async fn entries_until(store: &HistoryStore, want: usize) -> usize {
    let mut n = 0;
    for _ in 0..250 {
        n = store.entries("example.com").await.unwrap().len();
        if n >= want {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    n
}

async fn age(path: &Path, secs: i64) {
    let mut conn = SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(path))
        .await
        .unwrap();
    sqlx::query("UPDATE entries SET last_seen_at = last_seen_at - ?")
        .bind(secs)
        .execute(&mut conn)
        .await
        .unwrap();
}

fn admitted(a: Admit) -> RunStream {
    match a {
        Admit::Admitted(s) => s,
        Admit::Refused(r) => panic!("expected a run, got a refusal: {r:?}"),
        Admit::NotStarted => panic!("expected a run, got NotStarted"),
    }
}

/// Reads the stream to `Done` and returns the run's snapshot id.
async fn until_published(stream: &mut RunStream) -> Option<String> {
    while let Some(ev) = stream.next().await {
        if let RunEvent::Done(p) = ev {
            return p.snapshot_id.clone();
        }
    }
    panic!("the run ended without Done");
}

async fn until_recorded(stream: &mut RunStream) {
    while stream.next().await.is_some() {}
}

fn throttle_rejections(recorder_snapshot: metrics_util::debugging::Snapshotter) -> u64 {
    let acc: RefCell<HashMap<String, u64>> = RefCell::new(HashMap::new());
    for (k, _, _, v) in recorder_snapshot.snapshot().into_vec() {
        let throttle = k
            .key()
            .labels()
            .any(|l| l.key() == "reason" && l.value() == "throttle");
        if k.key().name() == REJECTIONS
            && throttle
            && let DebugValue::Counter(n) = v
        {
            *acc.borrow_mut().entry("throttle".into()).or_default() += n;
        }
    }
    acc.into_inner().get("throttle").copied().unwrap_or(0)
}

fn client() -> IpAddr {
    IpAddr::from([127, 0, 0, 1])
}

/// R reads a stale head, is held before `admit`, and run 2 starts, records and deregisters;
/// released, R finds the throttle minute running but the head a new fresh entry: it answers
/// that entry as a hit, never 429.
#[tokio::test(flavor = "current_thread")]
async fn admission_head_race_c2_throttled_request_answers_the_new_head() {
    let recorder = DebuggingRecorder::new();
    let snapshotter = recorder.snapshotter();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let h = harness().await;

    let first = post_check(&h.app).await;
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(entries_until(&h.store, 1).await, 1, "run 1 is recorded");
    age(&h.db_path, 301).await;

    h.armed.store(true, Ordering::SeqCst);
    let r = tokio::spawn(get_check(h.app.clone()));
    h.at_hook.notified().await;

    // Run 2 starts after the minute, records a different entry and deregisters.
    common::pass_throttle().await;
    let mut stream = admitted(admit(
        &h.state,
        "example.com",
        client(),
        Trigger::Visit,
        false,
    ));
    let second = until_published(&mut stream).await.expect("snapshot id");
    until_recorded(&mut stream).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        entries_until(&h.store, 2).await,
        2,
        "run 2 recorded a new entry"
    );

    h.release.notify_one();
    let a = r.await.unwrap();
    assert_eq!(a.status, StatusCode::OK, "never 429 THROTTLED");
    assert_eq!(a.cached, Some(true));
    assert_eq!(a.snapshot_id.as_deref(), Some(second.as_str()));
    assert_eq!(
        throttle_rejections(snapshotter),
        0,
        "no rejection is counted"
    );
}

/// Run 2 has published `Done` but its record waits on the store's write lock; the head is the
/// older entry, still inside the window. R joins run 2 and answers its fresh result.
#[tokio::test(flavor = "current_thread")]
async fn admission_head_race_c2_request_joins_a_recording_run() {
    let h = harness().await;

    let first = post_check(&h.app).await;
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(entries_until(&h.store, 1).await, 1, "run 1 is recorded");
    age(&h.db_path, 10).await;

    common::pass_throttle().await;
    let mut lock =
        SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&h.db_path))
            .await
            .unwrap();
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut lock)
        .await
        .unwrap();

    let mut stream = admitted(admit(
        &h.state,
        "example.com",
        client(),
        Trigger::Visit,
        false,
    ));
    let second = until_published(&mut stream).await.expect("snapshot id");

    let a = tokio::time::timeout(Duration::from_secs(3), get_check(h.app.clone()))
        .await
        .expect("R answers while run 2 is still recording");
    assert_eq!(a.status, StatusCode::OK);
    assert_eq!(a.cached, Some(false), "run 2's result, not a stored entry");
    assert_eq!(a.snapshot_id.as_deref(), Some(second.as_str()));
    assert_ne!(a.snapshot_id, first.snapshot_id);

    sqlx::query("COMMIT").execute(&mut lock).await.unwrap();
    until_recorded(&mut stream).await;
    assert_eq!(entries_until(&h.store, 2).await, 2);
}
