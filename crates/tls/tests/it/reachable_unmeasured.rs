// V2 unmeasured, spec requirements 8 and 9, Phase 3 criteria C5-C11: `tls.tls_reachable` and the
// 20 other TLS checks when addresses time out, refuse, or cannot be tested from here.
// A timeout carries the error code `HANDSHAKE_TIMEOUT` (message "handshake timed out"); a refusal
// stays `HANDSHAKE_FAILED`; an unusable source address stays `NOT_TESTED_FROM_HERE`.

use std::path::PathBuf;

use netray_engine::SectionOutcome;
use netray_model::{CheckResult, Status};
use netray_tls::routes::InspectResponse;
use netray_tls::translate;
use serde_json::{Value, json};

const A: &str = "192.0.2.10";
const B: &str = "192.0.2.11";

#[derive(Clone, Copy)]
enum Addr {
    Valid,
    Timeout,
    Refused,
    NotHere,
}

enum Rest {
    /// The other 20 checks all carry this status.
    All(Status),
    /// The other 20 are NotApplicable with the finding "no TLS handshake".
    NoHandshake,
    /// Not asserted: measured on the valid address, or unspecified by the criterion.
    Unchecked,
}

fn golden() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts/tlsight-inspect.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn address(kind: Addr, ip: &str) -> Value {
    let error = |code: &str, message: &str| {
        json!({
            "ip": ip,
            "ip_version": "v4",
            "error": { "code": code, "message": message }
        })
    };
    match kind {
        Addr::Valid => {
            let mut v = golden()["ports"][0]["ips"][0].clone();
            v["ip"] = json!(ip);
            v
        }
        Addr::Timeout => error("HANDSHAKE_TIMEOUT", "handshake timed out"),
        Addr::Refused => error("HANDSHAKE_FAILED", "connection refused"),
        Addr::NotHere => error("NOT_TESTED_FROM_HERE", "Network is unreachable"),
    }
}

fn translated(kinds: [Addr; 2]) -> Vec<CheckResult> {
    let mut doc = golden();
    let port = doc["ports"][0].as_object_mut().unwrap();
    port.remove("quality");
    port.remove("consistency");
    port.insert(
        "ips".into(),
        Value::Array(vec![address(kinds[0], A), address(kinds[1], B)]),
    );
    let resp: InspectResponse = serde_json::from_value(doc).unwrap();
    match translate(&resp) {
        SectionOutcome::Measured { checks, .. } => checks,
        other => panic!("expected Measured, got {other:?}"),
    }
}

#[test]
fn tls_reachable_by_address_outcome() {
    use Addr::*;
    // (criterion, addresses, tls_reachable status, finding must name, rest)
    let rows: [(&str, [Addr; 2], Status, Option<&str>, Rest); 7] = [
        (
            "C5",
            [Timeout, Timeout],
            Status::Unmeasured,
            None,
            Rest::All(Status::Unmeasured),
        ),
        (
            "C6",
            [Refused, Refused],
            Status::Fail,
            None,
            Rest::NoHandshake,
        ),
        (
            "C7",
            [Timeout, Valid],
            Status::Pass,
            Some(A),
            Rest::Unchecked,
        ),
        ("C8", [Refused, Valid], Status::Pass, None, Rest::Unchecked),
        (
            "C9",
            [Refused, Timeout],
            Status::Fail,
            None,
            Rest::NoHandshake,
        ),
        (
            "C10",
            [NotHere, Timeout],
            Status::Unmeasured,
            None,
            Rest::Unchecked,
        ),
        (
            "C11",
            [NotHere, NotHere],
            Status::NotTested,
            None,
            Rest::Unchecked,
        ),
    ];

    let mut failures = Vec::new();
    for (name, kinds, want, names, rest) in rows {
        let checks = translated(kinds);
        let reachable = checks
            .iter()
            .find(|c| c.id.to_string() == "tls.tls_reachable")
            .expect("tls.tls_reachable emitted");
        if reachable.status != want {
            failures.push(format!(
                "{name}: tls_reachable is {:?}, want {want:?}",
                reachable.status
            ));
        }
        match names {
            Some(ip) if !reachable.findings.iter().any(|f| f.contains(ip)) => {
                failures.push(format!(
                    "{name}: tls_reachable findings name no {ip}: {:?}",
                    reachable.findings
                ))
            }
            None if want == Status::Unmeasured && reachable.findings.is_empty() => {
                failures.push(format!("{name}: Unmeasured tls_reachable has no finding"))
            }
            _ => {}
        }
        for c in checks
            .iter()
            .filter(|c| c.id.to_string() != "tls.tls_reachable")
        {
            let bad = match &rest {
                Rest::All(s) => c.status != *s,
                Rest::NoHandshake => {
                    c.status != Status::NotApplicable || c.findings != ["no TLS handshake"]
                }
                Rest::Unchecked => false,
            };
            if bad {
                failures.push(format!(
                    "{name}: {} is {:?} {:?}",
                    c.id, c.status, c.findings
                ));
            }
        }
        if !matches!(rest, Rest::Unchecked) {
            let n = checks.len() - 1;
            if n != 20 {
                failures.push(format!("{name}: {n} other checks, want 20"));
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
