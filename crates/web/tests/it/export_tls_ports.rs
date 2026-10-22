//! v2-backend-followups Phase 2, requirements 6 and 7 (C3-C9): a multi-port TLS run exports each
//! port's own checks beside the band's (JSON `ports`, CSV `checks@<port>`, Markdown
//! `## Checks · port <port>`); a port whose checks never arrived is an error section; a run of
//! one port or of another tool exports none of it.
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
use http_body_util::BodyExt;
use netray_engine::catalogue::catalogue;
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, Tool, inspect_router,
};
use tower::ServiceExt;

#[derive(Clone, Copy)]
enum Plan {
    TwoPorts,
    TwoPortsTimedOut465,
    OnePort,
    Dns,
    Http,
}

struct Stub(Plan);

fn check(id: &str, status: Status) -> CheckResult {
    CheckResult {
        id: CheckId::parse(id).unwrap(),
        status,
        findings: Vec::new(),
        evidence: Vec::new(),
    }
}

fn fields_block(port: u16) -> Block {
    Block {
        id: BlockId::parse(&format!("tls.handshake@192.0.2.1:{port}")).unwrap(),
        title: "Handshake".into(),
        shape: Shape::Fields(vec![("protocol".into(), Value::Text("TLSv1.3".into()))]),
    }
}

fn port_part(port: u16) -> JobPart {
    JobPart {
        label: format!("port {port}"),
        output: Box::pin(async move { Ok(vec![fields_block(port)]) }),
    }
}

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }

    fn start(&self, req: JobRequest) -> Result<Job, String> {
        let _ = req;
        let pass = || vec![check("tls.chain_trusted", Status::Pass)];
        let fail = || vec![check("tls.chain_trusted", Status::Fail)];
        Ok(match self.0 {
            Plan::TwoPorts => Job::new(vec![port_part(443), port_part(465)])
                .with_part_checks(0, Box::pin(async move { Ok(pass()) }))
                .with_part_checks(1, Box::pin(async move { Ok(fail()) }))
                .with_band(Protocol::Tls, Box::pin(async move { Ok(pass()) })),
            Plan::TwoPortsTimedOut465 => Job::new(vec![port_part(443), port_part(465)])
                .with_part_checks(0, Box::pin(async move { Ok(pass()) }))
                .with_part_checks(1, Box::pin(async { Err("timed out".to_string()) }))
                .with_band(Protocol::Tls, Box::pin(async move { Ok(pass()) })),
            Plan::OnePort => Job::new(vec![JobPart {
                label: "handshake".into(),
                output: Box::pin(async { Ok(vec![fields_block(443)]) }),
            }])
            .with_part_checks(0, Box::pin(async move { Ok(pass()) }))
            .with_band(Protocol::Tls, Box::pin(async move { Ok(pass()) })),
            Plan::Dns => Job::new(vec![JobPart {
                label: "answers".into(),
                output: Box::pin(async {
                    Ok(vec![Block {
                        id: BlockId::parse("dns.answers").unwrap(),
                        title: "Answers".into(),
                        shape: Shape::Text("example.com".into()),
                    }])
                }),
            }]),
            Plan::Http => {
                let id = catalogue()
                    .entries()
                    .iter()
                    .find(|e| e.protocol == Protocol::Http)
                    .unwrap()
                    .id
                    .to_string();
                Job::new(vec![JobPart {
                    label: "response".into(),
                    output: Box::pin(async { Ok(vec![]) }),
                }])
                .with_band(
                    Protocol::Http,
                    Box::pin(async move { Ok(vec![check(&id, Status::Warn)]) }),
                )
            }
        })
    }
}

fn app(plan: Plan) -> Router {
    inspect_router(InspectDeps::new(
        Arc::new(Stub(plan)),
        InspectConfig::default(),
    ))
}

async fn body(app: &Router, req: Request<Body>) -> String {
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).replace("\r\n", "\n")
}

