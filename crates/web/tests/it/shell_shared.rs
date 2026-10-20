//! V2 spec v2-inspector-shell, requirement 13 (C3, C9, C10, C12, C13, C17): a finished
//! inspector run is kept under a random key for one hour and `GET /inspect/<tool>?r=<key>`
//! renders it server-side, without JavaScript and without a new job.
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::catalogue::catalogue;
use netray_model::diff::diff;
use netray_model::evidence::{Block, BlockId, EVIDENCE_SCHEMA_VERSION, Shape};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, CheckResult, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_tls::testing::{StubTlsBackend, stub_state};
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, InspectJobs, RunStart, RunTrigger,
};
use netray_web::entries::privacy_router;
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, SharedRuns, Tool,
    inspect_router,
};
use tower::ServiceExt;

const BLOCK: &str = "http.headers";
const NOTICE: &str =
    "This shared result has expired or never existed; results are kept for one hour.";

struct Stub(Arc<AtomicUsize>);

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let part = JobPart {
            label: "Headers".into(),
            output: Box::pin(async {
                Ok(vec![Block {
                    id: BlockId::parse(BLOCK).unwrap(),
                    title: "Response headers".into(),
                    shape: Shape::Text("server: example".into()),
                }])
            }),
        };
        let checks = catalogue()
            .entries()
            .iter()
            .filter(|e| e.protocol == Protocol::Http)
            .map(|e| CheckResult {
                id: e.id.clone(),
                status: Status::Pass,
                findings: Vec::new(),
                evidence: Vec::new(),
            })
            .collect::<Vec<_>>();
        Ok(Job::new(vec![part]).with_band(Protocol::Http, Box::pin(async move { Ok(checks) })))
    }
}

struct Events(Vec<(String, String, String)>);

impl Events {
    fn html(&self, id: &str) -> &str {
        &self
            .0
            .iter()
            .find(|e| e.1 == id)
            .unwrap_or_else(|| panic!("no {id} fragment in {:?}", self.0))
            .2
    }
    fn pos(&self, pred: impl Fn(&(String, String, String)) -> bool) -> usize {
        self.0
            .iter()
            .position(pred)
            .unwrap_or_else(|| panic!("event missing in {:?}", self.0))
    }
}

/// POSTs one run; events as (event name, fragment id, fragment html).
async fn post(app: &Router, tool: &str, body: &str) -> Events {
    let resp = app
        .clone()
        .oneshot(
            Request::post(format!("/inspect/{tool}/run"))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let mut events = Vec::new();
    for raw in text.split("\n\n").filter(|e| !e.trim().is_empty()) {
        let (mut name, mut data) = (String::new(), String::new());
        for line in raw.lines() {
            if let Some(v) = line.strip_prefix("event:") {
                name = v.trim().to_owned();
            }
            if let Some(v) = line.strip_prefix("data:") {
                data.push_str(v.trim_start());
            }
        }
        let j = serde_json::from_str::<serde_json::Value>(&data).unwrap_or_default();
        events.push((
            name,
            j["id"].as_str().unwrap_or_default().to_owned(),
            j["html"].as_str().unwrap_or_default().to_owned(),
        ));
    }
    Events(events)
}

async fn get(app: &Router, path: &str) -> (StatusCode, String) {
    let resp = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn app(deps: InspectDeps) -> Router {
    inspect_router(deps)
}

fn attr<'a>(html: &'a str, name: &str) -> &'a str {
    let open = format!("{name}=\"");
    let a = html
        .find(&open)
        .unwrap_or_else(|| panic!("no {name} in {html}"))
        + open.len();
    &html[a..a + html[a..].find('"').unwrap()]
}

fn between<'a>(s: &'a str, open: &str, close: &str) -> &'a str {
    let a = s.find(open).unwrap_or_else(|| panic!("no {open} in {s}")) + open.len();
    &s[a..a + s[a..].find(close).unwrap()]
}

