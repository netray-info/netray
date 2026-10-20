// v2-tls-versions-coop, Phase 3 (requirement 10, criteria C18, C19, C4 tail): the HTTP
// translation emits `http.coop`, a Cross-Origin-Opener-Policy check over the HTTPS response.
// C18: same-origin, same-origin-allow-popups (with parameters) and noopener-allow-popups pass;
//      unsafe-none and a missing header warn. Evidence references http.headers and http.isolation.
// C19: a refused HTTPS probe fails ("no HTTPS response"), a timed-out one is unmeasured.
// C4 tail: the translation emits exactly 13 checks, `http.coop` among them.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use netray_engine::SectionOutcome;
use netray_http::inspect::assembler::{ProbeError, ProbeErrorKind};
use netray_http::inspect::{EnrichmentData, InspectResult, TaskResult, assemble_response};
use netray_http::module::translate;
use netray_model::{CheckResult, Status};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use url::Url;

fn task(headers: HeaderMap, error: Option<ProbeError>) -> TaskResult {
    TaskResult {
        final_url: "https://example.com/".into(),
        status: if error.is_some() { 0 } else { 200 },
        http_version: "HTTP/1.1".into(),
        headers,
        redirects: vec![],
        redirect_limit_reached: false,
        error,
    }
}

fn run(pairs: &[(&str, &str)], https_error: Option<ProbeErrorKind>) -> Vec<CheckResult> {
    let mut https = HeaderMap::new();
    for (k, v) in pairs {
        https.append(
            HeaderName::from_bytes(k.as_bytes()).unwrap(),
            HeaderValue::from_str(v).unwrap(),
        );
    }
    let error = https_error.map(|kind| ProbeError {
        kind,
        message: "probe failed".into(),
    });
    let resp = assemble_response(
        &Url::parse("https://example.com/").unwrap(),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)), 443),
        InspectResult {
            https: task(https, error),
            http_upgrade: None,
            cors: task(HeaderMap::new(), None),
        },
        EnrichmentData::default(),
        None,
        1,
    );
    match translate(&resp) {
        SectionOutcome::Measured { checks, .. } => checks,
        other => panic!("expected Measured, got {other:?}"),
    }
}

fn coop(checks: &[CheckResult]) -> &CheckResult {
    checks
        .iter()
        .find(|c| c.id.to_string() == "http.coop")
        .unwrap_or_else(|| {
            panic!(
                "no http.coop in {:?}",
                checks.iter().map(|c| c.id.to_string()).collect::<Vec<_>>()
            )
        })
}

#[test]
fn coop_check_table() {
    let hdr = "cross-origin-opener-policy";
    let rows: Vec<(&str, Vec<(&str, &str)>, Status)> = vec![
        ("same-origin", vec![(hdr, "same-origin")], Status::Pass),
        (
            "allow-popups with parameter",
            vec![(hdr, "same-origin-allow-popups; report-to=\"r\"")],
            Status::Pass,
        ),
        (
            "noopener-allow-popups",
            vec![(hdr, "noopener-allow-popups")],
            Status::Pass,
        ),
        ("unsafe-none", vec![(hdr, "unsafe-none")], Status::Warn),
        ("no header", vec![], Status::Warn),
    ];
    let mut failures = Vec::new();
    for (name, pairs, want) in rows {
        let checks = run(&pairs, None);
        let c = coop(&checks);
        if c.status != want {
            failures.push(format!("{name}: status {:?}, want {want:?}", c.status));
        }
        let refs: Vec<String> = c.evidence.iter().map(|b| b.to_string()).collect();
        for id in ["http.headers", "http.isolation"] {
            if !refs.iter().any(|r| r == id) {
                failures.push(format!("{name}: evidence {refs:?} lacks {id}"));
            }
        }
        if pairs.is_empty()
            && !c
                .findings
                .iter()
                .any(|f| f.to_lowercase().contains("cross-origin-opener-policy"))
        {
            failures.push(format!(
                "{name}: findings {:?} lack the header name",
                c.findings
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn coop_check_follows_the_https_probe() {
    let checks = run(&[], Some(ProbeErrorKind::Refused));
    let c = coop(&checks);
    assert_eq!(c.status, Status::Fail);
    assert!(
        c.findings
            .iter()
            .any(|f| f.starts_with("no HTTPS response")),
        "{:?}",
        c.findings
    );

    let checks = run(&[], Some(ProbeErrorKind::Timeout));
    assert_eq!(coop(&checks).status, Status::Unmeasured);
}

#[test]
fn translation_emits_thirteen_checks_with_coop() {
    let checks = run(&[], None);
    let mut ids: Vec<String> = checks.iter().map(|c| c.id.to_string()).collect();
    ids.sort();
    assert_eq!(
        ids,
        [
            "http.caching",
            "http.cookie_secure",
            "http.coop",
            "http.cors",
            "http.csp",
            "http.deprecated_headers",
            "http.hsts",
            "http.https_redirect",
            "http.info_leakage",
            "http.permissions_policy",
            "http.referrer_policy",
            "http.x_content_type_options",
            "http.x_frame_options",
        ]
    );
}
