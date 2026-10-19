// V2 http-checks Phase 2: the `http.csp` tree block and the `csp` check's findings and references.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use netray_engine::SectionOutcome;
use netray_http::inspect::{EnrichmentData, InspectResult, TaskResult, assemble_response};
use netray_http::module::{REGISTER, translate};
use netray_model::CheckResult;
use netray_model::evidence::{Block, Node, Shape, Value as Ev, check_evidence_rule, validate};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use url::Url;

const REPORT_ONLY_FINDING: &str =
    "Content-Security-Policy is report-only: browsers report violations but enforce nothing";

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

fn run(csp_header: Option<(&str, &str)>) -> (Vec<CheckResult>, Vec<Block>) {
    let mut https = HeaderMap::new();
    https.insert("content-type", HeaderValue::from_static("text/html"));
    if let Some((name, value)) = csp_header {
        https.insert(
            HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_str(value).unwrap(),
        );
    }
    let resp = assemble_response(
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
    );
    match translate(&resp) {
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

fn csp_block(blocks: &[Block]) -> Option<&Node> {
    blocks
        .iter()
        .find(|b| b.id.to_string() == "http.csp")
        .map(|b| match &b.shape {
            Shape::Tree(n) => n,
            other => panic!("http.csp must be a tree, got {other:?}"),
        })
}

fn csp_check(checks: &[CheckResult]) -> &CheckResult {
    checks
        .iter()
        .find(|c| c.id.to_string() == "http.csp")
        .unwrap_or_else(|| panic!("no http.csp check: {checks:?}"))
}

fn values(fields: &[(String, Ev)], key: &str) -> Vec<Ev> {
    fields
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .collect()
}

fn text(s: &str) -> Ev {
    Ev::Text(s.into())
}

fn evidence_ids(c: &CheckResult) -> Vec<String> {
    c.evidence.iter().map(|b| b.to_string()).collect()
}

#[test]
fn enforced_csp_is_a_tree_with_one_child_per_directive() {
    let (_, blocks) = run(Some((
        "content-security-policy",
        "default-src 'self'; script-src 'self' 'unsafe-inline' https://cdn.example.com",
    )));
    let root = csp_block(&blocks).expect("http.csp is emitted");
    assert_eq!(values(&root.fields, "report_only"), vec![Ev::Bool(false)]);
    assert_eq!(values(&root.fields, "issue_count"), vec![Ev::Int(2)]);
    assert_eq!(
        values(&root.fields, "issue"),
        vec![text("Missing base-uri directive")]
    );
    let names: Vec<_> = root
        .children
        .iter()
        .map(|c| values(&c.fields, "directive"))
        .collect();
    assert_eq!(
        names,
        vec![vec![text("default-src")], vec![text("script-src")]]
    );
    let script = &root.children[1];
    assert_eq!(
        values(&script.fields, "source"),
        vec![
            text("'self'"),
            text("'unsafe-inline'"),
            text("https://cdn.example.com")
        ]
    );
    assert_eq!(
        values(&script.fields, "issue"),
        vec![text("script-src contains 'unsafe-inline'")]
    );
}

#[test]
fn report_only_csp_marks_the_root_and_leads_the_findings() {
    let (checks, blocks) = run(Some((
        "content-security-policy-report-only",
        "default-src 'self'",
    )));
    let root = csp_block(&blocks).expect("http.csp is emitted");
    assert_eq!(values(&root.fields, "report_only"), vec![Ev::Bool(true)]);
    assert_eq!(csp_check(&checks).findings[0], REPORT_ONLY_FINDING);
}

#[test]
fn no_csp_header_has_no_block_and_references_headers_only() {
    let (checks, blocks) = run(None);
    assert!(csp_block(&blocks).is_none());
    let check = csp_check(&checks);
    assert_eq!(evidence_ids(check), vec!["http.headers"]);
    assert_eq!(check.findings, vec!["No Content-Security-Policy header"]);
}

#[test]
fn csp_findings_are_the_issue_messages_in_order() {
    let (checks, blocks) = run(Some((
        "content-security-policy",
        "default-src 'self'; script-src 'self' 'unsafe-inline' https://cdn.example.com",
    )));
    let root = csp_block(&blocks).expect("http.csp is emitted");
    let mut issues = values(&root.fields, "issue");
    for child in &root.children {
        issues.extend(values(&child.fields, "issue"));
    }
    let check = csp_check(&checks);
    assert_eq!(check.findings.len(), 2, "{:?}", check.findings);
    for f in &check.findings {
        assert!(issues.contains(&text(f)), "{f} is no issue in {issues:?}");
    }
    assert_eq!(evidence_ids(check), vec!["http.headers", "http.csp"]);
}
