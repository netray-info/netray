// V2 http checks, Phase 2: the `http.cdn` and `http.isolation` evidence blocks (R10, R11).
// `http.cdn` is context only: no check references it. `http.isolation` is referenced by
// `http.coop` alone (v2-tls-versions-coop, requirement 10).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use netray_engine::SectionOutcome;
use netray_http::inspect::{EnrichmentData, InspectResult, TaskResult, assemble_response};
use netray_http::module::{REGISTER, translate};
use netray_model::CheckResult;
use netray_model::evidence::{Block, Shape, Value as Ev, check_evidence_rule, validate};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use url::Url;

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

fn run(pairs: &[(&str, &str)]) -> (Vec<CheckResult>, Vec<Block>) {
    let mut https = HeaderMap::new();
    for (k, v) in pairs {
        https.append(
            HeaderName::from_bytes(k.as_bytes()).unwrap(),
            HeaderValue::from_str(v).unwrap(),
        );
    }
    let resp = assemble_response(
        &Url::parse("https://example.com/").unwrap(),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)), 443),
        InspectResult {
            https: task("https://example.com/", 200, https),
            http_upgrade: None,
            cors: task("https://example.com/", 200, HeaderMap::new()),
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

fn fields<'a>(blocks: &'a [Block], id: &str) -> Option<&'a [(String, Ev)]> {
    blocks
        .iter()
        .find(|b| b.id.to_string() == id)
        .map(|b| match &b.shape {
            Shape::Fields(f) => f.as_slice(),
            other => panic!("{id} must be fields, got {other:?}"),
        })
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

fn assert_no_check_references(checks: &[CheckResult], id: &str) {
    assert_only_checks_reference(checks, id, &[]);
}

fn assert_only_checks_reference(checks: &[CheckResult], id: &str, want: &[&str]) {
    let referencing: Vec<String> = checks
        .iter()
        .filter(|c| c.evidence.iter().any(|b| b.to_string() == id))
        .map(|c| c.id.to_string())
        .collect();
    assert_eq!(referencing, want, "checks referencing {id}");
}

#[test]
fn cdn_block_names_provider_cache_status_and_indicators() {
    let (checks, blocks) = run(&[("cf-ray", "abc-IAD"), ("cf-cache-status", "HIT")]);
    let cdn = fields(&blocks, "http.cdn").unwrap_or_else(|| panic!("no http.cdn: {blocks:?}"));
    assert_eq!(values(cdn, "provider"), vec!["Cloudflare"]);
    assert_eq!(values(cdn, "cache_status"), vec!["HIT"]);
    assert_eq!(values(cdn, "indicator"), vec!["cf-cache-status", "cf-ray"]);
    assert_no_check_references(&checks, "http.cdn");

    let (checks, blocks) = run(&[("content-type", "text/html")]);
    assert!(fields(&blocks, "http.cdn").is_none(), "{blocks:?}");
    assert_no_check_references(&checks, "http.cdn");
}

#[test]
fn isolation_block_carries_values_and_missing_headers() {
    let (checks, blocks) = run(&[("cross-origin-opener-policy", "same-origin")]);
    let iso = fields(&blocks, "http.isolation")
        .unwrap_or_else(|| panic!("no http.isolation: {blocks:?}"));
    assert_eq!(values(iso, "coop"), vec!["same-origin"]);
    assert!(values(iso, "coep").is_empty());
    assert!(values(iso, "corp").is_empty());
    assert_eq!(
        values(iso, "missing"),
        vec![
            "cross-origin-embedder-policy",
            "cross-origin-resource-policy"
        ]
    );
    assert_only_checks_reference(&checks, "http.isolation", &["http.coop"]);

    let (checks, blocks) = run(&[
        ("cross-origin-opener-policy", "same-origin"),
        ("cross-origin-embedder-policy", "require-corp"),
        ("cross-origin-resource-policy", "same-site"),
    ]);
    let iso = fields(&blocks, "http.isolation")
        .unwrap_or_else(|| panic!("no http.isolation: {blocks:?}"));
    assert_eq!(values(iso, "coop"), vec!["same-origin"]);
    assert_eq!(values(iso, "coep"), vec!["require-corp"]);
    assert_eq!(values(iso, "corp"), vec!["same-site"]);
    assert!(values(iso, "missing").is_empty());
    assert_only_checks_reference(&checks, "http.isolation", &["http.coop"]);
}
