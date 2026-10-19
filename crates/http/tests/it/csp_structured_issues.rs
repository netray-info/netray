//! CSP analysis yields structured issues (directive + severity); the status follows
//! from severities and the Report-Only marker, V1 issue texts and order unchanged.

use indexmap::IndexMap;
use netray_http::inspect::csp::{CspIssue, CspSeverity, analyze_csp, issues};
use netray_http::quality::types::CheckStatus;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

fn headers(name: &'static str, value: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(
        HeaderName::from_static(name),
        HeaderValue::from_str(value).unwrap(),
    );
    h
}

fn directives(csp: &str) -> IndexMap<String, Vec<String>> {
    analyze_csp(&headers("content-security-policy", csp)).directives
}

#[test]
fn missing_default_src_is_a_fail_issue_naming_default_src() {
    let csp = "script-src 'self'";
    let found: Vec<CspIssue> = issues(&directives(csp));
    assert!(
        found.iter().any(|i| i.directive == "default-src"
            && i.severity == CspSeverity::Fail
            && i.message == "Missing default-src directive"),
        "got {found:?}"
    );
    let report = analyze_csp(&headers("content-security-policy", csp));
    assert_eq!(report.status, CheckStatus::Fail);
}

#[test]
fn unsafe_inline_script_src_is_one_warn_issue() {
    let csp = "default-src 'self'; script-src 'unsafe-inline'; object-src 'none'; base-uri 'self'";
    let found = issues(&directives(csp));
    assert_eq!(
        found,
        vec![CspIssue {
            directive: "script-src".to_string(),
            severity: CspSeverity::Warn,
            message: "script-src contains 'unsafe-inline'".to_string(),
        }]
    );
    let report = analyze_csp(&headers("content-security-policy", csp));
    assert_eq!(report.status, CheckStatus::Warn);
}

#[test]
fn v1_issue_texts_equal_structured_messages_in_order() {
    for csp in [
        "script-src 'self'",
        "default-src 'self'; script-src 'unsafe-inline' 'unsafe-eval' data:; style-src 'unsafe-inline'",
        "default-src 'self'; script-src *.com",
        "default-src 'self'; object-src 'none'; base-uri 'self'",
    ] {
        let report = analyze_csp(&headers("content-security-policy", csp));
        let messages: Vec<String> = issues(&report.directives)
            .into_iter()
            .map(|i| i.message)
            .collect();
        assert_eq!(report.issues, messages, "csp: {csp}");
    }
    let report = analyze_csp(&headers("content-security-policy", "script-src 'self'"));
    assert_eq!(report.issues[0], "Missing default-src directive");
}

#[test]
fn report_only_without_issues_is_warn() {
    let report = analyze_csp(&headers(
        "content-security-policy-report-only",
        "default-src 'self'; object-src 'none'; base-uri 'self'",
    ));
    assert!(report.issues.is_empty(), "got {:?}", report.issues);
    assert!(report.report_only);
    assert_eq!(report.status, CheckStatus::Warn);
}
