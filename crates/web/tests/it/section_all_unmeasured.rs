//! v2-correctness Phase 5 (requirement 10; criteria C4 to C7): a section the module reports
//! measured with every check `unmeasured` is reported `incomplete` with the reason "every check
//! unmeasured: <first finding>"; its checks stay as they are. Driven through
//! `POST /api/v1/domains/example.com/runs` on a stub HTTP module.
//!
//! Copied from crates/lens/tests/it/section_all_unmeasured.rs (spec v2-cutover, Phase 6); the
//! app is the apex over the state of `tests/fixtures/lens.production.toml`; nothing dropped.

use std::net::SocketAddr;

use crate::common;

use axum::Extension;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use common::{
    dns_golden_raw, email_golden, facts_golden_raw, ip_golden, registry_with, tls_golden,
};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use serde_json::{Value, json};
use tower::ServiceExt;

const URI: &str = "/api/v1/domains/example.com/runs";
const HTTP_CHECKS: [&str; 13] = [
    "https_redirect",
    "hsts",
    "cors",
    "cookie_secure",
    "csp",
    "x_frame_options",
    "x_content_type_options",
    "referrer_policy",
    "permissions_policy",
    "coop",
    "deprecated_headers",
    "info_leakage",
    "caching",
];
const TIMEOUT: &str = "HTTPS probe: timed out after 10s";

/// An HTTP module answering `Measured` with the given `(check, status)` rows; the first
/// unmeasured row carries the timeout finding.
struct StubHttp {
    ids: &'static [CheckId],
    rows: Vec<(CheckId, Status)>,
}

impl Module for StubHttp {
    fn protocol(&self) -> Protocol {
        Protocol::Http
    }
    fn checks(&self) -> &'static [CheckId] {
        self.ids
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }
    fn needs_address_records(&self) -> bool {
        true
    }
    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        let checks = self
            .rows
            .iter()
            .map(|(id, status)| CheckResult {
                id: id.clone(),
                status: *status,
                findings: vec![match status {
                    Status::Unmeasured => TIMEOUT.to_string(),
                    _ => "ok".to_string(),
                }],
                evidence: vec![],
            })
            .collect();
        Box::pin(async move {
            SectionOutcome::Measured {
                checks,
                evidence: vec![],
            }
        })
    }
}

fn stub(first: Status, rest: Status, count: usize) -> Box<dyn Module> {
    let ids: Vec<CheckId> = HTTP_CHECKS
        .iter()
        .map(|n| CheckId::parse(&format!("http.{n}")).unwrap())
        .collect();
    let rows = ids
        .iter()
        .take(count)
        .enumerate()
        .map(|(i, id)| (id.clone(), if i == 0 { first } else { rest }))
        .collect();
    Box::new(StubHttp {
        ids: Box::leak(ids.into_boxed_slice()),
        rows,
    })
}

fn app(http: Box<dyn Module>) -> Router {
    let mut config = common::production_config();
    config.cache_enabled = false;
    let registry = registry_with(
        dns_golden_raw("prism.sse"),
        facts_golden_raw("prism.sse"),
        http,
        email_golden("beacon.sse"),
        ip_golden("ifconfig-json.json"),
        tls_golden("tlsight-inspect.json"),
    );
    let plan = common::production_plan(&registry);
    let state = common::state_with(config, registry, plan);
    netray_web::host::apex::app(state).layer(Extension(ConnectInfo(SocketAddr::from((
        [127, 0, 0, 1],
        40000,
    )))))
}

async fn post(app: &Router, accept: &str) -> String {
    let req = Request::builder()
        .method("POST")
        .uri(URI)
        .header(header::ACCEPT, accept)
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn http_section(sections: &Value) -> &Value {
    sections
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["protocol"] == "http")
        .unwrap_or_else(|| panic!("no http section in {sections}"))
}

async fn json_http_section(http: Box<dyn Module>) -> (Value, Value) {
    let body = post(&app(http), "application/json").await;
    let answer: Value = serde_json::from_str(&body).unwrap_or_else(|e| panic!("{e}: {body}"));
    let run = answer["run"].clone();
    (http_section(&run["sections"]).clone(), run)
}

fn unmeasured_checks(run: &Value) -> Vec<Value> {
    run["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["id"].as_str().unwrap().starts_with("http."))
        .cloned()
        .collect()
}

#[tokio::test(flavor = "current_thread")]
async fn section_all_unmeasured_c4_json_answer_reports_incomplete_with_reason() {
    let (section, run) = json_http_section(stub(Status::Unmeasured, Status::Unmeasured, 13)).await;

    assert_eq!(section["outcome"], "incomplete", "{section}");
    assert_eq!(
        section["reason"],
        format!("every check unmeasured: {TIMEOUT}")
    );
    let checks = unmeasured_checks(&run);
    assert_eq!(checks.len(), 13);
    for c in &checks {
        assert_eq!(c["status"], "unmeasured", "{c}");
        assert_eq!(c["findings"], json!([TIMEOUT]), "{c}");
    }
    assert_eq!(run["grade"], "incomplete");
}

#[tokio::test(flavor = "current_thread")]
async fn section_all_unmeasured_c5_section_event_carries_incomplete_reason_and_checks() {
    let body = post(
        &app(stub(Status::Unmeasured, Status::Unmeasured, 13)),
        "text/event-stream",
    )
    .await;

    let data = body
        .split("\n\n")
        .find(|b| b.contains("event: section") && b.contains("\"protocol\":\"http\""))
        .unwrap_or_else(|| panic!("no http section event: {body}"));
    let data = data
        .lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n");
    let event: Value = serde_json::from_str(&data).unwrap();
    assert_eq!(event["outcome"], "incomplete", "{event}");
    assert_eq!(
        event["reason"],
        format!("every check unmeasured: {TIMEOUT}")
    );
    let checks = event["checks"].as_array().unwrap();
    assert_eq!(checks.len(), 13);
    assert!(checks.iter().all(|c| c["status"] == "unmeasured"));
}

#[tokio::test(flavor = "current_thread")]
async fn section_all_unmeasured_c6_one_pass_check_keeps_the_section_measured() {
    let (section, _) = json_http_section(stub(Status::Pass, Status::Unmeasured, 13)).await;

    assert_eq!(section["outcome"], "measured", "{section}");
    assert!(section["reason"].is_null(), "{section}");
}

#[tokio::test(flavor = "current_thread")]
async fn section_all_unmeasured_c7_measured_section_without_checks_stays_measured() {
    let (section, _) = json_http_section(stub(Status::Pass, Status::Pass, 0)).await;

    assert_eq!(section["outcome"], "measured", "{section}");
    assert!(section["reason"].is_null(), "{section}");
}
