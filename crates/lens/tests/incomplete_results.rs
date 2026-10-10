//! Incomplete results (spec grade-integrity, Phase 2, requirements 3 and 4).
//!
//! Real stub servers serve the committed backend goldens (or a failure, or a rewritten
//! variant of a golden; the HTTP and email sections come from golden modules of the engine
//! registry, or from ones that are incomplete); lens runs with `tests/fixtures/lens.production.toml` (URLs pointed
//! at the stubs), the cache enabled and a temp-file snapshot store. The routers are driven
//! in-process. A result with an Errored section is `incomplete`: never cached, never
//! snapshotted, never badged or rendered as a letter.

use std::net::SocketAddr;
use std::path::PathBuf;

mod common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
use axum::routing::{get, post};
use common::{email_golden, email_incomplete, http_module, registry_with};
use lens::config::Config;
use lens::routes::{api_router, badge_router, og_router};
use lens::snapshot::SnapshotStore;
use lens::state::AppState;
use serde_json::{Value, json};
use tempfile::NamedTempFile;
use tower::ServiceExt;

/// What one backend stub answers.
#[derive(Clone)]
enum Answer {
    Golden(&'static str),
    Body(String),
    Http500,
}

fn contracts_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/contracts")
}

fn golden(name: &str) -> String {
    let path = contracts_dir().join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("golden {} unreadable: {e}", path.display()))
}

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    format!("http://{addr}")
}

async fn stub(
    path: &'static str,
    post_method: bool,
    content_type: &'static str,
    answer: Answer,
) -> String {
    let body = match &answer {
        Answer::Golden(f) => Some(golden(f)),
        Answer::Body(b) => Some(b.clone()),
        Answer::Http500 => None,
    };
    let handler = move || {
        let body = body.clone();
        async move {
            match body {
                Some(b) => (StatusCode::OK, [(header::CONTENT_TYPE, content_type)], b),
                None => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [(header::CONTENT_TYPE, "text/plain")],
                    "backend broke".to_string(),
                ),
            }
        }
    };
    let route = if post_method {
        post(handler)
    } else {
        get(handler)
    };
    serve(Router::new().route(path, route)).await
}

struct Backends {
    dns: Answer,
    tls: Answer,
    /// The spectra golden the HTTP module runs; `None` makes the HTTP section incomplete.
    http: Option<&'static str>,
    /// The beacon golden the email module runs; `None` makes the email section incomplete.
    email: Option<&'static str>,
}

impl Backends {
    fn healthy() -> Self {
        Self {
            dns: Answer::Golden("prism.sse"),
            tls: Answer::Golden("tlsight-inspect.json"),
            http: Some("spectra-inspect.json"),
            email: Some("beacon.sse"),
        }
    }
}

struct Harness {
    app: Router,
    _db: NamedTempFile,
}

/// Production config on stubs, cache enabled, snapshot store on a temp sqlite file.
async fn harness(b: Backends) -> Harness {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    config.backends.dns.url = Some(stub("/api/check", true, "text/event-stream", b.dns).await);
    config.backends.tls.url = Some(stub("/api/inspect", false, "application/json", b.tls).await);
    config.backends.ip.url = Some(
        stub(
            "/json",
            false,
            "application/json",
            Answer::Golden("ifconfig-json.json"),
        )
        .await,
    );
    assert!(config.cache.enabled, "production config enables the cache");
    config.snapshots.enabled = true;

    let db = NamedTempFile::new().unwrap();
    config.snapshots.db_path = db.path().to_path_buf();
    let store = SnapshotStore::new(db.path()).await.unwrap();
    store.migrate().await.unwrap();

    let email = match b.email {
        Some(file) => email_golden(file),
        None => email_incomplete(),
    };
    let registry = registry_with(http_module(b.http), email);
    let mut state = AppState::with_registry(config, registry).unwrap();
    state.snapshot_store = Some(std::sync::Arc::new(store));
    assert!(state.badge_check_fn.is_none(), "use the real check");

    let (api, _) = api_router().split_for_parts();
    let (badge, _) = badge_router().split_for_parts();
    let app = Router::new()
        .merge(api.with_state(state.clone()))
        .merge(badge.with_state(state.clone()))
        .merge(og_router().with_state(state))
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
    Harness { app, _db: db }
}

fn parse_sse(body: &str) -> Vec<(String, Value)> {
    body.replace("\r\n", "\n")
        .split("\n\n")
        .filter_map(|block| {
            let mut event = None;
            let mut data = Vec::new();
            for line in block.lines() {
                if let Some(e) = line.strip_prefix("event:") {
                    event = Some(e.trim().to_string());
                } else if let Some(d) = line.strip_prefix("data:") {
                    data.push(d.strip_prefix(' ').unwrap_or(d).to_string());
                }
            }
            Some((event?, serde_json::from_str(&data.join("\n")).ok()?))
        })
        .collect()
}

struct Checked {
    cache: String,
    events: Vec<(String, Value)>,
}

impl Checked {
    fn event(&self, name: &str) -> &Value {
        &self
            .events
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("no {name} event"))
            .1
    }
}

async fn post_check(app: &Router) -> Checked {
    let req = Request::builder()
        .method("POST")
        .uri("/api/check")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"domain":"example.com"}"#))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cache = resp
        .headers()
        .get("x-cache")
        .expect("x-cache header")
        .to_str()
        .unwrap()
        .to_string();
    let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    Checked {
        cache,
        events: parse_sse(core::str::from_utf8(&bytes).unwrap()),
    }
}

async fn get_path(app: &Router, uri: &str) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let (status, headers) = (resp.status(), resp.headers().clone());
    let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    (status, headers, bytes.to_vec())
}

