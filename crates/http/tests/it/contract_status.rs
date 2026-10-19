// Contract: every HTTP catalogue check can report its worst measured status and can report
// `Unmeasured` with a finding. One table, two rows per `http.*` id in checks.toml.
//
// Spec: specs/features/v2-unmeasured/spec.md, requirements 13 and 16 (C2, C13).
//
// Seam: `netray_http::module::translate` over an `InspectResponse` built from the spectra
// fixture, as in probe_outcomes.rs. Assumed interface (absent today): a probe that failed carries
// `"error": {"kind": "timeout"|..., "message": ...}` on the response (HTTPS probe),
// `http_upgrade` (port 80) or `cors` (CORS probe).
//
// Worst measured status, verified against src/inspect/*.rs and src/quality/checks.rs:
//   https_redirect, hsts, cors, csp  Fail (csp: an enforced policy with a `[fail]` issue, csp.rs:37;
//                                    the missing-header case alone is Warn, csp.rs:22)
//   every other check                Warn (no Fail branch)
//
// Exceptions (ids without a row): none.

use std::collections::BTreeSet;

use netray_engine::SectionOutcome;
use netray_http::inspect::assembler::InspectResponse;
use netray_http::module::translate;
use netray_model::Status;
use serde_json::{Value, json};

const SPECTRA_JSON: &str =
    include_str!("../../../../tests/fixtures/contracts/spectra-inspect.json");

fn set_quality(v: &mut Value, name: &str, status: &str, message: &str) {
    let checks = v["quality"]["checks"].as_array_mut().unwrap();
    checks.retain(|c| c["name"] != name);
    checks.push(json!({"name": name, "label": name, "status": status, "message": message}));
}

fn timeout(message: &str) -> Value {
    json!({"kind": "timeout", "message": message})
}

/// The HTTPS fetch gave no response (as in probe_outcomes.rs `no_https_response`).
fn https_timeout(v: &mut Value) {
    v["status"] = json!(0);
    v["headers"] = json!({});
    v["error"] = timeout("https probe timed out");
    set_quality(
        v,
        "hsts",
        "fail",
        "Missing Strict-Transport-Security header",
    );
    set_quality(v, "csp", "warn", "No Content-Security-Policy header");
}

fn port80_timeout(v: &mut Value) {
    v["http_upgrade"] = json!({
        "redirects_to_https": false, "same_host": true, "message": "",
        "redirects": [], "status_code": null,
        "error": timeout("port 80 probe timed out")
    });
}

fn cors_timeout(v: &mut Value) {
    v["cors"]["error"] = timeout("cors probe timed out");
    v["cors"]["status"] = json!("pass");
    set_quality(v, "cors", "pass", "No CORS headers");
}

fn port80_no_redirect(v: &mut Value) {
    v["http_upgrade"] = json!({
        "redirects_to_https": false, "same_host": true, "message": "",
        "redirects": [], "status_code": 200
    });
}

struct Row {
    /// Catalogue id without the `http.` prefix.
    id: &'static str,
    worst: Status,
    mutate_worst: fn(&mut Value),
    mutate_unmeasured: fn(&mut Value),
}

macro_rules! quality_row {
    ($id:literal, $status:literal, $worst:expr, $unmeasured:expr) => {
        Row {
            id: $id,
            worst: $worst,
            mutate_worst: |v| set_quality(v, $id, $status, "worst case"),
            mutate_unmeasured: $unmeasured,
        }
    };
}

fn table() -> Vec<Row> {
    vec![
        Row {
            id: "https_redirect",
            worst: Status::Fail,
            mutate_worst: port80_no_redirect,
            mutate_unmeasured: port80_timeout,
        },
        quality_row!("hsts", "fail", Status::Fail, https_timeout),
        quality_row!("cors", "fail", Status::Fail, cors_timeout),
        quality_row!("csp", "fail", Status::Fail, https_timeout),
        quality_row!("cookie_secure", "warn", Status::Warn, https_timeout),
        quality_row!("x_frame_options", "warn", Status::Warn, https_timeout),
        quality_row!(
            "x_content_type_options",
            "warn",
            Status::Warn,
            https_timeout
        ),
        quality_row!("referrer_policy", "warn", Status::Warn, https_timeout),
        quality_row!("permissions_policy", "warn", Status::Warn, https_timeout),
        quality_row!("deprecated_headers", "warn", Status::Warn, https_timeout),
        quality_row!("info_leakage", "warn", Status::Warn, https_timeout),
        quality_row!("caching", "warn", Status::Warn, https_timeout),
    ]
}

fn run(mutate: fn(&mut Value), id: &str) -> (Status, Vec<String>) {
    let mut v: Value = serde_json::from_str(SPECTRA_JSON).unwrap();
    mutate(&mut v);
    let resp: InspectResponse = serde_json::from_value(v).unwrap();
    match translate(&resp) {
        SectionOutcome::Measured { checks, .. } => {
            let want = format!("http.{id}");
            let c = checks
                .iter()
                .find(|c| c.id.to_string() == want)
                .unwrap_or_else(|| panic!("no {want}"));
            (c.status, c.findings.clone())
        }
        other => panic!("expected Measured, got {other:?}"),
    }
}

#[test]
fn every_http_check_reports_worst_status_and_unmeasured() {
    let mut failures = Vec::new();
    for row in table() {
        let (got, _) = run(row.mutate_worst, row.id);
        if got != row.worst {
            failures.push(format!(
                "http.{}: worst row is {got:?}, want {:?}",
                row.id, row.worst
            ));
        }
        let (got, findings) = run(row.mutate_unmeasured, row.id);
        if got != Status::Unmeasured {
            failures.push(format!("http.{}: unmeasured row is {got:?}", row.id));
        } else if !findings.iter().any(|f| !f.trim().is_empty()) {
            failures.push(format!("http.{}: unmeasured without a finding", row.id));
        }
    }
    assert!(
        failures.is_empty(),
        "{} failing:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn table_ids_equal_catalogue_http_ids() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../engine/catalogue/checks.toml"
    );
    let toml: toml::Value =
        toml::from_str(&std::fs::read_to_string(path).expect("read checks.toml"))
            .expect("parse checks.toml");
    let catalogue: BTreeSet<String> = toml["check"]
        .as_array()
        .expect("[[check]] array")
        .iter()
        .filter_map(|c| c["id"].as_str())
        .filter_map(|id| id.strip_prefix("http."))
        .map(str::to_owned)
        .collect();
    let covered: BTreeSet<String> = table().iter().map(|r| r.id.to_owned()).collect();
    // Exceptions: none for HTTP.
    assert_eq!(
        covered, catalogue,
        "table ids must equal the http.* catalogue ids"
    );
}
