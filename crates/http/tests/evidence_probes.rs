// V2 evidence, Phase 4 HTTP row: the probe data the JSON wire drops. Repeated headers join into
// one row, the port-80 probe has its own block, every cookie gets a row, and the CORS block
// carries the raw header values.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use netray_engine::SectionOutcome;
use netray_http::inspect::assembler::InspectResponse;
use netray_http::inspect::{EnrichmentData, InspectResult, TaskResult, assemble_response};
use netray_http::module::{REGISTER, translate};
use netray_model::evidence::{Block, Shape, Value as Ev, check_evidence_rule, validate};
use netray_model::{CheckResult, Status};
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::{Value, json};
use url::Url;

const SPECTRA_JSON: &str = include_str!("../../../tests/fixtures/contracts/spectra-inspect.json");

fn fixture(mutate: impl FnOnce(&mut Value)) -> InspectResponse {
    let mut v: Value = serde_json::from_str(SPECTRA_JSON).unwrap();
    mutate(&mut v);
    serde_json::from_value(v).unwrap()
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

fn block<'a>(blocks: &'a [Block], id: &str) -> &'a Block {
    blocks
        .iter()
        .find(|b| b.id.to_string() == id)
        .unwrap_or_else(|| panic!("no block {id}: {blocks:?}"))
}

fn fields<'a>(blocks: &'a [Block], id: &str) -> &'a [(String, Ev)] {
    match &block(blocks, id).shape {
        Shape::Fields(f) => f,
        other => panic!("{id} must be fields, got {other:?}"),
    }
}

fn field<'a>(blocks: &'a [Block], id: &str, key: &str) -> Option<&'a Ev> {
    fields(blocks, id)
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
}

/// The rows of a table block as column-name/value pairs.
fn rows(blocks: &[Block], id: &str) -> Vec<Vec<(String, Ev)>> {
    let Shape::Table { columns, rows, .. } = &block(blocks, id).shape else {
        panic!("{id} must be a table");
    };
    rows.iter()
        .map(|row| columns.iter().cloned().zip(row.iter().cloned()).collect())
        .collect()
}

fn cell(row: &[(String, Ev)], column: &str) -> Ev {
    row.iter()
        .find(|(c, _)| c == column)
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| panic!("no column {column}"))
}

fn text(s: &str) -> Ev {
    Ev::Text(s.into())
}

fn task(final_url: &str, status: u16, headers: HeaderMap) -> TaskResult {
    TaskResult {
        final_url: final_url.into(),
        status,
        http_version: "HTTP/1.1".into(),
        headers,
        redirects: vec![],
        redirect_limit_reached: false,
        error: None,
    }
}

#[test]
fn repeated_set_cookie_keeps_one_row_per_value() {
    let mut resp = fixture(|_| {});
    resp.raw_headers = vec![
        ("set-cookie".into(), "a=1; Secure".into()),
        ("content-type".into(), "text/html".into()),
        ("set-cookie".into(), "b=2; Secure".into()),
    ];
    let (_, blocks) = run(&resp);
    let headers = rows(&blocks, "http.headers");
    let first = headers
        .iter()
        .find(|r| cell(r, "name") == text("set-cookie"))
        .unwrap_or_else(|| panic!("{headers:?}"));
    let second = headers
        .iter()
        .find(|r| cell(r, "name") == text("set-cookie#2"))
        .unwrap_or_else(|| panic!("{headers:?}"));
    assert_eq!(cell(first, "value"), text("a=1; Secure"));
    assert_eq!(cell(second, "value"), text("b=2; Secure"));
}

