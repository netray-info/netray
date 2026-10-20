// v2-tls-probe-close, C3, C14: a probe refused by a close is a refusal in the evidence and the
// checks: the key reads `disabled` or `refused`, its answer `closed`.
//
// Spec: specs/features/v2-tls-probe-close/spec.md, Phase 1.

use std::path::PathBuf;

use netray_engine::SectionOutcome;
use netray_model::Status;
use netray_model::evidence::{Shape, Value};
use netray_tls::routes::InspectResponse;
use netray_tls::tls::probe::{ProbeOutcome, ProtocolProbes, Verdict};
use netray_tls::translate;

fn fixture() -> InspectResponse {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts/tlsight-inspect.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn out(verdict: Verdict, detail: &str) -> ProbeOutcome {
    ProbeOutcome {
        verdict,
        version: None,
        cipher_suite: None,
        detail: detail.into(),
    }
}

#[test]
fn a_closed_refusal_is_disabled_with_answer_closed_and_legacy_versions_pass() {
    let mut resp = fixture();
    resp.ports[0].ips[0].probes = Some(ProtocolProbes {
        ssl3: out(Verdict::Refused, "closed (reset)"),
        tls1_0: out(Verdict::Refused, "closed"),
        tls1_1: out(Verdict::Refused, "alert protocol_version"),
        tls1_2: out(Verdict::Accepted, "handshake TLSv1.2"),
        tls1_3: out(Verdict::Refused, "handshake TLSv1.2"),
        suites_insecure: out(Verdict::Refused, "closed"),
        suites_cbc: out(Verdict::Refused, "alert handshake_failure"),
        suites_static_rsa: out(Verdict::Refused, "alert handshake_failure"),
    });
    let outcome = translate(&resp);
    let SectionOutcome::Measured {
        checks, evidence, ..
    } = &outcome
    else {
        panic!("expected Measured, got {outcome:?}");
    };

    let block = evidence
        .iter()
        .find(|b| b.id.to_string() == "tls.probes@192.0.2.10:443")
        .expect("tls.probes block");
    let Shape::Fields(fields) = &block.shape else {
        panic!("fields block");
    };
    let get = |k: &str| fields.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
    let text = |s: &str| Some(Value::Text(s.into()));
    assert_eq!(get("tls1_0"), text("disabled"));
    assert_eq!(get("tls1_0_answer"), text("closed"));
    assert_eq!(get("ssl3"), text("disabled"));
    assert_eq!(get("ssl3_answer"), text("closed (reset)"));
    assert_eq!(get("suites_insecure"), text("refused"));
    assert_eq!(get("suites_insecure_answer"), text("closed"));

    let status = |id: &str| {
        checks
            .iter()
            .find(|c| c.id.to_string() == id)
            .unwrap_or_else(|| panic!("no {id}"))
            .status
    };
    assert_eq!(status("tls.legacy_versions"), Status::Pass);
    assert_eq!(status("tls.weak_suites"), Status::Pass);
}
