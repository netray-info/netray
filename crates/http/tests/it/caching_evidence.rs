// V2 http checks, R9: `http.caching` is a Fields block of the cache headers the HTTPS probe
// answered, each only when known; `http.caching` check references it beside `http.headers`.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use netray_engine::SectionOutcome;
use netray_http::inspect::{EnrichmentData, InspectResult, TaskResult, assemble_response};
use netray_http::module::{REGISTER, translate};
use netray_model::CheckResult;
use netray_model::evidence::{Block, Shape, Value as Ev, check_evidence_rule, validate};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
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

fn run(pairs: &[(&'static str, &'static str)]) -> (Vec<CheckResult>, Vec<Block>) {
    let mut h = HeaderMap::new();
    h.insert("content-type", HeaderValue::from_static("text/html"));
    for (k, v) in pairs {
        h.insert(HeaderName::from_static(k), HeaderValue::from_static(v));
    }
    let resp = assemble_response(
        &Url::parse("https://example.com/").unwrap(),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)), 443),
        InspectResult {
            https: task(200, h.clone()),
            http_upgrade: None,
            cors: task(200, h),
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

fn caching(blocks: &[Block]) -> Option<&[(String, Ev)]> {
    blocks
        .iter()
        .find(|b| b.id.to_string() == "http.caching")
        .map(|b| match &b.shape {
            Shape::Fields(f) => f.as_slice(),
            other => panic!("http.caching must be fields, got {other:?}"),
        })
}

fn all<'a>(f: &'a [(String, Ev)], key: &str) -> Vec<&'a Ev> {
    f.iter().filter(|(k, _)| k == key).map(|(_, v)| v).collect()
}

fn check_evidence(checks: &[CheckResult]) -> Vec<String> {
    let c = checks
        .iter()
        .find(|c| c.id.to_string() == "http.caching")
        .expect("http.caching check");
    c.evidence.iter().map(|b| b.to_string()).collect()
}

fn t(s: &str) -> Ev {
    Ev::Text(s.into())
}

#[test]
fn caching_block_holds_the_known_cache_headers() {
    let (checks, blocks) = run(&[
        ("cache-control", "public, max-age=3600, must-revalidate"),
        ("etag", "W/\"abc\""),
        ("last-modified", "Mon, 01 Jan 2024 00:00:00 GMT"),
        ("vary", "Accept-Encoding, Accept-Language"),
        ("age", "42"),
    ]);
    let f = caching(&blocks).unwrap_or_else(|| panic!("no http.caching block: {blocks:?}"));
    assert_eq!(
        all(f, "directive"),
        vec![&t("public"), &t("max-age=3600"), &t("must-revalidate")]
    );
    assert_eq!(all(f, "max_age"), vec![&Ev::Int(3600)]);
    assert_eq!(all(f, "etag"), vec![&t("W/\"abc\"")]);
    assert_eq!(all(f, "etag_weak"), vec![&Ev::Bool(true)]);
    assert_eq!(
        all(f, "vary"),
        vec![&t("Accept-Encoding"), &t("Accept-Language")]
    );
    assert_eq!(all(f, "age"), vec![&Ev::Int(42)]);
    let lm = all(f, "last_modified");
    assert_eq!(lm.len(), 1, "{f:?}");
    let Ev::Time(s) = lm[0] else {
        panic!("last_modified must be Time: {lm:?}")
    };
    let parsed = chrono::DateTime::parse_from_rfc3339(s).expect("RFC 3339");
    assert_eq!(
        parsed.with_timezone(&chrono::Utc).timestamp(),
        1_704_067_200
    );
    assert_eq!(
        check_evidence(&checks),
        vec!["http.headers", "http.caching"]
    );
}

#[test]
fn unparsable_last_modified_is_omitted_and_no_headers_means_an_empty_block() {
    let (_, blocks) = run(&[("etag", "\"abc\""), ("last-modified", "yesterday")]);
    let f = caching(&blocks).unwrap_or_else(|| panic!("no http.caching block: {blocks:?}"));
    assert!(all(f, "last_modified").is_empty(), "{f:?}");
    assert_eq!(all(f, "etag_weak"), vec![&Ev::Bool(false)]);

    // The block stands whenever the probe answered, empty without cache headers.
    let (checks, blocks) = run(&[]);
    let f = caching(&blocks).unwrap_or_else(|| panic!("no http.caching block: {blocks:?}"));
    assert!(f.is_empty(), "{f:?}");
    assert_eq!(
        check_evidence(&checks),
        vec!["http.headers", "http.caching"]
    );
}