/// (fragment id, html) of every event.
fn fragments(stream: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for raw in stream.split("\n\n").filter(|e| !e.trim().is_empty()) {
        let data: String = raw
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(|l| l.trim_start())
            .collect();
        if let Ok(j) = serde_json::from_str::<serde_json::Value>(&data) {
            out.push((
                j["id"].as_str().unwrap_or_default().to_owned(),
                j["html"].as_str().unwrap_or_default().to_owned(),
            ));
        }
    }
    out
}

fn unesc(s: &str) -> String {
    s.replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&quot;", "\"")
        .replace("&#34;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#x2f;", "/")
        .replace("&amp;", "&")
        .replace("&#38;", "&")
}

/// The unescaped text of each `<template data-export=...>`, by kind.
fn templates(html: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut rest = html;
    let open = "<template data-export=\"";
    while let Some(i) = rest.find(open) {
        let after = &rest[i + open.len()..];
        let kind = &after[..after.find('"').unwrap()];
        let start = after.find('>').unwrap() + 1;
        let end = after.find("</template>").expect("closing template");
        out.insert(kind.to_owned(), unesc(&after[start..end]));
        rest = &after[end..];
    }
    out
}

struct Run {
    live: BTreeMap<String, String>,
    shared: BTreeMap<String, String>,
}

