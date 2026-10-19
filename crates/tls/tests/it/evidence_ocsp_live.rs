// V2 tls backend, Phase 1 (C4/R4, C12-C15): the live OCSP result in the handshake block.
//
// Spec: specs/features/v2-tls-backend/spec.md, Phase 1.
//
// Interface assumed (does not exist yet): `OcspRevocationResult` gains `pub answered: bool`
// (`#[serde(skip)]`): true only when the responder returned a successful response.

use std::path::PathBuf;

use netray_engine::SectionOutcome;
use netray_model::evidence::{Block, Shape, Value, validate};
use netray_tls::module::REGISTER;
use netray_tls::routes::InspectResponse;
use netray_tls::tls::ocsp::OcspRevocationResult;
use netray_tls::translate;
use serde_json::{Value as Json, json};

fn translated(ocsp: OcspRevocationResult) -> Vec<Block> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts/tlsight-inspect.json");
    let doc: Json = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut resp: InspectResponse = serde_json::from_value(doc).unwrap();
    resp.ports[0].ips[0].tls.as_mut().unwrap().ocsp_live = Some(ocsp);
    match translate(&resp) {
        SectionOutcome::Measured { evidence, .. } => evidence,
        other => panic!("expected Measured, got {other:?}"),
    }
}

fn handshake(blocks: &[Block]) -> Vec<(String, Value)> {
    let b = blocks
        .iter()
        .find(|b| b.id.to_string() == "tls.handshake@192.0.2.10:443")
        .expect("handshake block");
    match &b.shape {
        Shape::Fields(f) => f.clone(),
        other => panic!("handshake is not Fields: {other:?}"),
    }
}

fn get<'a>(f: &'a [(String, Value)], key: &str) -> Option<&'a Value> {
    f.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn ocsp(
    status: &str,
    reason: Option<&str>,
    revoked_at: Option<&str>,
    answered: bool,
) -> OcspRevocationResult {
    OcspRevocationResult {
        status: status.into(),
        reason: reason.map(Into::into),
        revoked_at: revoked_at.map(Into::into),
        checked_at: "2030-03-01T00:00:00Z".into(),
        answered,
    }
}

#[test]
fn answered_revocation_is_in_the_handshake_block_and_validates() {
    let blocks = translated(ocsp(
        "revoked",
        Some("KeyCompromise"),
        Some("2030-02-01T00:00:00Z"),
        true,
    ));
    validate("tls", &REGISTER, &blocks).expect("validates");
    let f = handshake(&blocks);
    assert_eq!(
        get(&f, "ocsp_live_status"),
        Some(&Value::Text("revoked".into()))
    );
    assert_eq!(
        get(&f, "ocsp_live_revoked_at"),
        Some(&Value::Time("2030-02-01T00:00:00Z".into()))
    );
    assert_eq!(
        get(&f, "ocsp_live_reason"),
        Some(&Value::Text("KeyCompromise".into()))
    );
}

#[test]
fn unanswered_fetch_leaves_no_ocsp_live_key() {
    for reason in [Some("blocked"), None] {
        let blocks = translated(ocsp("unknown", reason, None, false));
        let f = handshake(&blocks);
        let keys: Vec<&str> = f
            .iter()
            .map(|(k, _)| k.as_str())
            .filter(|k| k.starts_with("ocsp_live_"))
            .collect();
        assert!(keys.is_empty(), "reason {reason:?}: {keys:?}");
    }
}

#[test]
fn answered_unknown_status_is_published() {
    let blocks = translated(ocsp("unknown", None, None, true));
    validate("tls", &REGISTER, &blocks).expect("validates");
    let f = handshake(&blocks);
    assert_eq!(
        get(&f, "ocsp_live_status"),
        Some(&Value::Text("unknown".into()))
    );
}

#[test]
fn answered_stays_off_the_v1_wire() {
    let v = serde_json::to_value(ocsp(
        "revoked",
        Some("KeyCompromise"),
        Some("2030-02-01T00:00:00Z"),
        true,
    ))
    .unwrap();
    let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["checked_at", "reason", "revoked_at", "status"]);
    assert_eq!(v["status"], json!("revoked"));
}
