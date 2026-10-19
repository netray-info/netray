// v2-tls-backend, Phase 2 (requirements 6 and 7, criteria C8-C12): one outcome per inspected
// port, port-qualified port-wide blocks, the host-wide CAA block once, and `tls.notices`.
//
// Spec: specs/features/v2-tls-backend/spec.md, Phase 2.
//
// Interface assumed (does not exist yet):
// - `netray_tls::module::translate_ports(&InspectResponse) -> Vec<(u16, SectionOutcome)>`

use std::path::PathBuf;

use netray_engine::SectionOutcome;
use netray_model::evidence::{Block, Shape, Value, check_evidence_rule, validate};
use netray_model::{CheckResult, Status};
use netray_tls::module::{REGISTER, translate_ports};
use netray_tls::routes::InspectResponse;
use netray_tls::translate;
use serde_json::{Value as Json, json};

fn fixture() -> Json {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts/tlsight-inspect.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn response(doc: Json) -> InspectResponse {
    serde_json::from_value(doc).unwrap()
}

/// The fixture with a second port 465 cloned from the first.
fn two_ports() -> Json {
    let mut doc = fixture();
    for port in doc["ports"].as_array_mut().unwrap() {
        let p = port.as_object_mut().unwrap();
        p.remove("quality");
        p.remove("consistency");
    }
    let mut second = doc["ports"][0].clone();
    second["port"] = json!(465);
    doc["ports"].as_array_mut().unwrap().push(second);
    doc
}

fn measured(outcome: &SectionOutcome) -> (&Vec<CheckResult>, &Vec<Block>) {
    match outcome {
        SectionOutcome::Measured {
            checks, evidence, ..
        } => (checks, evidence),
        other => panic!("expected Measured, got {other:?}"),
    }
}

fn ids(outcome: &SectionOutcome) -> Vec<String> {
    measured(outcome)
        .1
        .iter()
        .map(|b| b.id.to_string())
        .collect()
}

fn status(outcome: &SectionOutcome, short: &str) -> Status {
    let id = format!("tls.{short}");
    measured(outcome)
        .0
        .iter()
        .find(|c| c.id.to_string() == id)
        .unwrap_or_else(|| panic!("{id} emitted"))
        .status
}

fn notices(outcome: &SectionOutcome) -> Option<&Block> {
    measured(outcome)
        .1
        .iter()
        .find(|b| b.id.to_string() == "tls.notices")
}

#[test]
fn two_ports_give_two_qualified_outcomes() {
    // CT checked, so each port carries its SCT table (a skipped CT check has none).
    let mut doc = two_ports();
    doc["summary"]["checks"]["ct_logged"] = serde_json::json!("pass");
    let outcomes = translate_ports(&response(doc));
    assert_eq!(
        outcomes.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
        vec![443, 465]
    );
    for (port, outcome) in &outcomes {
        let (checks, blocks) = measured(outcome);
        assert_eq!(checks.len(), 21, "port {port}");
        let found = ids(outcome);
        assert!(
            found.contains(&format!("tls.addresses@{port}")),
            "{found:?}"
        );
        assert!(!found.iter().any(|i| i == "tls.addresses"), "{found:?}");
        assert!(
            found.contains(&format!("tls.chain@192.0.2.10:{port}")),
            "{found:?}"
        );
        assert!(found.contains(&format!("tls.ct@{port}")), "{found:?}");
        validate("tls", &REGISTER, blocks)
            .unwrap_or_else(|e| panic!("port {port} does not validate: {e:?}"));
        check_evidence_rule(checks, blocks)
            .unwrap_or_else(|e| panic!("port {port} evidence rule: {e:?}"));
    }
}

#[test]
fn a_failed_port_fails_only_its_own_reachability() {
    let mut doc = two_ports();
    let second = doc["ports"][1].as_object_mut().unwrap();
    second.insert(
        "ips".into(),
        json!([{
            "ip": "192.0.2.10",
            "ip_version": "v4",
            "error": { "code": "HANDSHAKE_FAILED", "message": "connection refused" }
        }]),
    );
    let outcomes = translate_ports(&response(doc));
    assert_eq!(status(&outcomes[0].1, "tls_reachable"), Status::Pass);
    assert_eq!(status(&outcomes[1].1, "tls_reachable"), Status::Fail);
}

#[test]
fn one_port_equals_translate() {
    let resp = response(fixture());
    let outcomes = translate_ports(&resp);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].0, 443);
    assert_eq!(outcomes[0].1, translate(&resp));
    assert!(ids(&outcomes[0].1).iter().any(|i| i == "tls.addresses"));
}