async fn run_tool(plan: Plan, tool: &str, form: &str) -> Run {
    let app = app(plan);
    let stream = body(
        &app,
        Request::post(format!("/inspect/{tool}/run"))
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(form.to_owned()))
            .unwrap(),
    )
    .await;
    let frags = fragments(&stream);
    let export = &frags
        .iter()
        .find(|(id, _)| id == "inspect-export")
        .unwrap_or_else(|| panic!("no export fragment: {stream}"))
        .1;
    let marker = "data-share-key=\\\"";
    let a = stream.find(marker).expect("no share key") + marker.len();
    let key = &stream[a..a + 12];
    let page = body(
        &app,
        Request::get(format!("/inspect/{tool}?r={key}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    Run {
        live: templates(export),
        shared: templates(&page),
    }
}

async fn tls(plan: Plan) -> Run {
    run_tool(plan, "tls", "h=example.com").await
}

fn json_of(t: &BTreeMap<String, String>) -> serde_json::Value {
    serde_json::from_str(&t["json"]).unwrap_or_else(|e| panic!("{e}: {}", t["json"]))
}

fn status_of(checks: &serde_json::Value) -> Vec<(String, String)> {
    checks
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["id"].as_str().unwrap().to_owned(),
                c["status"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

/// The Markdown section under `heading`, up to the next `## `.
fn md_section<'a>(md: &'a str, heading: &str) -> &'a str {
    let a = md
        .find(&format!("\n{heading}\n"))
        .unwrap_or_else(|| panic!("no {heading:?} in {md}"))
        + 1;
    let rest = &md[a..];
    let end = rest[3..].find("\n## ").map_or(rest.len(), |i| i + 3);
    &rest[..end]
}

fn csv_lines(csv: &str) -> Vec<&str> {
    csv.lines().collect()
}

#[tokio::test]
async fn c3_json_export_lists_each_ports_own_checks_under_ports() {
    let run = tls(Plan::TwoPorts).await;
    let json = json_of(&run.live);

    assert_eq!(
        status_of(&json["checks"]),
        vec![("tls.chain_trusted".to_string(), "pass".to_string())]
    );
    let ports = json["ports"].as_array().unwrap_or_else(|| panic!("{json}"));
    assert_eq!(ports.len(), 2, "{json}");
    assert_eq!(ports[0]["port"], "443");
    assert_eq!(ports[1]["port"], "465");
    assert_eq!(
        status_of(&ports[0]["checks"]),
        vec![("tls.chain_trusted".to_string(), "pass".to_string())]
    );
    assert_eq!(
        status_of(&ports[1]["checks"]),
        vec![("tls.chain_trusted".to_string(), "fail".to_string())]
    );
}

#[tokio::test]
async fn c4_csv_export_adds_checks_at_port_rows_after_the_checks_rows() {
    let run = tls(Plan::TwoPorts).await;
    let lines = csv_lines(&run.live["csv"]);

    let at = |row: &str| {
        lines
            .iter()
            .position(|l| *l == row)
            .unwrap_or_else(|| panic!("no row {row:?} in {lines:?}"))
    };
    let band = at("checks,tls.chain_trusted,status,pass");
    let p443 = at("checks@443,tls.chain_trusted,status,pass");
    let p465 = at("checks@465,tls.chain_trusted,status,fail");
    assert!(band < p443 && p443 < p465, "{lines:?}");
    at("checks@465,tls.chain_trusted,finding,");
}

#[tokio::test]
async fn c5_markdown_export_adds_a_checks_table_per_port_after_the_checks_table() {
    let run = tls(Plan::TwoPorts).await;
    let md = &run.live["markdown"];

    let band = md.find("\n## Checks\n").expect("band table");
    let p443 = md
        .find("\n## Checks · port 443\n")
        .unwrap_or_else(|| panic!("{md}"));
    let p465 = md
        .find("\n## Checks · port 465\n")
        .unwrap_or_else(|| panic!("{md}"));
    assert!(band < p443 && p443 < p465, "{md}");
    let s443 = md_section(md, "## Checks · port 443");
    let s465 = md_section(md, "## Checks · port 465");
    assert!(
        s443.contains("| Code | Check | Status | Finding | Fix |"),
        "{s443}"
    );
    let row = |s: &str| {
        s.lines()
            .find(|l| l.contains("Chain trusted"))
            .unwrap_or_else(|| panic!("no Chain trusted row in {s}"))
            .to_owned()
    };
    assert!(row(s443).contains("| pass |"), "{s443}");
    assert!(row(s465).contains("| fail |"), "{s465}");
}

#[tokio::test]
async fn c6_port_whose_checks_timed_out_is_an_error_section_not_a_status() {
    let run = tls(Plan::TwoPortsTimedOut465).await;

    let lines = csv_lines(&run.live["csv"]);
    assert!(lines.contains(&"checks@465,,error,timed out"), "{lines:?}");
    assert!(
        lines.contains(&"checks@443,tls.chain_trusted,status,pass"),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|l| l.starts_with("checks@465,tls.")),
        "{lines:?}"
    );

    let md = &run.live["markdown"];
    assert!(
        md_section(md, "## checks@465").contains("Not measured: timed out"),
        "{md}"
    );
    assert!(!md.contains("## Checks · port 465"), "{md}");

    let json = json_of(&run.live);
    assert!(
        json["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["section"] == "checks@465" && e["error"] == "timed out"),
        "{json}"
    );
    let ports = json["ports"].as_array().unwrap_or_else(|| panic!("{json}"));
    assert_eq!(ports.len(), 1, "{json}");
    assert_eq!(ports[0]["port"], "443");
}

#[tokio::test]
async fn c7_one_port_run_exports_no_per_port_checks() {
    let run = tls(Plan::OnePort).await;

    assert!(json_of(&run.live).get("ports").is_none());
    assert!(!run.live["csv"].contains("checks@"));
    assert!(!run.live["markdown"].contains("Checks · port"));
}

#[tokio::test]
async fn c8_dns_and_http_runs_export_no_ports_key_and_no_checks_at_rows() {
    let dns = run_tool(Plan::Dns, "dns", "q=example.com+A").await;
    let http = run_tool(Plan::Http, "http", "url=https%3A%2F%2Fexample.com%2F").await;

    for (name, run) in [("dns", dns), ("http", http)] {
        assert!(json_of(&run.live).get("ports").is_none(), "{name}");
        assert!(!run.live["csv"].contains("checks@"), "{name}");
        assert!(!run.live["markdown"].contains("Checks · port"), "{name}");
    }
}

#[tokio::test]
async fn c9_shared_run_page_carries_the_same_per_port_exports_as_the_live_run() {
    let run = tls(Plan::TwoPorts).await;

    for kind in ["markdown", "csv", "json"] {
        assert_eq!(run.shared[kind], run.live[kind], "{kind}");
    }
    assert!(run.shared["csv"].contains("checks@465,tls.chain_trusted,status,fail"));
    assert!(run.shared["markdown"].contains("## Checks · port 465"));
    assert_eq!(
        json_of(&run.shared)["ports"].as_array().map(Vec::len),
        Some(2)
    );
}
