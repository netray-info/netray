// v2-backend-followups, Phase 1 (requirements 3, 4): `http.isolation` tells a header that was
// received but is not readable as text (`unreadable`) from one that was not received (`missing`).
// The `http.coop` status and the V1 wire stay as they were.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use netray_engine::SectionOutcome;
use netray_http::inspect::assembler::InspectResponse;
use netray_http::inspect::{EnrichmentData, InspectResult, TaskResult, assemble_response};
use netray_http::module::{REGISTER, translate};
use netray_model::evidence::{Block, Shape, Value as Ev, ValueType, check_evidence_rule, validate};
use netray_model::{CheckResult, Status};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use url::Url;

const COOP: &str = "cross-origin-opener-policy";
const COEP: &str = "cross-origin-embedder-policy";
const CORP: &str = "cross-origin-resource-policy";

fn task(headers: HeaderMap) -> TaskResult {
    TaskResult {
        final_url: "https://example.com/".into(),
        status: 200,
        http_version: "HTTP/1.1".into(),
        headers,
        redirects: vec![],
        redirect_limit_reached: false,
        error: None,
    }
}

fn response(pairs: &[(&str, &[u8])]) -> InspectResponse {
    let mut https = HeaderMap::new();
    for (k, v) in pairs {
        https.append(
            HeaderName::from_bytes(k.as_bytes()).unwrap(),
            HeaderValue::from_bytes(v).unwrap(),
        );
    }
    assemble_response(
        &Url::parse("https://example.com/").unwrap(),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)), 443),
        InspectResult {
            https: task(https),
            http_upgrade: None,
            cors: task(HeaderMap::new()),
        },
        EnrichmentData::default(),
        None,
        1,
    )
}

fn run(pairs: &[(&str, &[u8])]) -> (Vec<CheckResult>, Vec<Block>) {
    match translate(&response(pairs)) {
        SectionOutcome::Measured {
            checks, evidence, ..
        } => {
            validate("http", &REGISTER, &evidence).expect("blocks fit the register");
            check_evidence_rule(&checks, &evidence).expect("evidence rule holds");
            (checks, evidence)
        }
        other => panic!("expected Measured, got {other:?}"),
    }
}

fn isolation(blocks: &[Block]) -> &[(String, Ev)] {
    blocks
        .iter()
        .find(|b| b.id.to_string() == "http.isolation")
        .map(|b| match &b.shape {
            Shape::Fields(f) => f.as_slice(),
            other => panic!("http.isolation must be fields, got {other:?}"),
        })
        .unwrap_or_else(|| panic!("no http.isolation: {blocks:?}"))
}

fn values(f: &[(String, Ev)], key: &str) -> Vec<String> {
    let mut v: Vec<String> = f
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| match v {
            Ev::Text(t) => t.clone(),
            other => panic!("{key} must be Text, got {other:?}"),
        })
        .collect();
    v.sort();
    v
}

fn coop(checks: &[CheckResult]) -> &CheckResult {
    checks
        .iter()
        .find(|c| c.id.to_string() == "http.coop")
        .expect("http.coop")
}

#[test]
fn register_declares_unreadable_as_text() {
    assert!(
        REGISTER
            .keys
            .contains(&("http.isolation", "unreadable", ValueType::Text)),
        "http.isolation/unreadable missing from the register"
    );
}

#[test]
fn unreadable_coop_is_named_unreadable_not_missing() {
    let (_, blocks) = run(&[(COOP, b"same-origin\xe9")]);
    let iso = isolation(&blocks);
    assert_eq!(values(iso, "unreadable"), vec![COOP]);
    assert_eq!(values(iso, "missing"), vec![COEP, CORP]);
    assert!(values(iso, "coop").is_empty());
}

#[test]
fn unreadable_coep_and_corp_keep_a_readable_coop() {
    let (_, blocks) = run(&[
        (COOP, b"same-origin"),
        (COEP, b"require-corp\xe9"),
        (CORP, b"same-site\xe9"),
    ]);
    let iso = isolation(&blocks);
    assert_eq!(values(iso, "coop"), vec!["same-origin"]);
    assert_eq!(values(iso, "unreadable"), vec![COEP, CORP]);
    assert!(values(iso, "missing").is_empty());
}

#[test]
fn readable_headers_write_neither_missing_nor_unreadable() {
    let (_, blocks) = run(&[
        (COOP, b"same-origin"),
        (COEP, b"require-corp"),
        (CORP, b"same-site"),
    ]);
    let iso = isolation(&blocks);
    assert!(values(iso, "missing").is_empty());
    assert!(values(iso, "unreadable").is_empty());
}

#[test]
fn coop_status_and_findings_table() {
    let rows: Vec<(&str, Vec<(&str, &[u8])>, Status, Vec<&str>)> = vec![
        (
            "unreadable",
            vec![(COOP, b"same-origin\xe9")],
            Status::Warn,
            vec!["Cross-Origin-Opener-Policy header is not readable as text"],
        ),
        (
            "missing",
            vec![],
            Status::Warn,
            vec!["Missing cross-origin-opener-policy header"],
        ),
        (
            "same-origin",
            vec![(COOP, b"same-origin")],
            Status::Pass,
            vec![],
        ),
    ];
    let mut failures = Vec::new();
    for (name, pairs, status, findings) in rows {
        let (checks, _) = run(&pairs);
        let c = coop(&checks);
        if c.status != status {
            failures.push(format!("{name}: status {:?}, want {status:?}", c.status));
        }
        if !findings.is_empty() && c.findings != findings {
            failures.push(format!(
                "{name}: findings {:?}, want {findings:?}",
                c.findings
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn v1_wire_keeps_its_coop_message_and_gains_no_unreadable_key() {
    let v = serde_json::to_value(response(&[(COOP, b"same-origin\xe9")])).unwrap();
    assert_eq!(
        v["security"]["coop"],
        serde_json::json!({"status": "warn", "message": "Header value contains non-ASCII"})
    );
    assert!(
        v.as_object()
            .unwrap()
            .keys()
            .all(|k| k != "unreadable_headers"),
        "V1 wire gained unreadable_headers"
    );
}
