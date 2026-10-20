//! V2 spec v2-inspector-shell, requirements 19 and 20 (C1, C3-C9): a finished run's final
//! fragment carries its exports as inert templates (Markdown, CSV, JSON), built from what the
//! run streamed; the export controls start disabled and a shared run page carries the same.
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
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

const WARN_FINDING: &str = "Strict-Transport-Security is missing";

fn http_ids() -> Vec<String> {
    catalogue()
        .entries()
        .iter()
        .filter(|e| e.protocol == Protocol::Http)
        .map(|e| e.id.to_string())
        .collect()
}

fn stub_checks() -> Vec<CheckResult> {
    http_ids()
        .iter()
        .enumerate()
        .map(|(i, id)| CheckResult {
            id: CheckId::parse(id).unwrap(),
            status: if i == 0 { Status::Warn } else { Status::Pass },
            findings: if i == 0 {
                vec![WARN_FINDING.to_owned()]
            } else {
                Vec::new()
            },
            evidence: Vec::new(),
        })
        .collect()
}

fn text(s: &str) -> Value {
    Value::Text(s.to_owned())
}

fn stub_blocks() -> Vec<Block> {
    vec![
        Block {
            id: BlockId::parse("http.headers").unwrap(),
            title: "Response headers".into(),
            shape: Shape::Table {
                key: "name".into(),
                columns: vec!["name".into(), "value".into()],
                rows: vec![
                    vec![text("server"), text("example")],
                    vec![text("x-note"), text("a, \"quoted\" value")],
                ],
            },
        },
        Block {
            id: BlockId::parse("http.server").unwrap(),
            title: "Server facts".into(),
            shape: Shape::Fields(vec![
                ("software".into(), text("nginx-unique")),
                ("status".into(), Value::Int(204)),
            ]),
        },
    ]
}

struct Stub(Arc<AtomicUsize>);

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let part = JobPart {
            label: "Headers".into(),
            output: Box::pin(async { Ok(stub_blocks()) }),
        };
        let checks = stub_checks();
        Ok(Job::new(vec![part]).with_band(Protocol::Http, Box::pin(async move { Ok(checks) })))
    }
}

fn stub_app() -> (Router, Arc<AtomicUsize>) {
    let starts = Arc::new(AtomicUsize::new(0));
    let deps = InspectDeps::new(Arc::new(Stub(starts.clone())), InspectConfig::default());
    (inspect_router(deps), starts)
}

/// Events as (event name, fragment id, fragment html).
async fn post(app: &Router, tool: &str, body: &str) -> Vec<(String, String, String)> {
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
    let raw_text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let mut events = Vec::new();
    for raw in raw_text.split("\n\n").filter(|e| !e.trim().is_empty()) {
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
    events
}

async fn get(app: &Router, path: &str) -> String {
    let resp = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn html_of<'a>(events: &'a [(String, String, String)], id: &str) -> &'a str {
    &events
        .iter()
        .find(|e| e.1 == id)
        .unwrap_or_else(|| panic!("no {id} fragment in {events:?}"))
        .2
}

fn pos(
    events: &[(String, String, String)],
    pred: impl Fn(&(String, String, String)) -> bool,
) -> usize {
    events
        .iter()
        .position(pred)
        .unwrap_or_else(|| panic!("event missing in {events:?}"))
}

fn attr<'a>(html: &'a str, name: &str) -> &'a str {
    let open = format!("{name}=\"");
    let a = html
        .find(&open)
        .unwrap_or_else(|| panic!("no {name} in {html}"))
        + open.len();
    &html[a..a + html[a..].find('"').unwrap()]
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

/// The raw (still escaped) inner text of each `<template data-export=...>`, by kind.
fn templates(html: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut rest = html;
    let open = "<template data-export=\"";
    while let Some(i) = rest.find(open) {
        let after = &rest[i + open.len()..];
        let kind = &after[..after.find('"').unwrap()];
        let body_start = after.find('>').unwrap() + 1;
        let body_end = after.find("</template>").expect("closing template");
        out.insert(kind.to_owned(), after[body_start..body_end].to_owned());
        rest = &after[body_end..];
    }
    out
}

