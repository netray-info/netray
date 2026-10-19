// V2 unmeasured, spec requirements 11 and 16, criteria C4 and C17: every `tls.*` catalogue check
// can report its worst measured status and `Unmeasured` with a non-empty finding.
//
// Worst measured status per check, verified against `quality/checks.rs`: Fail for every check
// with a Fail branch; Warn for the six without one (san_quality, ct_logged, ocsp_stapled,
// consistency, alpn_consistency, ech_advertised). No other check lacks a Fail branch:
// caa_compliant and dane_valid pass the summary status through, so they can Fail.
//
// Assumed interfaces, absent today: per-address error code `HANDSHAKE_TIMEOUT` (timeout is
// unmeasured) and `InspectResponse.dns.lookup_errors` naming errored lookups ("caa", "https",
// "tlsa"), as in `reachable_unmeasured.rs` and `lookup_errors_unmeasured.rs`.

use std::collections::BTreeSet;
use std::path::PathBuf;

use netray_engine::SectionOutcome;
use netray_model::{CheckResult, Status};
use netray_tls::routes::InspectResponse;
use netray_tls::translate;
use serde_json::{Value, json};

const A: &str = "192.0.2.10";
const B: &str = "192.0.2.11";

/// Catalogue `tls.*` ids deliberately absent from the table, with the reason. None today.
const EXCEPTIONS: [(&str, &str); 0] = [];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn golden() -> Value {
    let path = root().join("tests/fixtures/contracts/tlsight-inspect.json");
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

/// The golden with the given addresses, the per-port `quality` block dropped so the translation
/// works from the per-address results, and `tweak` applied last.
fn translated(ips: Vec<Value>, tweak: impl FnOnce(&mut Value)) -> Vec<CheckResult> {
    let mut doc = golden();
    let port = doc["ports"][0].as_object_mut().unwrap();
    port.remove("quality");
    port.remove("consistency");
    port.insert("ips".into(), Value::Array(ips));
    tweak(&mut doc);
    let resp: InspectResponse = serde_json::from_value(doc).unwrap();
    match translate(&resp) {
        SectionOutcome::Measured { checks, .. } => checks,
        other => panic!("expected Measured, got {other:?}"),
    }
}

/// One handshaken address, `edit` applied to it.
fn one(edit: impl FnOnce(&mut Value)) -> Vec<Value> {
    let mut ip = handshook(A);
    edit(&mut ip);
    vec![ip]
}

fn noop(_: &mut Value) {}

/// (check, worst measured status, addresses, tweak of the whole document)
type Worst = (&'static str, Status, Vec<Value>, fn(&mut Value));
/// (check, addresses, tweak of the whole document)
type Unmeasured = (&'static str, Vec<Value>, fn(&mut Value));

fn worst_rows() -> Vec<Worst> {
    use Status::{Fail, Warn};
    let version = |v: &'static str| move |ip: &mut Value| ip["tls"]["version"] = json!(v);
    vec![
        ("tls_reachable", Fail, vec![refused(A), refused(B)], noop),
        (
            "chain_trusted",
            Fail,
            one(|ip| ip["validation"]["chain_trusted"] = json!(false)),
            noop,
        ),
        (
            "not_expired",
            Fail,
            one(|ip| ip["validation"]["any_expired"] = json!(true)),
            noop,
        ),
        (
            "hostname_match",
            Fail,
            one(|ip| ip["validation"]["leaf_covers_hostname"] = json!(false)),
            noop,
        ),
        (
            "chain_complete",
            Fail,
            one(|ip| ip["validation"]["ends_at_anchor"] = json!(false)),
            noop,
        ),
        (
            "strong_signature",
            Fail,
            one(|ip| ip["validation"]["weakest_signature"] = json!("sha1WithRSAEncryption")),
            noop,
        ),
        (
            "key_strength",
            Fail,
            one(|ip| {
                ip["chain"][0]["key_type"] = json!("RSA");
                ip["chain"][0]["key_size"] = json!(1024);
            }),
            noop,
        ),
        (
            "expiry_window",
            Fail,
            one(|ip| ip["chain"][0]["days_remaining"] = json!(3)),
            noop,
        ),
        (
            "cert_lifetime",
            Fail,
            one(|ip| ip["chain"][0]["lifetime_days"] = json!(900)),
            noop,
        ),
        (
            "san_quality",
            Warn,
            one(|ip| ip["chain"][0]["sans"] = json!(["example.com", "intranet"])),
            noop,
        ),
        (
            // Incomplete chain and no CA Issuers URL (the golden has none).
            "aia_reachability",
            Fail,
            one(|ip| ip["validation"]["ends_at_anchor"] = json!(false)),
            noop,
        ),
        ("tls_version", Fail, one(version("TLSv1.0")), noop),
        (
            "forward_secrecy",
            Fail,
            one(|ip| ip["tls"]["cipher_suite"] = json!("TLS_RSA_WITH_AES_128_GCM_SHA256")),
            noop,
        ),
        (
            "aead_cipher",
            Fail,
            one(|ip| ip["tls"]["cipher_suite"] = json!("TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA")),
            noop,
        ),
        // CT enabled (summary not "skip") and no SCTs on the address.
        ("ct_logged", Warn, one(noop), |d| {
            d["summary"]["checks"]["ct_logged"] = json!("warn")
        }),
        ("ocsp_stapled", Warn, one(noop), |d| {
            d["summary"]["checks"]["ocsp_stapled"] = json!("warn")
        }),
        ("caa_compliant", Fail, one(noop), |d| {
            d["summary"]["checks"]["caa_compliant"] = json!("fail")
        }),
        ("dane_valid", Fail, one(noop), |d| {
            d["summary"]["checks"]["dane_valid"] = json!("fail")
        }),
        ("consistency", Warn, one(noop), |d| {
            d["ports"][0]["consistency"] = json!({
                "certificates_match": false,
                "tls_versions_match": true,
                "cipher_suites_match": true,
                "mismatches": [{ "field": "certificate", "values": { A: "AA", B: "BB" } }]
            });
        }),
        (
            "alpn_consistency",
            Warn,
            vec![handshook(A), {
                let mut ip = handshook(B);
                ip["tls"]["alpn"] = json!("http/1.1");
                ip
            }],
            noop,
        ),
        (
            "ech_advertised",
            Warn,
            one(|ip| ip["tls"]["ech_advertised"] = json!(false)),
            noop,
        ),
    ]
}

fn unmeasured_rows() -> Vec<Unmeasured> {
    let both_timed_out = || vec![timed_out(A), timed_out(B)];
    // Checks that need a handshake: every address timed out.
    let mut rows: Vec<Unmeasured> = [
        "tls_reachable",
        "chain_trusted",
        "not_expired",
        "hostname_match",
        "chain_complete",
        "strong_signature",
        "key_strength",
        "expiry_window",
        "cert_lifetime",
        "san_quality",
        "aia_reachability",
        "tls_version",
        "forward_secrecy",
        "aead_cipher",
        "ct_logged",
        "ocsp_stapled",
    ]
    .into_iter()
    .map(|id| (id, both_timed_out(), noop as fn(&mut Value)))
    .collect();
    // Lookups that errored while the handshake worked.
    rows.push(("caa_compliant", vec![handshook(A)], |d| {
        d["dns"]["lookup_errors"] = json!(["caa"]);
    }));
    rows.push(("ech_advertised", vec![handshook(A)], |d| {
        d["dns"]["lookup_errors"] = json!(["https"]);
    }));
    rows.push(("dane_valid", vec![handshook(A)], |d| {
        d["dns"]["lookup_errors"] = json!(["tlsa"]);
    }));
    // One address handshook, one timed out: nothing to compare against.
    rows.push(("consistency", vec![handshook(A), timed_out(B)], noop));
    rows.push(("alpn_consistency", vec![handshook(A), timed_out(B)], noop));
    rows
}

fn status_of<'a>(checks: &'a [CheckResult], id: &str) -> &'a CheckResult {
    let full = format!("tls.{id}");
    checks
        .iter()
        .find(|c| c.id.to_string() == full)
        .unwrap_or_else(|| panic!("no CheckResult {full}"))
}

#[test]
fn every_tls_check_reports_its_worst_status_and_unmeasured() {
    let mut failures = Vec::new();
    for (id, want, ips, tweak) in worst_rows() {
        let checks = translated(ips, tweak);
        let c = status_of(&checks, id);
        if c.status != want {
            failures.push(format!("worst: tls.{id} is {:?}, want {want:?}", c.status));
        }
    }
    for (id, ips, tweak) in unmeasured_rows() {
        let checks = translated(ips, tweak);
        let c = status_of(&checks, id);
        if c.status != Status::Unmeasured {
            failures.push(format!(
                "unmeasured: tls.{id} is {:?}, want Unmeasured",
                c.status
            ));
        } else if c.findings.iter().all(|f| f.trim().is_empty()) {
            failures.push(format!("unmeasured: tls.{id} has no finding"));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn table_ids_equal_the_catalogue_tls_ids() {
    #[derive(serde::Deserialize)]
    struct Catalogue {
        check: Vec<Entry>,
    }
    #[derive(serde::Deserialize)]
    struct Entry {
        id: String,
    }
    let text = std::fs::read_to_string(root().join("crates/engine/catalogue/checks.toml")).unwrap();
    let catalogue: Catalogue = toml::from_str(&text).unwrap();
    let want: BTreeSet<String> = catalogue
        .check
        .into_iter()
        .filter_map(|e| e.id.strip_prefix("tls.").map(str::to_string))
        .filter(|id| !EXCEPTIONS.iter().any(|(x, _)| x == id))
        .collect();
    let worst: BTreeSet<String> = worst_rows().iter().map(|r| r.0.to_string()).collect();
    let unmeasured: BTreeSet<String> = unmeasured_rows().iter().map(|r| r.0.to_string()).collect();
    assert_eq!(worst, want, "worst-status rows vs catalogue tls.* ids");
    assert_eq!(unmeasured, want, "unmeasured rows vs catalogue tls.* ids");
    assert_eq!(want.len(), 21);
}
