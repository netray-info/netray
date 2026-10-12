// V2 unmeasured, Phase 4 (requirement 12, criteria C3-C12): `translate` tells a probe that
// gave no answer (timeout, policy-blocked hop) from a probe that answered "no" (refused, reset,
// 200 without redirect), and a missing HTTPS response from a missing measurement.
//
// Assumed interface (absent today): a probe that failed carries
// `"error": {"kind": "timeout"|"refused"|"reset"|"tls"|"blocked", "message": "<text>"}` on
//   - the response itself (top level) for the HTTPS probe,
//   - `http_upgrade` for the port-80 probe,
//   - `cors` for the CORS probe.
// A "blocked" message names the blocked hop.

use netray_engine::SectionOutcome;
use netray_http::inspect::assembler::InspectResponse;
use netray_http::translate;
use netray_model::{CheckResult, Status};
use serde_json::{Value, json};

const SPECTRA_JSON: &str = include_str!("../../../tests/fixtures/contracts/spectra-inspect.json");

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

/// What today's request path leaves behind when the HTTPS fetch gave no response.
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

fn cors_any_origin(v: &mut Value) {
    v["cors"] = json!({
        "allows_any_origin": true, "allows_credentials": false, "reflects_origin": false,
        "status": "warn", "message": "Access-Control-Allow-Origin: *"
    });
    set_quality(v, "cors", "warn", "Access-Control-Allow-Origin: *");
}

fn port80(v: &mut Value, redirects: bool, err: Option<Value>) {
    v["http_upgrade"] = json!({
        "redirects_to_https": redirects, "same_host": true, "message": "",
        "redirects": [], "status_code": if redirects { json!(301) } else { json!(200) }
    });
    if let Some(e) = err {
        v["http_upgrade"]["error"] = e;
        v["http_upgrade"]["status_code"] = Value::Null;
    }
}

struct Row {
    name: &'static str,
    mutate: fn(&mut Value),
    /// (check name, expected status)
    want: Vec<(&'static str, Status)>,
    /// Each of these checks must carry a finding containing the text (lowercased).
    finding: Vec<(&'static str, &'static str)>,
    /// Checks that must not be Unmeasured.
    measured: Vec<&'static str>,
}

fn all_ten(status: Status) -> Vec<(&'static str, Status)> {
    RESPONSE_CHECKS.iter().map(|c| (*c, status)).collect()
}

fn rows() -> Vec<Row> {
    let mut c7 = all_ten(Status::Unmeasured);
    c7.push(("cors", Status::Warn));
    let mut c8_measured: Vec<&str> = RESPONSE_CHECKS.to_vec();
    c8_measured.push("https_redirect");
    vec![
        Row {
            name: "C3 port-80 timeout",
            mutate: |v| port80(v, false, Some(error("timeout", "port 80 probe timed out"))),
            want: vec![("https_redirect", Status::Unmeasured)],
            finding: vec![("https_redirect", "port 80")],
            measured: vec![],
        },
        Row {
            name: "C4 port-80 refused",
            mutate: |v| port80(v, false, Some(error("refused", "connection refused"))),
            want: vec![("https_redirect", Status::Fail)],
            finding: vec![],
            measured: vec![],
        },
        Row {
            name: "C5 port-80 reset after accept",
            mutate: |v| port80(v, false, Some(error("reset", "connection reset by peer"))),
            want: vec![("https_redirect", Status::Fail)],
            finding: vec![],
            measured: vec![],
        },
        Row {
            name: "C6 port-80 200 without redirect",
            mutate: |v| port80(v, false, None),
            want: vec![("https_redirect", Status::Fail)],
            finding: vec![],
            measured: vec![],
        },
        Row {
            name: "C7 https timeout, cors answers *",
            mutate: |v| {
                no_https_response(v, error("timeout", "https probe timed out"));
                cors_any_origin(v);
            },
            want: c7,
            finding: vec![("hsts", "timed out"), ("csp", "timed out")],
            measured: vec![],
        },
        Row {
            name: "C8 https 200, cors timeout",
            mutate: |v| {
                v["cors"]["error"] = error("timeout", "cors probe timed out");
                v["cors"]["status"] = json!("pass");
                set_quality(v, "cors", "pass", "No CORS headers");
            },
            want: vec![("cors", Status::Unmeasured)],
            finding: vec![],
            measured: c8_measured,
        },
        Row {
            name: "C9 https redirect to blocked address",
            mutate: |v| {
                no_https_response(
                    v,
                    error("blocked", "redirect to 10.0.0.1 blocked by fetch policy"),
                );
                v["redirects"] = json!([{"url": "https://example.com/", "status": 301,
                    "location": "https://10.0.0.1/", "http_version": ""}]);
            },
            want: all_ten(Status::Unmeasured),
            finding: vec![("hsts", "10.0.0.1"), ("csp", "10.0.0.1")],
            measured: vec![],
        },
        Row {
            name: "C10 https response without CSP",
            mutate: |v| {
                set_quality(v, "csp", "warn", "No Content-Security-Policy header");
                v["security"]["csp"]["status"] = json!("warn");
            },
            want: vec![("csp", Status::Warn)],
            finding: vec![],
            measured: vec![],
        },
        Row {
            name: "C11 https response without HSTS",
            mutate: |v| {
                set_quality(
                    v,
                    "hsts",
                    "fail",
                    "Missing Strict-Transport-Security header",
                );
                v["security"]["hsts"]["status"] = json!("fail");
            },
            want: vec![("hsts", Status::Fail)],
            finding: vec![],
            measured: vec![],
        },
        Row {
            name: "C12 https connection refused",
            mutate: |v| no_https_response(v, error("refused", "connection refused")),
            want: all_ten(Status::Fail),
            finding: vec![
                ("hsts", "no https response"),
                ("info_leakage", "no https response"),
            ],
            measured: vec![],
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
fn probe_outcomes_table() {
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
        for name in &row.measured {
            if get(name).status == Status::Unmeasured {
                failures.push(format!("{}: http.{name} must be measured", row.name));
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
