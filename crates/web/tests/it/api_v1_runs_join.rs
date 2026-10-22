//! `POST /api/v1/domains/{domain}/runs` goes through lens's admission coordinator (spec v2-api,
//! requirement 14; criteria C14 to C18 and C20).
//!
//! The apex runs with lens's `tests/fixtures/lens.production.toml` (`common::production_config`),
//! the five golden modules and a `HistoryStore` on a temp file, driven in-process through
//! `netray_web::host::apex::app`. The HTTP, email and IP
//! modules can be held open on one gate (DNS and TLS then finish alone); a counting wrapper on
//! the DNS module counts the runs.
//!
//! Copied from crates/lens/tests/api_v1_runs_join.rs (spec v2-cutover, Phase 6); the run in
//! flight that C14 and C16 join is started by the result page's `POST /{domain}/run` (a plain
//! form post, answered 303 once the run is done) instead of the dropped V1 `POST /api/check`;
//! C18 compared V1 `/api/check` JSON answers only and is dropped.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode, header};
use common::{
    dns_golden_raw, email_golden, facts_golden_raw, gated, http_module, ip_golden, registry_with,
    tls_golden,
};
use futures::StreamExt;
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::admission::ApexState;
use serde_json::Value;
use tempfile::NamedTempFile;
use tokio::sync::Notify;
use tokio::time::timeout;
use tower::ServiceExt;

const RUNS: &str = "/api/v1/domains/example.com/runs";
const WAIT: Duration = Duration::from_secs(10);

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
    app: Router,
    state: ApexState,
    store: Arc<HistoryStore>,
    runs: Arc<AtomicUsize>,
    entered: Arc<Notify>,
    release: Arc<Notify>,
    _db: NamedTempFile,
}

impl Harness {
    fn runs(&self) -> usize {
        self.runs.load(Ordering::SeqCst)
    }

