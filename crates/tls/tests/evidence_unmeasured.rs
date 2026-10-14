// v2-evidence branch review: an unmeasured TLS check references no block (Rule M; the same as
// DNS and HTTP). A failed CAA lookup must not point at the facts' `dns.caa` answer, nor a failed
// HTTPS lookup at `dns.https`.

use std::path::PathBuf;

use netray_engine::SectionOutcome;
use netray_model::evidence::BlockId;
use netray_model::{CheckResult, Status};
use netray_tls::module::translate_with_facts;
use netray_tls::routes::InspectResponse;
use serde_json::{Value, json};

fn golden() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts/tlsight-inspect.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn translated(lookup_errors: &[&str]) -> Vec<CheckResult> {
    let mut doc = golden();
    doc["dns"]["lookup_errors"] = json!(lookup_errors);
    let resp: InspectResponse = serde_json::from_value(doc).unwrap();
    let facts = [
        BlockId::parse("dns.caa").unwrap(),
        BlockId::parse("dns.https").unwrap(),
    ];
    match translate_with_facts(&resp, &facts) {
        SectionOutcome::Measured { checks, .. } => checks,
        other => panic!("expected Measured, got {other:?}"),
    }
}

#[test]
fn unmeasured_checks_reference_nothing() {
    let checks = translated(&["caa", "https"]);
    let mut unmeasured = 0;
    for c in &checks {
        if c.status == Status::Unmeasured {
            unmeasured += 1;
            assert!(
                c.evidence.is_empty(),
                "{} unmeasured but references {:?}",
                c.id,
                c.evidence
            );
        }
    }
    let caa = checks
        .iter()
        .find(|c| c.id.to_string() == "tls.caa_compliant")
        .unwrap();
    assert_eq!(caa.status, Status::Unmeasured);
    assert!(
        unmeasured >= 2,
        "caa_compliant and ech_advertised are unmeasured"
    );
}