#[test]
fn assembled_response_keeps_raw_headers_and_the_port80_probe() {
    let mut https = HeaderMap::new();
    https.append("Set-Cookie", HeaderValue::from_static("a=1"));
    https.append("Set-Cookie", HeaderValue::from_static("b=2"));
    let mut port80 = HeaderMap::new();
    port80.insert("location", HeaderValue::from_static("https://example.com/"));
    let mut cors = HeaderMap::new();
    cors.insert("access-control-allow-origin", HeaderValue::from_static("*"));

    let resp = assemble_response(
        &Url::parse("https://example.com/").unwrap(),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)), 443),
        InspectResult {
            https: task("https://example.com/", 200, https),
            http_upgrade: Some(task("http://example.com/", 301, port80)),
            cors: task("https://example.com/", 200, cors),
        },
        EnrichmentData::default(),
        None,
        1,
    );
    assert_eq!(
        resp.raw_headers,
        vec![
            ("set-cookie".to_string(), "a=1".to_string()),
            ("set-cookie".to_string(), "b=2".to_string()),
        ]
    );
    assert_eq!(
        resp.headers.get("set-cookie").map(String::as_str),
        Some("a=1")
    );

    let (_, blocks) = run(&resp);
    let probe = "http.port80_probe";
    assert_eq!(
        field(&blocks, probe, "url"),
        Some(&text("http://example.com/"))
    );
    assert_eq!(field(&blocks, probe, "status_code"), Some(&Ev::Int(301)));
    assert_eq!(
        field(&blocks, probe, "location"),
        Some(&text("https://example.com/"))
    );
    assert_eq!(
        field(&blocks, "http.cors", "allow_origin"),
        Some(&text("*"))
    );
}

#[test]
fn port80_refused_fails_https_redirect_against_the_probe() {
    let resp = fixture(|v| {
        v["http_upgrade"] = json!({
            "redirects_to_https": false,
            "status_code": 0,
            "same_host": false,
            "message": "HTTP port 80 returned status 0",
            "redirects": [],
            "error": {"kind": "refused", "message": "connection refused"},
        });
    });
    let (checks, blocks) = run(&resp);
    let c = checks
        .iter()
        .find(|c| c.id.to_string() == "http.https_redirect")
        .unwrap();
    assert_eq!(c.status, Status::Fail);
    let refs: Vec<String> = c.evidence.iter().map(|b| b.to_string()).collect();
    assert_eq!(refs, vec!["http.port80_probe"]);
    assert!(blocks.iter().all(|b| b.id.to_string() != "http.port80"));
    let probe = "http.port80_probe";
    assert_eq!(field(&blocks, probe, "error_kind"), Some(&text("refused")));
    assert_eq!(field(&blocks, probe, "status_code"), None);
    assert_eq!(field(&blocks, probe, "redirects_to_https"), None);
}

#[test]
fn fixture_redirect_without_hops_emits_the_port80_probe() {
    let (_, blocks) = run(&fixture(|_| {}));
    let probe = "http.port80_probe";
    assert_eq!(
        field(&blocks, probe, "redirects_to_https"),
        Some(&Ev::Bool(true))
    );
    assert_eq!(field(&blocks, probe, "status_code"), Some(&Ev::Int(301)));
    assert_eq!(
        field(&blocks, probe, "url"),
        None,
        "a JSON replay has no URL"
    );
    assert_eq!(field(&blocks, probe, "error_kind"), None);
}

#[test]
fn cookies_of_one_name_on_two_paths_give_two_rows() {
    let resp = fixture(|v| {
        v["cookies"] = json!([
            {"name": "sid", "secure": true, "httponly": true, "samesite": "lax", "path": "/a"},
            {"name": "sid", "secure": false, "httponly": false, "path": "/b"},
        ]);
    });
    let (_, blocks) = run(&resp);
    let cookies = rows(&blocks, "http.cookies");
    assert_eq!(cookies.len(), 2, "{cookies:?}");
    assert_eq!(cell(&cookies[0], "cookie"), text("sid; ; /a"));
    assert_eq!(cell(&cookies[1], "cookie"), text("sid; ; /b"));
    for row in &cookies {
        assert_eq!(cell(row, "name"), text("sid"));
    }
    assert_eq!(cell(&cookies[0], "same_site"), text("lax"));
    assert_eq!(cell(&cookies[1], "same_site"), text(""));
    assert_eq!(cell(&cookies[1], "secure"), Ev::Bool(false));
}

#[test]
fn cors_block_carries_the_raw_header_values() {
    let mut resp = fixture(|_| {});
    let (_, blocks) = run(&resp);
    assert_eq!(field(&blocks, "http.cors", "allow_origin"), None);
    assert_eq!(
        field(&blocks, "http.cors", "allow_credentials_header"),
        None
    );

    resp.cors.allow_origin = Some("https://evil.example.com".into());
    resp.cors.allow_credentials_header = Some("true".into());
    let (_, blocks) = run(&resp);
    assert_eq!(
        field(&blocks, "http.cors", "allow_origin"),
        Some(&text("https://evil.example.com"))
    );
    assert_eq!(
        field(&blocks, "http.cors", "allow_credentials_header"),
        Some(&text("true"))
    );
}