const SHORT_CACHE: &str = "max-age=300";

fn assert_incomplete(c: &Checked) {
    let summary = c.event("summary");
    assert_eq!(summary["grade"], "incomplete", "summary: {summary}");
    assert_eq!(summary["complete"], false, "summary: {summary}");
}

// ---------------------------------------------------------------------------

#[tokio::test]
async fn incomplete_c4_email_http500_is_incomplete_unsnapshotted_and_uncached() {
    let h = harness(Backends {
        email: None,
        ..Backends::healthy()
    })
    .await;

    let first = post_check(&h.app).await;
    assert_incomplete(&first);
    assert!(
        first.event("done")["snapshot_id"].is_null(),
        "an incomplete result gets no snapshot: {}",
        first.event("done")
    );
    assert_eq!(first.cache, "MISS");

    let second = post_check(&h.app).await;
    assert_eq!(second.cache, "MISS", "an incomplete result is never cached");
}

/// Control for C4: a complete result is snapshotted and cached as before.
#[tokio::test]
async fn complete_result_is_snapshotted_and_cached() {
    let h = harness(Backends::healthy()).await;

    let first = post_check(&h.app).await;
    assert_eq!(first.event("summary")["complete"], true);
    assert!(
        !first.event("done")["snapshot_id"].is_null(),
        "a complete result is snapshotted: {}",
        first.event("done")
    );
    assert_eq!(first.cache, "MISS");

    let second = post_check(&h.app).await;
    assert_eq!(second.cache, "HIT", "a complete result is cached");

    // Sync mode (JSON) serves the same cached result.
    let req = Request::builder()
        .method("POST")
        .uri("/api/check")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json")
        .body(Body::from(r#"{"domain":"example.com"}"#))
        .unwrap();
    let resp = h.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("x-cache").and_then(|v| v.to_str().ok()),
        Some("HIT")
    );
    let ct = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        ct.contains("application/json"),
        "sync cache hit is JSON, got {ct}"
    );
}

/// prism.sse with the A lookup answered NxDomain and an AAAA batch answered NxDomain.
fn prism_nxdomain(prism: &str) -> String {
    let nx = json!({"NxDomain": {"response_time": {"secs": 0, "nanos": 12000000}}});
    let mut out = Vec::new();
    let mut next_completed = 0;
    for block in prism.replace("\r\n", "\n").split("\n\n") {
        let rewritten = block
            .lines()
            .map(|line| {
                let Some(d) = line.strip_prefix("data: ") else {
                    return line.to_string();
                };
                let mut v: Value = serde_json::from_str(d).unwrap();
                if v["record_type"] == "A" {
                    let lookups = v["lookups"]["lookups"].as_array_mut().unwrap();
                    for l in lookups {
                        l["result"] = nx.clone();
                    }
                    next_completed = v["completed"].as_u64().unwrap();
                    // Add the AAAA twin after this block.
                    let mut aaaa = v.clone();
                    aaaa["record_type"] = json!("AAAA");
                    for l in aaaa["lookups"]["lookups"].as_array_mut().unwrap() {
                        l["query"]["record_type"] = json!("AAAA");
                    }
                    return format!("data: {v}\n\nevent: batch\ndata: {aaaa}");
                }
                format!("data: {v}")
            })
            .collect::<Vec<_>>()
            .join("\n");
        out.push(rewritten);
    }
    let _ = next_completed;
    out.join("\n\n")
}

#[tokio::test]
async fn incomplete_c6_no_address_records_and_failed_tls_http_is_incomplete() {
    let h = harness(Backends {
        dns: Answer::Body(prism_nxdomain(&golden("prism.sse"))),
        tls: Answer::Http500,
        http: None,
        ..Backends::healthy()
    })
    .await;
    let c = post_check(&h.app).await;
    assert_incomplete(&c);
}

/// A beacon run that timed out ends with every check skipped: nothing was measured, so the
/// verdict is incomplete, not a passing grade.
#[tokio::test]
async fn incomplete_email_all_skipped_summary_is_incomplete() {
    let h = harness(Backends {
        email: Some("beacon-timeout.sse"),
        ..Backends::healthy()
    })
    .await;
    let c = post_check(&h.app).await;
    assert_incomplete(&c);
}

#[tokio::test]
async fn incomplete_c7_badge_first_shows_question_mark_and_leaves_check_uncached() {
    let h = harness(Backends {
        email: None,
        ..Backends::healthy()
    })
    .await;

    let (status, headers, body) = get_path(&h.app, "/badge/example.com.svg").await;
    assert_eq!(status, StatusCode::OK);
    let cc = headers["cache-control"].to_str().unwrap();
    assert!(cc.contains(SHORT_CACHE), "short Cache-Control, got: {cc}");
    let svg = String::from_utf8(body).unwrap();
    assert!(svg.contains(">?<"), "badge must show ?, got: {svg}");

    let c = post_check(&h.app).await;
    assert_eq!(c.cache, "MISS", "the badge recompute must not cache");
    assert_incomplete(&c);
}

#[tokio::test]
async fn incomplete_c8_og_first_shows_unknown_card_and_leaves_check_uncached() {
    let h = harness(Backends {
        email: None,
        ..Backends::healthy()
    })
    .await;

    let (status, headers, body) = get_path(&h.app, "/og/example.com.png").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.starts_with(b"\x89PNG"), "a PNG card is served");
    let cc = headers["cache-control"].to_str().unwrap();
    assert!(cc.contains(SHORT_CACHE), "short Cache-Control, got: {cc}");

    let c = post_check(&h.app).await;
    assert_eq!(c.cache, "MISS", "the OG recompute must not cache");
    assert_incomplete(&c);
}
