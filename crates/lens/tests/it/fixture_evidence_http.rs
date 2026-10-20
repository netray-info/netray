//! HTTP `from_evidence` rebuilds the module's outcome from the `http.*` blocks
//! (specs/features/v2-fixture-consistency/spec.md, Phase 1, requirement 1 and 6; report.md C1,
//! C4, C5, C6, C7, C8). The base blocks are the `http.*` blocks of the seeded history fixture
//! `new.example.com`; each scenario edits the headers table or the port-80 blocks.

use std::path::PathBuf;

use netray_engine::SectionOutcome;
use netray_model::evidence::Block;
use netray_model::{CheckResult, Status};
use serde_json::{Value, json};

const DOMAIN: &str = "new.example.com";

const CHECK_IDS: [&str; 13] = [
    "http.https_redirect",
    "http.hsts",
    "http.cors",
    "http.cookie_secure",
    "http.csp",
    "http.x_frame_options",
    "http.x_content_type_options",
    "http.referrer_policy",
    "http.permissions_policy",
    "http.coop",
    "http.deprecated_headers",
    "http.info_leakage",
    "http.caching",
];

/// The fixture's `http.*` blocks as JSON, so a scenario edits them without the shape types.
fn base_blocks() -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acceptance/fixtures/history/new.example.com.json");
    let fixture: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    fixture["run"]["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["id"].as_str().unwrap().starts_with("http."))
        .cloned()
        .collect()
}

fn block_mut<'a>(blocks: &'a mut [Value], id: &str) -> &'a mut Value {
    blocks.iter_mut().find(|b| b["id"] == id).unwrap()
}

/// Removes every row of `http.headers` named `name`, then adds `name: value` when given.
fn set_header(blocks: &mut [Value], name: &str, value: Option<&str>) {
    let rows = block_mut(blocks, "http.headers")["shape"]["data"]["rows"]
        .as_array_mut()
        .unwrap();
    rows.retain(|r| r[0]["v"] != name);
    if let Some(value) = value {
        rows.push(json!([{"t": "text", "v": name}, {"t": "text", "v": value}]));
    }
}

fn set_probe(blocks: &mut [Value], status: i64, redirects: bool) {
    let data = block_mut(blocks, "http.port80_probe")["shape"]["data"]
        .as_array_mut()
        .unwrap();
    for pair in data.iter_mut() {
        match pair[0].as_str().unwrap() {
            "status_code" => pair[1] = json!({"t": "int", "v": status}),
            "redirects_to_https" => pair[1] = json!({"t": "bool", "v": redirects}),
            _ => {}
        }
    }
}

fn set_port80_chain(blocks: &mut [Value], hops: Vec<Value>) {
    block_mut(blocks, "http.port80")["shape"]["data"] = Value::Array(hops);
}

fn parse(blocks: Vec<Value>) -> Vec<Block> {
    blocks
        .into_iter()
        .map(|b| serde_json::from_value(b).unwrap())
        .collect()
}

async fn outcome(blocks: Vec<Value>) -> SectionOutcome {
    netray_http::testing::from_evidence(DOMAIN, &parse(blocks)).await
}

fn measured(outcome: SectionOutcome) -> (Vec<CheckResult>, Vec<Block>) {
    match outcome {
        SectionOutcome::Measured { checks, evidence } => (checks, evidence),
        other => panic!("expected Measured, got {other:?}"),
    }
}

async fn status_of(blocks: Vec<Value>, check: &str) -> Status {
    let (checks, _) = measured(outcome(blocks).await);
    checks
        .iter()
        .find(|c| c.id.to_string() == check)
        .unwrap_or_else(|| panic!("no check {check}"))
        .status
}

#[tokio::test]
async fn http_from_evidence_measures_all_thirteen_checks() {
    let (checks, _) = measured(outcome(base_blocks()).await);
    let ids: Vec<String> = checks.iter().map(|c| c.id.to_string()).collect();
    for id in CHECK_IDS {
        assert!(ids.iter().any(|i| i == id), "missing {id} in {ids:?}");
    }
    assert_eq!(ids.len(), 13, "{ids:?}");
}

#[tokio::test]
async fn http_from_evidence_x_frame_options_follows_the_headers_table() {
    let mut without = base_blocks();
    set_header(&mut without, "x-frame-options", None);
    assert_ne!(
        status_of(without, "http.x_frame_options").await,
        Status::Pass
    );

    let mut with = base_blocks();
    set_header(&mut with, "x-frame-options", Some("DENY"));
    assert_eq!(
        status_of(with, "http.x_frame_options").await,
        Status::Pass
    );
}

#[tokio::test]
async fn http_from_evidence_https_redirect_follows_the_port_80_evidence() {
    let mut no_redirect = base_blocks();
    set_port80_chain(&mut no_redirect, vec![]);
    set_probe(&mut no_redirect, 200, false);
    assert_eq!(
        status_of(no_redirect, "http.https_redirect").await,
        Status::Fail
    );

    let mut redirect = base_blocks();
    set_port80_chain(
        &mut redirect,
        vec![json!([
            ["url", {"t": "text", "v": "http://new.example.com/"}],
            ["status", {"t": "int", "v": 301}],
            ["location", {"t": "text", "v": "https://new.example.com/"}]
        ])],
    );
    set_probe(&mut redirect, 301, true);
    assert_eq!(
        status_of(redirect, "http.https_redirect").await,
        Status::Pass
    );
}

#[tokio::test]
async fn http_from_evidence_hsts_reads_the_header_and_emits_max_age() {
    let mut blocks = base_blocks();
    set_header(
        &mut blocks,
        "strict-transport-security",
        Some("max-age=31536000"),
    );
    let (checks, evidence) = measured(outcome(blocks).await);
    let hsts = checks.iter().find(|c| c.id.to_string() == "http.hsts").unwrap();
    assert_eq!(hsts.status, Status::Pass);

    let block = evidence
        .iter()
        .find(|b| b.id.to_string() == "http.hsts")
        .expect("an http.hsts block");
    let shape = serde_json::to_value(&block.shape).unwrap();
    assert!(
        shape["data"]
            .as_array()
            .unwrap()
            .contains(&json!(["max_age", {"t": "int", "v": 31536000}])),
        "{shape}"
    );
}

#[tokio::test]
async fn http_from_evidence_is_a_fixpoint() {
    let mut blocks = base_blocks();
    set_header(&mut blocks, "x-frame-options", Some("DENY"));
    set_header(
        &mut blocks,
        "strict-transport-security",
        Some("max-age=31536000"),
    );
    let first = outcome(blocks).await;
    let (_, evidence) = measured(first.clone());
    let second = netray_http::testing::from_evidence(DOMAIN, &evidence).await;
    assert_eq!(first, second);
}
