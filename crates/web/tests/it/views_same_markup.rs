//! V2 spec v2-http-ui, requirements 4 and 9 (C4, C9): one run yields the same HSTS view
//! fragment in the inspector's shared page, the evidence entry view and the result page's live panel (its "All evidence" details).
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::{SectionEvent, SectionOutcome};
use netray_model::diff::diff;
use netray_model::evidence::{Block, BlockId, EVIDENCE_SCHEMA_VERSION, Shape, Value};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, CheckResult, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::api::{DomainRefusal, DomainRuns, RunEvent, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router as result_router};
use netray_web::pages::{
    EvidenceDeps, InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, Tool,
    evidence_router, inspect_router,
};
use tower::ServiceExt;

const OPEN: &str = r#"<section class="protocol-view" id="ev-http.hsts""#;
const T: i64 = 1_800_000_000;

fn blocks() -> Vec<Block> {
    vec![
        Block {
            id: BlockId::parse("http.hsts").unwrap(),
            title: "HSTS".into(),
            shape: Shape::Fields(vec![
                ("max_age".into(), Value::Int(31_536_000)),
                ("include_subdomains".into(), Value::Bool(true)),
                ("preload".into(), Value::Bool(false)),
            ]),
        },
        Block {
            id: BlockId::parse("http.headers").unwrap(),
            title: "Response headers".into(),
            shape: Shape::Table {
                key: "name".into(),
                columns: vec!["name".into(), "value".into()],
                rows: vec![vec![
                    Value::Text("server".into()),
                    Value::Text("example".into()),
                ]],
            },
        },
    ]
}

fn evidence() -> Vec<BlockId> {
    vec![BlockId::parse("http.hsts").unwrap()]
}

fn run() -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Http,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("http.hsts").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: evidence(),
        }],
        blocks: blocks(),
    }
}

async fn get(app: &Router, path: &str) -> String {
    let resp = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "{path}");
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn view(page: &str) -> String {
    let a = page
        .find(OPEN)
        .unwrap_or_else(|| panic!("no protocol-view in {page}"));
    let end = page[a..].find("</section>").expect("unclosed view") + "</section>".len();
    page[a..a + end].to_owned()
}

struct Stub;

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        let part = JobPart {
            label: "HSTS".into(),
            output: Box::pin(async { Ok(blocks()) }),
        };
        let checks = vec![CheckResult {
            id: CheckId::parse("http.hsts").unwrap(),
            status: Status::Pass,
            findings: Vec::new(),
            evidence: evidence(),
        }];
        Ok(Job::new(vec![part]).with_band(Protocol::Http, Box::pin(async move { Ok(checks) })))
    }
}

async fn shared_page() -> String {
    let app = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()));
    let resp = app
        .clone()
        .oneshot(
            Request::post("/inspect/http/run")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("url=https%3A%2F%2Fexample.com%2F"))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let marker = "data-share-key=\\\"";
    let a = text.find(marker).expect("no share key") + marker.len();
    let key = &text[a..a + 12];
    get(&app, &format!("/inspect/http?r={key}")).await
}

struct LiveRuns;

impl DomainRuns for LiveRuns {
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
        let check = CheckResult {
            id: CheckId::parse("http.hsts").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: evidence(),
        };
        let events = vec![
            RunEvent::Section(SectionEvent {
                protocol: Protocol::Http,
                outcome: SectionOutcome::Measured {
                    checks: vec![check],
                    evidence: blocks(),
                },
            }),
            RunEvent::Done(run()),
            RunEvent::Recorded(Some(Recorded::NotStored { diff: None })),
        ];
        Box::pin(async move { RunStart::Admitted(Box::pin(futures::stream::iter(events))) })
    }
}

/// The html of the result page's streamed fragment `id` of a live, unrecorded run: its panel
/// holds the "All evidence" details with every block.
async fn live_fragment(id: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let app: Router = result_router(ResultState::new(Arc::new(store), Arc::new(LiveRuns), 300));
    let resp = app
        .oneshot(
            Request::post("/example.com/run")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::ACCEPT, "text/event-stream")
                .body(Body::from("trigger=visit"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    for raw in text.split("\n\n").filter(|e| !e.trim().is_empty()) {
        let mut name = String::new();
        let mut data = String::new();
        for line in raw.lines() {
            if let Some(v) = line.strip_prefix("event:") {
                name = v.trim().to_owned();
            } else if let Some(v) = line.strip_prefix("data:") {
                data.push_str(v.trim_start());
            }
        }
        let j = serde_json::from_str::<serde_json::Value>(&data).unwrap_or_default();
        if name == "fragment" && j["id"].as_str() == Some(id) {
            return j["html"].as_str().unwrap_or_default().to_owned();
        }
    }
    panic!("fragment {id} missing in {text}");
}

#[tokio::test]
async fn c4_c9_the_hsts_view_is_byte_identical_in_shared_page_entry_view_and_result_panel() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let id = match store
        .record("example.com", &run(), &[], T, |p, c| diff(p, c, &[], None))
        .await
        .unwrap()
    {
        Recorded::First { id } => id,
        other => panic!("expected First, got {other:?}"),
    };
    let store = Arc::new(store);

    let shared = view(&shared_page().await);
    let entry = view(
        &get(
            &evidence_router(EvidenceDeps {
                history: store.clone(),
            }),
            &format!("/r/{id}/http"),
        )
        .await,
    );
    let panel = view(&live_fragment("evidence-http").await);

    assert!(shared.contains(r#"data-tone="pass""#), "{shared}");
    assert_eq!(entry, shared);
    assert_eq!(panel, shared);
}
