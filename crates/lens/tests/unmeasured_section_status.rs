//! V2 criterion C13: a section incomplete because of an unmeasured check is "incomplete" in both
//! its SSE section event and `summary.sections.<name>`, and `summary.overall` is not "error".
//! A `NotApplicable` check leaves the section "pass".
//!
//! Same harness as `unmeasured_verdict.rs`: a stub email module, the other sections on goldens,
//! the router driven in-process over the SSE endpoint.

mod common;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::LazyLock;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
use common::{dns_golden_raw, facts_golden_raw, http_module, ip_golden, registry_with, tls_module};
use lens::config::Config;
use lens::routes::api_router;
use lens::state::AppState;
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use serde_json::{Value, json};
use tower::ServiceExt;

static CHECKS: LazyLock<Vec<CheckId>> = LazyLock::new(|| {
    vec![
        CheckId::parse("email.spf").unwrap(),
        CheckId::parse("email.dnsbl").unwrap(),
    ]
});

/// An email module whose `email.dnsbl` check carries `dnsbl`.
struct StubEmail {
    dnsbl: Status,
}

impl Module for StubEmail {
    fn protocol(&self) -> Protocol {
        Protocol::Email
    }
    fn checks(&self) -> &'static [CheckId] {
        &CHECKS
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }
    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        let dnsbl = self.dnsbl;
        Box::pin(async move {
            let result = |id: &str, status| CheckResult {
                id: CheckId::parse(id).unwrap(),
                status,
                findings: vec![],
                evidence: vec![],
            };
            SectionOutcome::Measured {
                checks: vec![
                    result("email.spf", Status::Pass),
                    result("email.dnsbl", dnsbl),
                ],
                presentation: json!({}),
                evidence: vec![],
            }
        })
    }
}

fn app(dnsbl: Status) -> Router {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    config.cache.enabled = false;
    config.snapshots.enabled = false;
    let registry = registry_with(
        dns_golden_raw("prism.sse"),
        facts_golden_raw("prism.sse"),
        http_module(Some("spectra-inspect.json")),
        Box::new(StubEmail { dnsbl }),
        ip_golden("ifconfig-json.json"),
        tls_module(Some("tlsight-inspect.json")),
    );
    let state = AppState::with_registry(config, registry).unwrap();
    let (routes, _) = api_router().split_for_parts();
    Router::new()
        .merge(routes.with_state(state))
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))))
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

/// (email event status, summary.sections.email, summary.overall) of one run.
async fn run(dnsbl: Status) -> (String, String, String) {
    let req = Request::builder()
        .method("POST")
        .uri("/api/check")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"domain":"example.com"}"#))
        .unwrap();
    let resp = app(dnsbl).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    let events = parse_sse(core::str::from_utf8(&bytes).unwrap());
    let find = |name: &str| {
        events
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("no {name} event in {events:?}"))
            .1
            .clone()
    };
    let text = |v: &Value, what: &str| {
        v.as_str()
            .unwrap_or_else(|| panic!("no string {what} in {events:?}"))
            .to_string()
    };
    let email = find("email");
    let summary = find("summary");
    (
        text(&email["status"], "email.status"),
        text(&summary["sections"]["email"], "summary.sections.email"),
        text(&summary["overall"], "summary.overall"),
    )
}

/// C13, two rows of one table.
#[tokio::test]
async fn unmeasured_check_makes_its_section_incomplete() {
    let rows = [
        // (dnsbl status, event status, summary section status)
        (Status::Unmeasured, "incomplete", "incomplete"),
        (Status::NotApplicable, "pass", "pass"),
    ];
    for (status, want_event, want_summary) in rows {
        let (event, summary, overall) = run(status).await;
        assert_eq!(event, want_event, "email event status for {status:?}");
        assert_eq!(
            summary, want_summary,
            "summary.sections.email for {status:?}"
        );
        assert_ne!(overall, "error", "summary.overall for {status:?}");
    }
}
