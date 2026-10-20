// V2 catalogue, Phase 3, requirement 3(d) and criterion C5: the TLS translation emits all 23
// `tls.*` catalogue IDs in every run, measures per address and reports the worst of the addresses
// that completed a handshake. Fixtures derive from the tlsight golden; the per-port `quality`
// block is dropped so the translation must work from the per-address results.

use std::path::PathBuf;

use netray_engine::{Module, SectionOutcome};
use netray_model::{CheckResult, Status};
use netray_tls::routes::InspectResponse;
use netray_tls::{ModuleConfig, TlsModule, translate};
use serde_json::{Value, json};

const VALID: &str = "192.0.2.10";
const OTHER: &str = "192.0.2.11";

const IDS: [&str; 23] = [
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
    "legacy_versions",
    "weak_suites",
    "ct_logged",
    "ocsp_stapled",
    "caa_compliant",
    "dane_valid",
    "consistency",
    "alpn_consistency",
    "ech_advertised",
];

fn golden() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts/tlsight-inspect.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn valid_ip(ip: &str) -> Value {
    let mut v = golden()["ports"][0]["ips"][0].clone();
    v["ip"] = json!(ip);
    v
}

fn expired_ip(ip: &str) -> Value {
    let mut v = valid_ip(ip);
    v["chain"][0]["is_expired"] = json!(true);
    v["chain"][0]["days_remaining"] = json!(-5);
    v["chain"][0]["not_before"] = json!("2020-01-01T00:00:00Z");
    v["chain"][0]["not_after"] = json!("2020-04-01T00:00:00Z");
    v["validation"]["any_expired"] = json!(true);
    v["validation"]["earliest_expiry"] = json!("2020-04-01T00:00:00Z");
    v["validation"]["earliest_expiry_days"] = json!(-5);
    v
}

fn failed_ip(ip: &str) -> Value {
    json!({
        "ip": ip,
        "ip_version": "v4",
        "error": { "code": "HANDSHAKE_FAILED", "message": "connection refused" }
    })
}

fn translated(ips: Vec<Value>) -> Vec<CheckResult> {
    let mut doc = golden();
    let port = doc["ports"][0].as_object_mut().unwrap();
    port.remove("quality");
    port.remove("consistency");
    port.insert("ips".into(), Value::Array(ips));
    let resp: InspectResponse = serde_json::from_value(doc).unwrap();
    match translate(&resp) {
        SectionOutcome::Measured { checks, .. } => checks,
        other => panic!("expected Measured, got {other:?}"),
    }
}

fn check<'a>(checks: &'a [CheckResult], name: &str) -> &'a CheckResult {
    let id = format!("tls.{name}");
    let mut hits = checks.iter().filter(|c| c.id.to_string() == id);
    let first = hits.next().unwrap_or_else(|| panic!("no CheckResult {id}"));
    assert!(hits.next().is_none(), "{id} emitted twice");
    first
}

fn assert_all_ids_once(checks: &[CheckResult]) {
    assert_eq!(checks.len(), 23, "23 checks");
    for name in IDS {
        check(checks, name);
    }
}

#[test]
fn every_run_emits_all_23_ids_once() {
    for ips in [
        vec![valid_ip(VALID)],
        vec![valid_ip(VALID), expired_ip(OTHER)],
        vec![valid_ip(VALID), failed_ip(OTHER)],
        vec![failed_ip(VALID), failed_ip(OTHER)],
    ] {
        assert_all_ids_once(&translated(ips));
    }
}

#[test]
fn expired_address_fails_not_expired_and_is_named() {
    let checks = translated(vec![valid_ip(VALID), expired_ip(OTHER)]);
    let c = check(&checks, "not_expired");
    assert_eq!(c.status, Status::Fail);
    assert!(
        c.findings.iter().any(|f| f.contains(OTHER)),
        "finding names {OTHER}: {:?}",
        c.findings
    );
    assert!(
        !c.findings.iter().any(|f| f.contains(VALID)),
        "finding does not name the valid address: {:?}",
        c.findings
    );
}

#[test]
fn one_handshake_and_one_failed_address_is_reachable_and_measured_on_the_reachable_one() {
    let checks = translated(vec![valid_ip(VALID), failed_ip(OTHER)]);
    assert_eq!(check(&checks, "tls_reachable").status, Status::Pass);
    assert_eq!(check(&checks, "not_expired").status, Status::Pass);
    assert_eq!(check(&checks, "chain_trusted").status, Status::Pass);
    for name in IDS {
        assert_ne!(
            check(&checks, name).findings,
            ["no TLS handshake"],
            "tls.{name} is measured"
        );
    }
}

#[test]
fn no_handshake_fails_reachable_and_marks_the_other_22_not_applicable() {
    let checks = translated(vec![failed_ip(VALID), failed_ip(OTHER)]);
    assert_all_ids_once(&checks);
    assert_eq!(check(&checks, "tls_reachable").status, Status::Fail);
    for name in IDS.into_iter().filter(|n| *n != "tls_reachable") {
        let c = check(&checks, name);
        assert_eq!(c.status, Status::NotApplicable, "tls.{name}");
        assert_eq!(c.findings, ["no TLS handshake"], "tls.{name}");
    }
}

#[test]
fn complete_chain_makes_aia_reachability_not_applicable() {
    let checks = translated(vec![valid_ip(VALID)]);
    assert_eq!(
        check(&checks, "aia_reachability").status,
        Status::NotApplicable
    );
}

#[test]
fn one_address_makes_consistency_checks_not_applicable() {
    let checks = translated(vec![valid_ip(VALID)]);
    assert_eq!(check(&checks, "consistency").status, Status::NotApplicable);
    assert_eq!(
        check(&checks, "alpn_consistency").status,
        Status::NotApplicable
    );
}

#[tokio::test]
async fn module_declares_all_23_ids_and_wants_facts() {
    let cfg: ModuleConfig = toml::from_str("[limits]\nmax_ports = 3").unwrap();
    let module = TlsModule::new(cfg).await.expect("module builds");
    let ids: Vec<String> = module.checks().iter().map(|i| i.to_string()).collect();
    let want: Vec<String> = IDS.iter().map(|n| format!("tls.{n}")).collect();
    assert_eq!(ids, want);
    assert!(module.wants_facts());
}