    /// Opens the gate for every run that waits (a permit left over is harmless).
    async fn open_gate(&self) {
        for _ in 0..10 {
            self.release.notify_one();
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Lets the first request reach the gate and the later ones arrive, then opens the gate.
    async fn open_gate_after_arrivals(&self) {
        self.entered.notified().await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        self.open_gate().await;
    }
}

async fn harness(hold: bool) -> Harness {
    let mut config = common::production_config();
    config.limits.rate_limit_per_ip_per_minute = 1000;
    config.limits.rate_limit_per_ip_burst = 1000;
    let db = NamedTempFile::new().unwrap();
    let store = HistoryStore::open(db.path()).await.unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);

    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let runs = Arc::new(AtomicUsize::new(0));
    let hold_open = |m: Box<dyn Module>| {
        if hold {
            gated(m, entered.clone(), release.clone())
        } else {
            m
        }
    };
    let registry = registry_with(
        Box::new(Counting {
            inner: dns_golden_raw("prism.sse"),
            runs: runs.clone(),
        }),
        facts_golden_raw("prism.sse"),
        hold_open(http_module(Some("spectra-inspect.json"))),
        hold_open(email_golden("beacon.sse")),
        hold_open(ip_golden("ifconfig-json.json")),
        tls_golden("tlsight-inspect.json"),
    );
    let plan = common::production_plan(&registry);
    let mut state = common::state_with(config, registry, plan);
    state.history_store = Some(store.clone());
    Harness {
        app: netray_web::host::apex::app(state.clone()),
        state,
        store,
        runs,
        entered,
        release,
        _db: db,
    }
}

struct Answer {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl Answer {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }
}

fn request(method: &str, uri: &str, extra: &[(&str, &str)], body: Option<&str>) -> Request<Body> {
    let mut b = Request::builder().method(method).uri(uri);
    for (k, v) in extra {
        b = b.header(*k, *v);
    }
    let body = match body {
        Some(s) => {
            b = b.header(header::CONTENT_TYPE, "application/json");
            Body::from(s.to_string())
        }
        None => Body::empty(),
    };
    let mut req = b.body(body).unwrap();
    let peer: SocketAddr = "127.0.0.1:40000".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    req
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

async fn post_run(app: &Router, extra: &[(&str, &str)], body: Option<&str>) -> Answer {
    send(app, request("POST", RUNS, extra, body)).await
}

/// The result page's plain form post: it starts a run for `example.com` and answers 303 once
/// the run is done.
async fn page_run_post(app: &Router) -> Answer {
    let mut b = Request::builder()
        .method("POST")
        .uri("/example.com/run")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    b = b.header(header::ACCEPT, "text/html");
    let mut req = b.body(Body::from("trigger=visit")).unwrap();
    let peer: SocketAddr = "127.0.0.1:40000".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    send(app, req).await
}

/// A request that arrives while the first one holds the gate.
async fn later() {
    tokio::time::sleep(Duration::from_millis(200)).await;
}

/// Polls the store for `example.com` entries until `want` exist (up to 5 s), then lets the run
/// task finish its post-record work.
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
    store.entries("example.com").await.unwrap().len().max(n)
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn stored_run(profile_version: u32) -> Run {
    Run {
        profile_version,
        engine_version: "0.26.0".into(),
        evidence_schema_version: netray_model::evidence::EVIDENCE_SCHEMA_VERSION,
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
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

#[tokio::test(flavor = "current_thread")]
async fn api_v1_runs_c14_post_joins_a_page_run_in_flight_and_records_once() {
    let h = harness(true).await;

    let first = page_run_post(&h.app);
    let second = async {
        later().await;
        post_run(&h.app, &[], None).await
    };
    let (page, v2, ()) = tokio::join!(first, second, h.open_gate_after_arrivals());

    assert_eq!(page.status, StatusCode::SEE_OTHER);
    assert_eq!(
        v2.status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&v2.body)
    );
    assert_eq!(h.runs(), 1, "the POST joined the run in flight");
    let body = v2.json();
    assert!(
        body["run"]["checks"].is_array(),
        "the answer carries the run: {body}"
    );
    assert_eq!(
        entries_until(&h.store, 1).await,
        1,
        "the store records once"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn api_v1_runs_c15_head_inside_the_window_and_outside_the_throttle_starts_a_run() {
    let h = harness(false).await;
    let seeded_at = now_unix() - 120;
    h.store
        .record(
            "example.com",
            &stored_run(h.state.scoring_profile.version),
            &[],
            seeded_at,
            |p, c| netray_model::diff::diff(p, c, &[], None),
        )
        .await
        .unwrap();

    let a = post_run(&h.app, &[], None).await;

    assert_eq!(
        a.status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&a.body)
    );
    assert_eq!(h.runs(), 1, "a fresh run starts, no cache answer");
    entries_until(&h.store, 2).await;
    let head = h.store.latest("example.com").await.unwrap().unwrap();
    assert!(
        head.last_seen_at > seeded_at + 60,
        "the head was written by the fresh run"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn api_v1_runs_c16_prerender_without_a_run_is_refused_and_leaves_no_throttle() {
    let h = harness(false).await;
    let a = post_run(&h.app, &[("sec-purpose", "prerender")], None).await;
    assert_eq!(a.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(a.headers.get("cache-control").unwrap(), "no-store");
    assert_eq!(a.json()["error"]["code"], "PREFETCH_REFUSED");
    assert_eq!(h.runs(), 0, "no run started");

    let b = post_run(&h.app, &[], None).await;
    assert_eq!(
        b.status,
        StatusCode::OK,
        "not throttled by the refused request"
    );
    assert_eq!(h.runs(), 1);

    // With a run in flight the prerender request joins it.
    let h = harness(true).await;
    let first = page_run_post(&h.app);
    let joiner = async {
        h.entered.notified().await;
        post_run(&h.app, &[("sec-purpose", "prerender")], None).await
    };
    let opener = async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        h.open_gate().await;
    };
    let (plain, joined, ()) = tokio::join!(first, joiner, opener);
    assert_eq!(plain.status, StatusCode::SEE_OTHER);
    assert_eq!(
        joined.status,
        StatusCode::OK,
        "the prerender request joined the run"
    );
    assert_eq!(h.runs(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn api_v1_runs_c17_a_body_with_dkim_selectors_is_ignored() {
    let with = harness(false).await;
    let a = post_run(&with.app, &[], Some(r#"{"dkim_selectors":["s1"]}"#)).await;
    assert_eq!(
        a.status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&a.body)
    );
    let a = a.json();
    assert_eq!(
        a["recorded"], "first",
        "recorded like a run without options: {a}"
    );
    assert_eq!(entries_until(&with.store, 1).await, 1);

    let without = harness(false).await;
    let b = post_run(&without.app, &[], None).await.json();
    assert_eq!(
        a["run"]["checks"], b["run"]["checks"],
        "no selector was probed"
    );
    assert_eq!(a["run"]["grade"], b["run"]["grade"]);
}

/// Reads SSE events `(name, data)` from `body` as they arrive.
struct Sse {
    stream: std::pin::Pin<
        Box<dyn futures::Stream<Item = Result<axum::body::Bytes, axum::Error>> + Send>,
    >,
    buf: String,
}

impl Sse {
    async fn next(&mut self) -> Option<(String, Value)> {
        loop {
            if let Some(end) = self.buf.find("\n\n") {
                let block: String = self.buf.drain(..end + 2).collect();
                let mut name = String::new();
                let mut data = String::new();
                for line in block.lines() {
                    if let Some(v) = line.strip_prefix("event:") {
                        name = v.trim().to_string();
                    } else if let Some(v) = line.strip_prefix("data:") {
                        data.push_str(v.trim_start());
                    }
                }
                if name.is_empty() && data.is_empty() {
                    continue;
                }
                return Some((name, serde_json::from_str(&data).unwrap_or(Value::Null)));
            }
            let chunk = timeout(WAIT, self.stream.next())
                .await
                .expect("SSE stalled")?;
            self.buf
                .push_str(&String::from_utf8_lossy(&chunk.unwrap()).replace("\r\n", "\n"));
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn api_v1_runs_c20_event_stream_sections_arrive_as_they_finish_then_done_after_recorded() {
    let h = harness(true).await;
    let resp = timeout(
        WAIT,
        h.app.clone().oneshot(request(
            "POST",
            RUNS,
            &[("accept", "text/event-stream")],
            None,
        )),
    )
    .await
    .expect("headers arrive while the run is held")
    .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("text/event-stream")
    );
    let mut sse = Sse {
        stream: Box::pin(resp.into_body().into_data_stream()),
        buf: String::new(),
    };

    // DNS and TLS finish while the other sections are held at the gate.
    let mut early = Vec::new();
    for _ in 0..2 {
        let (name, data) = sse.next().await.expect("an early section event");
        assert_eq!(name, "section");
        early.push(data["protocol"].as_str().unwrap().to_string());
    }
    early.sort();
    assert_eq!(early, vec!["dns", "tls"]);

    h.open_gate().await;
    let mut late = Vec::new();
    for _ in 0..3 {
        let (name, data) = sse.next().await.expect("a late section event");
        assert_eq!(name, "section");
        late.push(data["protocol"].as_str().unwrap().to_string());
    }
    late.sort();
    assert_eq!(late, vec!["email", "http", "ip"]);

    let (name, data) = sse.next().await.expect("the done event");
    assert_eq!(name, "done");
    assert_eq!(data["recorded"], "first");
    assert_eq!(
        h.store.entries("example.com").await.unwrap().len(),
        1,
        "done follows the Recorded point"
    );
    assert!(sse.next().await.is_none(), "done comes last");
}
