// Referrer-Policy list grading (v2-http-checks R1, criteria C1, C6-C10): the effective policy is
// the last known token of the comma-separated value, trimmed, case-insensitive. Checked on the V1
// `security.referrer_policy` and on `http.referrer_policy` after `translate`.

use netray_engine::SectionOutcome;
use netray_http::inspect::assembler::InspectResponse;
use netray_http::inspect::security::analyze_security_headers;
use netray_http::module::translate;
use netray_http::quality::checks::run_checks;
use netray_http::quality::types::CheckStatus;
use netray_model::Status;
use reqwest::header::{HeaderMap, HeaderValue};

const SPECTRA_JSON: &str = include_str!("../../../tests/fixtures/contracts/spectra-inspect.json");

fn v2_status(value: &'static str) -> Status {
    let mut headers = HeaderMap::new();
    headers.insert("referrer-policy", HeaderValue::from_static(value));
    let report = analyze_security_headers(&headers);
    let mut resp: InspectResponse = serde_json::from_str(SPECTRA_JSON).unwrap();
    resp.security.referrer_policy = report.referrer_policy;
    resp.quality.checks = run_checks(&resp);
    let SectionOutcome::Measured { checks, .. } = translate(&resp) else {
        panic!("expected Measured");
    };
    checks
        .iter()
        .find(|c| c.id.to_string() == "http.referrer_policy")
        .expect("http.referrer_policy present")
        .status
}

#[test]
fn referrer_policy_list_is_graded_by_effective_policy() {
    // (criterion, header value, V1 status, V1 message, V2 status)
    let rows: [(&str, &'static str, CheckStatus, Option<&str>, Status); 5] = [
        (
            "C6",
            "origin-when-cross-origin, strict-origin-when-cross-origin",
            CheckStatus::Pass,
            None,
            Status::Pass,
        ),
        (
            "C7",
            "no-referrer, unsafe-url",
            CheckStatus::Warn,
            Some("Permissive policy: unsafe-url"),
            Status::Warn,
        ),
        (
            "C8",
            "strict-origin, made-up-policy",
            CheckStatus::Pass,
            None,
            Status::Pass,
        ),
        (
            "C9",
            "foo, bar",
            CheckStatus::Warn,
            Some("Unrecognized value: foo, bar"),
            Status::Warn,
        ),
        (
            "C10",
            " Strict-Origin ",
            CheckStatus::Pass,
            None,
            Status::Pass,
        ),
    ];
    for (id, value, v1_status, v1_message, v2) in rows {
        let mut headers = HeaderMap::new();
        headers.insert("referrer-policy", HeaderValue::from_static(value));
        let check = analyze_security_headers(&headers).referrer_policy;
        assert_eq!(check.status, v1_status, "{id}: V1 status for {value:?}");
        assert_eq!(
            check.message.as_deref(),
            v1_message,
            "{id}: V1 message for {value:?}"
        );
        assert_eq!(v2_status(value), v2, "{id}: V2 status for {value:?}");
    }
}
