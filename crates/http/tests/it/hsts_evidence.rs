// V2 evidence, HTTP row: the `http.hsts` fields block (max_age, include_subdomains, preload),
// present only when a Strict-Transport-Security header was received, and the hsts check's
// references to it.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use netray_engine::SectionOutcome;
use netray_http::inspect::assembler::InspectResponse;
use netray_http::inspect::{EnrichmentData, InspectResult, TaskResult, assemble_response};
use netray_http::module::{REGISTER, translate};
use netray_model::CheckResult;
use netray_model::evidence::{Block, Shape, Value as Ev, check_evidence_rule, validate};
use reqwest::header::{HeaderMap, HeaderValue};
use url::Url;

fn task(status: u16, headers: HeaderMap) -> TaskResult {
    TaskResult {
        final_url: "https://example.com/".into(),
        status,
        http_version: "HTTP/1.1".into(),
        headers,
        redirects: vec![],
        redirect_limit_reached: false,
        error: None,
    }
}

fn response(hsts: Option<&'static str>) -> InspectResponse {
    let mut https = HeaderMap::new();
    https.insert("content-type", HeaderValue::from_static("text/html"));
    if let Some(v) = hsts {
        https.insert("strict-transport-security", HeaderValue::from_static(v));
    }
    assemble_response(
        &Url::parse("https://example.com/").unwrap(),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)), 443),
        InspectResult {
            https: task(200, https),
            http_upgrade: None,
            cors: task(200, HeaderMap::new()),
        },
        EnrichmentData::default(),
        None,
        1,
    )
}

fn run(resp: &InspectResponse) -> (Vec<CheckResult>, Vec<Block>) {
    match translate(resp) {
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

fn hsts_fields(blocks: &[Block]) -> Option<&[(String, Ev)]> {
    blocks
        .iter()
        .find(|b| b.id.to_string() == "http.hsts")
        .map(|b| match &b.shape {
            Shape::Fields(f) => f.as_slice(),
            other => panic!("http.hsts must be fields, got {other:?}"),
        })
}

fn hsts_evidence(checks: &[CheckResult]) -> Vec<String> {
    checks
        .iter()
        .find(|c| c.id.to_string() == "http.hsts")
        .unwrap_or_else(|| panic!("no http.hsts check"))
        .evidence
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn get<'a>(f: &'a [(String, Ev)], key: &str) -> Option<&'a Ev> {
    f.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

#[test]
fn hsts_block_follows_the_received_header() {
    // max-age with includeSubDomains
    let (checks, blocks) = run(&response(Some("max-age=31536000; includeSubDomains")));
    let f = hsts_fields(&blocks).expect("http.hsts present");
    assert_eq!(get(f, "max_age"), Some(&Ev::Int(31_536_000)));
    assert_eq!(get(f, "include_subdomains"), Some(&Ev::Bool(true)));
    assert_eq!(get(f, "preload"), Some(&Ev::Bool(false)));
    assert_eq!(hsts_evidence(&checks), ["http.headers", "http.hsts"]);

    // includeSubDomains without max-age: block present, no max_age key
    let (checks, blocks) = run(&response(Some("includeSubDomains")));
    let f = hsts_fields(&blocks).expect("http.hsts present");
    assert_eq!(get(f, "max_age"), None);
    assert_eq!(get(f, "include_subdomains"), Some(&Ev::Bool(true)));
    assert_eq!(hsts_evidence(&checks), ["http.headers", "http.hsts"]);

    // no header: no block, the check references only the headers table
    let (checks, blocks) = run(&response(None));
    assert!(hsts_fields(&blocks).is_none(), "{blocks:?}");
    assert_eq!(hsts_evidence(&checks), ["http.headers"]);
}
