// V2 unmeasured, spec requirement 10, Phase 3 criteria C3 and C12-C16: a failed DNS lookup or a
// timed-out address is "could not check" (`Unmeasured`), never a pass or "not advertised".
//
// Assumed interface, absent today: `InspectResponse.dns.lookup_errors`, a list naming the lookups
// that errored (not answered definitively): "caa", "https", "tlsa". Per-address timeouts use the
// error code `HANDSHAKE_TIMEOUT` (as in `reachable_unmeasured.rs`); a refusal stays
// `HANDSHAKE_FAILED`.

use std::path::PathBuf;

use netray_engine::SectionOutcome;
use netray_model::{CheckResult, Status};
use netray_tls::inspection::InspectResponse;
use netray_tls::translate;
use serde_json::{Value, json};

const A: &str = "192.0.2.10";
const B: &str = "192.0.2.11";

fn golden() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts/tlsight-inspect.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn handshook(ip: &str) -> Value {
    let mut v = golden()["ports"][0]["ips"][0].clone();
    v["ip"] = json!(ip);
    v
}

fn failed(ip: &str, code: &str, message: &str) -> Value {
    json!({ "ip": ip, "ip_version": "v4", "error": { "code": code, "message": message } })
}

fn timed_out(ip: &str) -> Value {
    failed(ip, "HANDSHAKE_TIMEOUT", "handshake timed out")
}

fn refused(ip: &str) -> Value {
    failed(ip, "HANDSHAKE_FAILED", "connection refused")
}

/// The golden with the given addresses and the given lookups marked as errored. The summary
/// carries what today's code computes from an errored lookup: CAA pass, DANE skip.
fn translated(ips: Vec<Value>, lookup_errors: &[&str]) -> Vec<CheckResult> {
    let mut doc = golden();
    let port = doc["ports"][0].as_object_mut().unwrap();
    port.remove("quality");
    port.remove("consistency");
    port.insert("ips".into(), Value::Array(ips));
    doc["summary"]["checks"]["caa_compliant"] = json!("pass");
    doc["summary"]["checks"]["dane_valid"] = json!("skip");
    doc["dns"]["lookup_errors"] = json!(lookup_errors);
    let resp: InspectResponse = serde_json::from_value(doc).unwrap();
    match translate(&resp) {
        SectionOutcome::Measured { checks, .. } => checks,
        other => panic!("expected Measured, got {other:?}"),
    }
}

#[test]
fn lookup_errors_and_partial_handshakes_by_outcome() {
    // (criterion, addresses, errored lookups, check, expected status, finding required)
    let rows: Vec<(&str, Vec<Value>, Vec<&str>, &str, Status, bool)> = vec![
        (
            "C12",
            vec![handshook(A)],
            vec!["caa"],
            "caa_compliant",
            Status::Unmeasured,
            true,
        ),
        (
            "C13",
            vec![handshook(A)],
            vec![],
            "caa_compliant",
            Status::Pass,
            false,
        ),
        (
            "C3 ech",
            vec![handshook(A)],
            vec!["https"],
            "ech_advertised",
            Status::Unmeasured,
            true,
        ),
        (
            "C3 dane",
            vec![handshook(A)],
            vec!["tlsa"],
            "dane_valid",
            Status::Unmeasured,
            true,
        ),
        (
            "C14",
            vec![handshook(A)],
            vec![],
            "consistency",
            Status::NotApplicable,
            false,
        ),
        (
            "C15 consistency",
            vec![handshook(A), timed_out(B)],
            vec![],
            "consistency",
            Status::Unmeasured,
            true,
        ),
        (
            "C15 alpn",
            vec![handshook(A), timed_out(B)],
            vec![],
            "alpn_consistency",
            Status::Unmeasured,
            true,
        ),
        (
            "C16",
            vec![handshook(A), refused(B)],
            vec![],
            "consistency",
            Status::NotApplicable,
            false,
        ),
    ];

    let mut failures = Vec::new();
    for (criterion, ips, errors, name, want, needs_finding) in rows {
        let checks = translated(ips, &errors);
        let id = format!("tls.{name}");
        let c = checks
            .iter()
            .find(|c| c.id.to_string() == id)
            .unwrap_or_else(|| panic!("{criterion}: no CheckResult {id}"));
        if c.status != want {
            failures.push(format!(
                "{criterion}: {id} is {:?}, want {want:?}",
                c.status
            ));
        } else if needs_finding && c.findings.is_empty() {
            failures.push(format!("{criterion}: {id} is {want:?} without a finding"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
