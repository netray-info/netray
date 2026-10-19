// V2 unmeasured, Phase 4 (criteria C14-C16): what the landing answer is worth.
//   C14: port 80 answered 301 to https, a later hop timed out -> https_redirect Pass.
//   C15 (operator decision 2026-10-12): HTTPS probe refused/reset/tls -> the ten response
//        checks Fail with "no HTTPS response"; cors with cors.error refused stays NotApplicable.
//   C16: HTTPS probe timeout -> the ten stay Unmeasured (regression pin).
// Fixture helpers are copied from probe_outcomes.rs.

use netray_engine::SectionOutcome;
use netray_http::inspect::assembler::InspectResponse;
use netray_http::module::translate;
use netray_model::{CheckResult, Status};
use serde_json::{Value, json};

const SPECTRA_JSON: &str =
    include_str!("../../../../tests/fixtures/contracts/spectra-inspect.json");

const RESPONSE_CHECKS: [&str; 10] = [
    "hsts",
    "csp",
    "x_frame_options",
    "x_content_type_options",
    "referrer_policy",
    "permissions_policy",
    "cookie_secure",
    "caching",
    "deprecated_headers",
    "info_leakage",
];

fn error(kind: &str, message: &str) -> Value {
    json!({"kind": kind, "message": message})
}

fn set_quality(v: &mut Value, name: &str, status: &str, message: &str) {
    let checks = v["quality"]["checks"].as_array_mut().unwrap();
    checks.retain(|c| c["name"] != name);
    checks.push(json!({"name": name, "label": name, "status": status, "message": message}));
}

fn no_https_response(v: &mut Value, err: Value) {
    v["status"] = json!(0);
    v["headers"] = json!({});
    v["error"] = err;
    set_quality(
        v,
        "hsts",
        "fail",
        "Missing Strict-Transport-Security header",
    );
    set_quality(v, "csp", "warn", "No Content-Security-Policy header");
}

struct Row {
    name: &'static str,
    mutate: fn(&mut Value),
    want: Vec<(&'static str, Status)>,
    /// Each of these checks must carry a finding containing the text (lowercased).
    finding: Vec<(&'static str, &'static str)>,
}

fn all_ten(status: Status) -> Vec<(&'static str, Status)> {
    RESPONSE_CHECKS.iter().map(|c| (*c, status)).collect()
}

fn no_response_findings() -> Vec<(&'static str, &'static str)> {
    RESPONSE_CHECKS
        .iter()
        .map(|c| (*c, "no https response"))
        .collect()
}

fn refused_with_cors(v: &mut Value, kind: &str) {
    no_https_response(v, error(kind, "https probe failed"));
    v["cors"]["error"] = error("refused", "connection refused");
}

fn rows() -> Vec<Row> {
    let mut refused = all_ten(Status::Fail);
    refused.push(("cors", Status::NotApplicable));
    vec![
        Row {
            name: "C14 port-80 301 then later hop timeout",
            mutate: |v| {
                v["http_upgrade"] = json!({
                    "redirects_to_https": true, "same_host": false, "message": "",
                    "status_code": 301,
                    "redirects": [
                        {"url": "http://example.com/", "status": 301,
                         "location": "https://www.example.com/", "http_version": ""},
                        {"url": "https://www.example.com/", "status": 302,
                         "location": "https://www.example.com/home", "http_version": ""}
                    ],
                    "error": error("timeout", "later hop timed out")
                });
            },
            want: vec![("https_redirect", Status::Pass)],
            finding: vec![],
        },
        Row {
            name: "C15 https refused",
            mutate: |v| refused_with_cors(v, "refused"),
            want: refused.clone(),
            finding: no_response_findings(),
        },
        Row {
            name: "C15 https reset",
            mutate: |v| refused_with_cors(v, "reset"),
            want: refused.clone(),
            finding: no_response_findings(),
        },
        Row {
            name: "C15 https tls",
            mutate: |v| refused_with_cors(v, "tls"),
            want: refused,
            finding: no_response_findings(),
        },
        Row {
            name: "C16 https timeout stays unmeasured",
            mutate: |v| no_https_response(v, error("timeout", "https probe timed out")),
            want: all_ten(Status::Unmeasured),
            finding: vec![],
        },
    ]
}

fn checks_of(outcome: SectionOutcome) -> Vec<CheckResult> {
    match outcome {
        SectionOutcome::Measured { checks, .. } => checks,
        other => panic!("expected Measured, got {other:?}"),
    }
}

#[test]
fn probe_outcomes_landing_table() {
    let mut failures = Vec::new();
    for row in rows() {
        let mut v: Value = serde_json::from_str(SPECTRA_JSON).unwrap();
        (row.mutate)(&mut v);
        let resp: InspectResponse = serde_json::from_value(v).unwrap();
        let checks = checks_of(translate(&resp));
        let get = |name: &str| {
            checks
                .iter()
                .find(|c| c.id.to_string() == format!("http.{name}"))
                .unwrap_or_else(|| panic!("no http.{name}"))
        };
        for (name, status) in &row.want {
            let got = get(name).status;
            if got != *status {
                failures.push(format!(
                    "{}: http.{name} is {got:?}, want {status:?}",
                    row.name
                ));
            }
        }
        for (name, text) in &row.finding {
            let c = get(name);
            if !c.findings.iter().any(|f| f.to_lowercase().contains(text)) {
                failures.push(format!(
                    "{}: http.{name} findings {:?} lack {text:?}",
                    row.name, c.findings
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failing:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