fn is_key(k: &str) -> bool {
    k.len() == 12
        && k.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

const HTTP_FORM: &str = "url=https%3A%2F%2Fexample.com%2F";

/// Runs one stub HTTP run and returns its events and share key.
async fn finished_run(app: &Router) -> (Events, String) {
    let events = post(app, "http", HTTP_FORM).await;
    let key = attr(events.html("inspect-share"), "data-share-key").to_owned();
    (events, key)
}

fn stub_app() -> (Router, Arc<AtomicUsize>) {
    let starts = Arc::new(AtomicUsize::new(0));
    let deps = InspectDeps::new(Arc::new(Stub(starts.clone())), InspectConfig::default());
    (app(deps), starts)
}

#[tokio::test]
async fn c9_share_fragment_after_parts_and_the_key_renders_the_run_without_a_job() {
    let (app, starts) = stub_app();

    let (events, key) = finished_run(&app).await;

    assert!(is_key(&key), "{key}");
    assert_eq!(
        attr(events.html("inspect-share"), "data-share-url"),
        format!("/inspect/http?r={key}")
    );
    let share = events.pos(|e| e.1 == "inspect-share");
    assert!(share > events.pos(|e| e.1 == "inspect-band"));
    assert!(share > events.pos(|e| e.1.starts_with("inspect-part")));
    assert!(share < events.pos(|e| e.0 == "end"));

    let (status, page) = get(&app, &format!("/inspect/http?r={key}")).await;
    assert_eq!(status, StatusCode::OK);
    let h1 = r#"<h1 class="inspector-run__target">"#;
    assert_eq!(
        between(&page, h1, "</h1>"),
        between(events.html("inspect-result"), h1, "</h1>")
    );
    assert!(
        page.contains(r#"<figure class="inspector-band">"#),
        "{page}"
    );
    assert!(page.contains("inspector-checks"), "{page}");
    assert!(page.contains(&format!(r#"id="ev-{BLOCK}""#)), "{page}");
    assert!(!page.contains("data-stream-autostart"), "{page}");
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn c10_shared_page_run_title_reads_shared_and_a_utc_start_time() {
    let (app, _) = stub_app();
    let (_, key) = finished_run(&app).await;

    let (_, page) = get(&app, &format!("/inspect/http?r={key}")).await;

    let title = between(&page, r#"<p class="run-line run-line--title">"#, "</p>");
    let parts: Vec<&str> = title.split(" · ").map(str::trim).collect();
    assert_eq!(parts[0], "shared", "{title}");
    let date = parts[1];
    let b = date.as_bytes();
    let shape_ok = date.len() == "2026-10-20 10:30 UTC".len()
        && date.ends_with(" UTC")
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b' '
        && b[13] == b':'
        && [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15]
            .iter()
            .all(|&i| b[i].is_ascii_digit());
    assert!(shape_ok, "{title}");
    assert!(page.contains(&format!(r#"id="ev-{BLOCK}""#)), "{page}");
}

#[tokio::test]
async fn c12_unknown_malformed_or_foreign_keys_answer_404_with_the_idle_shell() {
    let (app, _) = stub_app();
    let (_, http_key) = finished_run(&app).await;
    let paths = [
        "/inspect/http?r=0123456789ab".to_owned(),
        "/inspect/http?r=zzz".to_owned(),
        "/inspect/http?r=0123456789abc".to_owned(),
        "/inspect/http?r=0123456789AB".to_owned(),
        format!("/inspect/tls?r={http_key}"),
    ];
    for path in paths {
        let (status, page) = get(&app, &path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert!(page.contains("inspector-shell"), "{path}: {page}");
        assert!(
            page.contains(&format!(r#"<p class="inspector-notice">{NOTICE}</p>"#)),
            "{path}: {page}"
        );
    }
}

#[tokio::test]
async fn c13_a_run_is_gone_after_its_ttl() {
    let starts = Arc::new(AtomicUsize::new(0));
    let deps = InspectDeps::new(Arc::new(Stub(starts)), InspectConfig::default())
        .with_shared_runs(SharedRuns::with_ttl(Duration::from_millis(200)));
    let app = app(deps);
    let (_, key) = finished_run(&app).await;
    let path = format!("/inspect/http?r={key}");

    tokio::time::sleep(Duration::from_millis(600)).await;

    let mut status = StatusCode::OK;
    for _ in 0..40 {
        status = get(&app, &path).await.0;
        if status == StatusCode::NOT_FOUND {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn c3_privacy_page_says_a_shared_result_is_kept_one_hour_and_own_address_never() {
    let resp = privacy_router()
        .oneshot(Request::get("/privacy").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let page = String::from_utf8_lossy(&bytes).to_lowercase();

    assert!(page.contains("one hour"), "{page}");
    assert!(page.contains("own address"), "{page}");
}

struct NoRuns;

impl DomainRuns for NoRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_string())
    }
    fn start<'a>(
        &'a self,
        _: &'a str,
        _: IpAddr,
        _: RunTrigger,
        _: bool,
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
    }
}

fn seeded_run() -> Run {
    Run {
        profile_version: 3,
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
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

async fn snapshot(pool: &sqlx::SqlitePool) -> (i64, Vec<(String, i64)>) {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entries")
        .fetch_one(pool)
        .await
        .unwrap();
    let rows = sqlx::query_as("SELECT id, last_seen_at FROM entries ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap();
    (count, rows)
}

#[tokio::test]
async fn c17_sharing_a_tls_run_leaves_the_history_store_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let store = Arc::new(HistoryStore::open(&db).await.unwrap());
    store.migrate().await.unwrap();
    match store
        .record("example.com", &seeded_run(), &[], 1_800_000_000, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap()
    {
        Recorded::First { .. } => {}
        other => panic!("expected First, got {other:?}"),
    }
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(&db);
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
    let before = snapshot(&pool).await;
    assert_eq!(before.0, 1);

    let state = ApiState::new(ApiConfig::default(), Arc::new(NoRuns))
        .with_store(Some(store.clone()))
        .with_parts(ApiParts {
            tls: Some(stub_state(Arc::new(StubTlsBackend::new())).inspector()),
            ..Default::default()
        });
    let app = app(
        InspectDeps::new(Arc::new(InspectJobs::new(state)), InspectConfig::default())
            .with_history(Some(store)),
    );

    let events = post(&app, "tls", "h=example.com%3A443").await;

    let key = attr(events.html("inspect-share"), "data-share-key");
    assert!(is_key(key), "{key}");
    assert_eq!(snapshot(&pool).await, before);
}