fn button_tags(html: &str) -> Vec<&str> {
    html.split("<button")
        .skip(1)
        .map(|s| &s[..s.find('>').unwrap()])
        .collect()
}

/// A tiny RFC 4180 parser.
fn parse_csv(s: &str) -> Vec<Vec<String>> {
    let (mut rows, mut row, mut field) = (Vec::new(), Vec::new(), String::new());
    let mut quoted = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' => quoted = true,
                ',' => row.push(std::mem::take(&mut field)),
                '\r' => {}
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                _ => field.push(c),
            }
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

const HTTP_FORM: &str = "url=https%3A%2F%2Fexample.com%2F";

#[tokio::test]
async fn c1_c3_final_export_fragment_holds_three_templates_between_share_and_end_without_a_rerun() {
    let (app, starts) = stub_app();

    let events = post(&app, "http", HTTP_FORM).await;

    let export = pos(&events, |e| e.1 == "inspect-export");
    assert!(export > pos(&events, |e| e.1 == "inspect-band"));
    assert!(export > pos(&events, |e| e.1.starts_with("inspect-part")));
    assert!(export > pos(&events, |e| e.1 == "inspect-share"));
    assert!(export < pos(&events, |e| e.0 == "end"));
    let html = &events[export].2;
    assert_eq!(
        html.matches("<template data-export=\"").count(),
        3,
        "{html}"
    );
    let t = templates(html);
    assert_eq!(
        t.keys().map(String::as_str).collect::<Vec<_>>(),
        ["csv", "json", "markdown"]
    );
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn c4_markdown_export_reads_title_checks_table_and_one_section_per_block() {
    let (app, _) = stub_app();
    let events = post(&app, "http", HTTP_FORM).await;

    let md = unesc(&templates(html_of(&events, "inspect-export"))["markdown"]);

    let md = md.trim_start();
    assert!(
        md.starts_with("# https://example.com/") || md.starts_with("# example.com"),
        "{md}"
    );
    let title = md
        .lines()
        .find(|l| l.contains("LIVE"))
        .unwrap_or_else(|| panic!("no title line in {md}"));
    assert!(
        title.contains("UTC") && title.contains("NOT RECORDED"),
        "{title}"
    );
    assert!(md.contains("\n## Checks\n"), "{md}");
    assert!(
        md.contains("| Code | Check | Status | Finding | Fix |"),
        "{md}"
    );
    let checks = md.split("\n## Checks\n").nth(1).unwrap();
    let checks = checks.split("\n## ").next().unwrap();
    let rows = checks
        .lines()
        .filter(|l| l.starts_with('|'))
        .filter(|l| !l.starts_with("| Code |") && !l.starts_with("|---") && !l.starts_with("| ---"))
        .count();
    assert_eq!(rows, http_ids().len(), "{checks}");
    assert!(checks.contains(WARN_FINDING), "{checks}");
    assert!(md.contains("\n## Response headers\n"), "{md}");
    assert!(md.contains("\n## Server facts\n"), "{md}");
}

#[tokio::test]
async fn c5_csv_export_is_long_format_with_every_check_field_and_block_value_once() {
    let (app, _) = stub_app();
    let events = post(&app, "http", HTTP_FORM).await;

    let csv = unesc(&templates(html_of(&events, "inspect-export"))["csv"]);

    let rows = parse_csv(&csv);
    assert_eq!(rows[0], ["section", "item", "field", "value"], "{csv}");
    assert!(rows.iter().all(|r| r.len() == 4), "{csv}");
    let find = |section: &str, field: &str, value: &str| {
        rows.iter()
            .filter(|r| r[0] == section && r[2] == field && r[3] == value)
            .count()
    };
    for id in http_ids() {
        assert!(
            rows.iter()
                .any(|r| r[0] == "checks" && r[1] == id && r[2] == "status"),
            "no status row for {id}: {csv}"
        );
    }
    let warn_id = &http_ids()[0];
    let warn = |field: &str| {
        rows.iter()
            .filter(|r| r[0] == "checks" && &r[1] == warn_id && r[2] == field)
            .collect::<Vec<_>>()
    };
    assert_eq!(warn("status")[0][3].to_lowercase(), "warn");
    assert_eq!(warn("finding")[0][3], WARN_FINDING);
    assert!(!warn("fix").is_empty(), "{csv}");
    for (field, value) in [
        ("name", "server"),
        ("value", "example"),
        ("name", "x-note"),
        ("value", "a, \"quoted\" value"),
    ] {
        assert_eq!(
            find("http.headers", field, value),
            1,
            "{field}={value}: {csv}"
        );
    }
    assert_eq!(find("http.server", "software", "nginx-unique"), 1, "{csv}");
    assert_eq!(find("http.server", "status", "204"), 1, "{csv}");
}

#[tokio::test]
async fn c6_json_export_has_target_checks_and_blocks_in_the_model_serialisation() {
    let (app, _) = stub_app();
    let events = post(&app, "http", HTTP_FORM).await;

    let json = unesc(&templates(html_of(&events, "inspect-export"))["json"]);

    let v: serde_json::Value =
        serde_json::from_str(&json).unwrap_or_else(|e| panic!("{e}: {json}"));
    let obj = v.as_object().unwrap();
    let mut keys = obj.keys().map(String::as_str).collect::<Vec<_>>();
    keys.sort_unstable();
    assert_eq!(keys, ["blocks", "checks", "target"]);
    let blocks: Vec<Block> = serde_json::from_value(v["blocks"].clone()).unwrap();
    assert_eq!(blocks, stub_blocks());
    let checks: Vec<CheckResult> = serde_json::from_value(v["checks"].clone()).unwrap();
    assert_eq!(checks, stub_checks());
}

#[tokio::test]
async fn c7_start_fragment_buttons_are_disabled_and_the_final_export_buttons_are_not() {
    let (app, _) = stub_app();
    let events = post(&app, "http", HTTP_FORM).await;

    let start = events
        .iter()
        .find(|e| e.2.contains("id=\"inspect-export\""))
        .unwrap_or_else(|| panic!("no export slot in {events:?}"));
    // Only the export slot's own buttons: the run head also holds the share slot's button.
    let slot = &start.2[start.2.find("id=\"inspect-export\"").unwrap()..];
    let slot = &slot[..slot.find("</div>").unwrap_or(slot.len())];
    let start_buttons = button_tags(slot);
    assert_eq!(start_buttons.len(), 3, "{}", start.2);
    assert!(
        start_buttons.iter().all(|b| b.contains("disabled")),
        "{start_buttons:?}"
    );

    let last = button_tags(html_of(&events, "inspect-export"));
    assert_eq!(last.len(), 3);
    assert!(last.iter().all(|b| !b.contains("disabled")), "{last:?}");
}

#[tokio::test]
async fn c8_export_slot_filename_names_the_tool_and_the_target_slug() {
    let (app, _) = stub_app();

    let events = post(&app, "tls", "h=example.com%3A443").await;

    assert_eq!(
        attr(html_of(&events, "inspect-export"), "data-filename"),
        "netray-tls-example.com-443"
    );
}

#[tokio::test]
async fn c9_shared_run_page_carries_the_same_templates_with_enabled_buttons() {
    let (app, starts) = stub_app();
    let events = post(&app, "http", HTTP_FORM).await;
    let key = attr(html_of(&events, "inspect-share"), "data-share-key").to_owned();

    let page = get(&app, &format!("/inspect/http?r={key}")).await;

    assert_eq!(
        templates(&page),
        templates(html_of(&events, "inspect-export"))
    );
    assert_eq!(templates(&page).len(), 3);
    let buttons = button_tags(&page[page.find("id=\"inspect-export\"").expect("export slot")..]);
    assert!(
        buttons.iter().take(3).all(|b| !b.contains("disabled")),
        "{buttons:?}"
    );
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}
