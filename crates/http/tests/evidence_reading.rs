// V2 evidence, Phase 4 HTTP row, reader findings: every judged cookie keeps a row, an unknown hop
// HTTP version is omitted rather than published empty, and a redirect-limited HTTPS chain is
// part of `https_redirect`'s evidence.

use netray_engine::SectionOutcome;
use netray_http::inspect::assembler::InspectResponse;
use netray_http::module::{REGISTER, translate};
use netray_model::CheckResult;
use netray_model::evidence::{Block, Shape, Value as Ev, check_evidence_rule, validate};
use serde_json::{Value, json};

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

fn shape<'a>(blocks: &'a [Block], id: &str) -> &'a Shape {
    &blocks
        .iter()
        .find(|b| b.id.to_string() == id)
        .unwrap_or_else(|| panic!("no block {id}: {blocks:?}"))
        .shape
}

#[test]
fn identical_cookies_keep_a_row_each_with_an_ordinal_key() {
    let resp = fixture(|v| {
        v["cookies"] = json!([
            {"name": "sid", "secure": true, "httponly": true, "path": "/"},
            {"name": "sid", "secure": false, "httponly": false, "path": "/"},
            {"name": "sid", "secure": true, "httponly": false, "path": "/"},
        ]);
    });
    let (_, blocks) = run(&resp);
    let Shape::Table { columns, rows, .. } = shape(&blocks, "http.cookies") else {
        panic!("http.cookies must be a table");
    };
    let at = |c: &str| columns.iter().position(|x| x == c).unwrap();
    let keys: Vec<&Ev> = rows.iter().map(|r| &r[at("cookie")]).collect();
    assert_eq!(
        keys,
        [
            &Ev::Text("sid; ; /".into()),
            &Ev::Text("sid; ; /#2".into()),
            &Ev::Text("sid; ; /#3".into()),
        ]
    );
    let secure: Vec<&Ev> = rows.iter().map(|r| &r[at("secure")]).collect();
    assert_eq!(secure, [&Ev::Bool(true), &Ev::Bool(false), &Ev::Bool(true)]);
}

#[test]
fn unknown_hop_http_version_is_omitted() {
    let resp = fixture(|v| {
        v["redirects"] = json!([
            {"url": "https://example.com", "status": 301, "location": "https://example.com/", "http_version": ""},
            {"url": "https://example.com/a", "status": 302, "location": "https://example.com/", "http_version": "HTTP/2"},
        ]);
    });
    let (_, blocks) = run(&resp);
    let Shape::Chain(hops) = shape(&blocks, "http.redirects") else {
        panic!("http.redirects must be a chain");
    };
    let version = |hop: &[(String, Ev)]| {
        hop.iter()
            .find(|(k, _)| k == "http_version")
            .map(|(_, v)| v.clone())
    };
    assert_eq!(version(&hops[0]), None, "{:?}", hops[0]);
    assert_eq!(version(&hops[1]), Some(Ev::Text("HTTP/2".into())));
}

#[test]
fn redirect_limit_adds_the_https_chain_to_https_redirect() {
    let mut resp = fixture(|v| {
        v["redirects"] = json!([
            {"url": "https://example.com", "status": 301, "location": "https://example.com/", "http_version": "HTTP/2"},
        ]);
    });
    let (checks, _) = run(&resp);
    let refs = |checks: &[CheckResult]| -> Vec<String> {
        checks
            .iter()
            .find(|c| c.id.to_string() == "http.https_redirect")
            .unwrap()
            .evidence
            .iter()
            .map(|b| b.to_string())
            .collect()
    };
    assert_eq!(refs(&checks), ["http.port80", "http.port80_probe"]);

    resp.redirect_limit_reached = Some(10);
    let (checks, _) = run(&resp);
    assert_eq!(
        refs(&checks),
        ["http.port80", "http.port80_probe", "http.redirects"]
    );
}