#[test]
fn host_wide_caa_block_is_in_the_first_outcome_only() {
    let mut doc = two_ports();
    doc["dns"] = json!({
        "caa": { "records": [], "issuer_allowed": true, "issuewild_present": false },
        "resolved_ips": ["192.0.2.10"]
    });
    let outcomes = translate_ports(&response(doc));
    assert!(ids(&outcomes[0].1).iter().any(|i| i == "tls.caa"));
    assert!(!ids(&outcomes[1].1).iter().any(|i| i == "tls.caa"));
}

#[test]
fn notices_block_carries_warnings_and_skipped_addresses_in_the_first_outcome() {
    let mut doc = two_ports();
    doc["warnings"] = json!(["192.0.2.99: blocked (documentation range)"]);
    doc["skipped_ips"] = json!(["192.0.2.20", "192.0.2.21"]);
    let resp = response(doc);
    let outcomes = translate_ports(&resp);

    let block = notices(&outcomes[0].1).expect("tls.notices in the first outcome");
    let Shape::Fields(fields) = &block.shape else {
        panic!("tls.notices is a fields block: {:?}", block.shape);
    };
    let addr = |s: &str| Value::Addr(s.parse().unwrap());
    assert_eq!(
        fields,
        &vec![
            (
                "warning".to_string(),
                Value::Text("192.0.2.99: blocked (documentation range)".into())
            ),
            ("skipped_address".to_string(), addr("192.0.2.20")),
            ("skipped_address".to_string(), addr("192.0.2.21")),
        ]
    );
    assert!(notices(&outcomes[1].1).is_none());
    assert!(notices(&translate(&resp)).is_none());
}

#[test]
fn no_notices_block_without_warnings_or_skipped_addresses() {
    for (_, outcome) in translate_ports(&response(two_ports())) {
        assert!(notices(&outcome).is_none());
    }
}

// Review of Phase 2: OCSP, DANE and the TLSA lookup error are judged per port, not from the
// summary of the first handshake; the host-wide blocks sit in the band port's outcome.

#[test]
fn ocsp_stapled_is_judged_on_each_port() {
    let mut doc = two_ports();
    doc["summary"]["checks"]["ocsp_stapled"] = json!("pass");
    doc["ports"][0]["ips"][0]["tls"]["ocsp"] = json!({"stapled": true, "status": "good"});
    doc["ports"][1]["ips"][0]["tls"]["ocsp"] = json!({"stapled": false});
    let outcomes = translate_ports(&response(doc));
    assert_eq!(status(&outcomes[0].1, "ocsp_stapled"), Status::Pass);
    assert_eq!(status(&outcomes[1].1, "ocsp_stapled"), Status::Warn);
}

#[test]
fn dane_and_a_tlsa_error_are_judged_on_each_port() {
    let mut doc = two_ports();
    doc["summary"]["checks"]["dane_valid"] = json!("pass");
    doc["ports"][0]["tlsa"] = json!({
        "records": ["3 1 1 aabb"],
        "dnssec_signed": false,
        "dane_valid": true
    });
    doc["dns"] = json!({"resolved_ips": ["192.0.2.10"], "lookup_errors": ["tlsa"]});
    let outcomes = translate_ports(&response(doc));
    assert_eq!(status(&outcomes[0].1, "dane_valid"), Status::Pass);
    assert_eq!(status(&outcomes[1].1, "dane_valid"), Status::Unmeasured);
}

#[test]
fn notices_sit_in_the_band_port_without_443() {
    let mut doc = two_ports();
    doc["ports"][0]["port"] = json!(8443);
    doc["ports"].as_array_mut().unwrap().reverse();
    doc["warnings"] = json!(["192.0.2.99: blocked (documentation range)"]);
    let outcomes = translate_ports(&response(doc));
    assert_eq!(
        outcomes.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
        vec![465, 8443]
    );
    assert!(notices(&outcomes[0].1).is_some());
    assert!(notices(&outcomes[1].1).is_none());
    assert_eq!(netray_tls::module::band_port([8443, 465]), 465);
    assert_eq!(netray_tls::module::band_port([8443, 443]), 443);
}
