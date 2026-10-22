// V2 tls backend, Phase 1 (requirement 1, criteria C6 and C7): the certificate detail of each
// `tls.chain` item: serial, fingerprint_sha1, policy, self_signed, expired, ocsp_url.
//
// Spec: specs/features/v2-tls-backend/spec.md, Phase 1.

use std::path::PathBuf;

use netray_engine::SectionOutcome;
use netray_model::CheckResult;
use netray_model::evidence::{Block, Shape, Value, check_evidence_rule, validate};
use netray_tls::inspection::InspectResponse;
use netray_tls::module::REGISTER;
use netray_tls::translate;
use serde_json::{Value as Json, json};

const CHAIN: &str = "tls.chain@192.0.2.10:443";

fn fixture(name: &str) -> Json {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn response(doc: Json) -> InspectResponse {
    serde_json::from_value(doc).unwrap()
}

fn measured(outcome: SectionOutcome) -> (Vec<CheckResult>, Vec<Block>) {
    match outcome {
        SectionOutcome::Measured {
            checks, evidence, ..
        } => (checks, evidence),
        other => panic!("expected Measured, got {other:?}"),
    }
}

fn leaf_item(blocks: &[Block]) -> Vec<(String, Value)> {
    let block = blocks
        .iter()
        .find(|b| b.id.to_string() == CHAIN)
        .unwrap_or_else(|| panic!("{CHAIN} emitted"));
    match &block.shape {
        Shape::Chain(items) => items
            .iter()
            .find(|item| {
                item.iter()
                    .any(|(k, v)| k == "position" && *v == Value::Text("leaf".into()))
            })
            .expect("leaf item")
            .clone(),
        other => panic!("expected Shape::Chain, got {other:?}"),
    }
}

fn get<'a>(item: &'a [(String, Value)], key: &str) -> Option<&'a Value> {
    item.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

#[test]
fn chain_item_carries_certificate_detail() {
    let resp = response(fixture("tlsight-inspect.json"));
    let (_, blocks) = measured(translate(&resp));
    let leaf = leaf_item(&blocks);

    assert_eq!(get(&leaf, "serial"), Some(&Value::Text("01:02:03".into())));
    assert_eq!(
        get(&leaf, "fingerprint_sha1"),
        Some(&Value::Text("CC:DD".into()))
    );
    assert_eq!(get(&leaf, "policy"), Some(&Value::Text("DV".into())));
    assert_eq!(get(&leaf, "self_signed"), Some(&Value::Bool(false)));
    assert_eq!(get(&leaf, "expired"), Some(&Value::Bool(false)));
    assert_eq!(
        get(&leaf, "ocsp_url"),
        Some(&Value::Text("http://ocsp.example.com".into()))
    );
}

#[test]
fn chain_item_omits_unknown_certificate_detail() {
    let mut doc = fixture("tlsight-inspect.json");
    let cert = doc["ports"][0]["ips"][0]["chain"][0]
        .as_object_mut()
        .unwrap();
    cert.insert("cert_policy".into(), json!("unknown"));
    cert.insert("serial".into(), json!(""));
    cert.insert("fingerprint_sha1".into(), json!(""));
    cert.remove("ocsp_url");

    let (checks, blocks) = measured(translate(&response(doc)));
    let leaf = leaf_item(&blocks);
    for key in ["policy", "serial", "fingerprint_sha1", "ocsp_url"] {
        assert_eq!(get(&leaf, key), None, "{key} omitted: {leaf:?}");
    }
    // The known booleans still ride along, so this fails today for the right reason.
    assert_eq!(get(&leaf, "self_signed"), Some(&Value::Bool(false)));

    validate("tls", &REGISTER, &blocks)
        .unwrap_or_else(|e| panic!("evidence does not validate: {e:?}"));
    check_evidence_rule(&checks, &blocks).unwrap_or_else(|e| panic!("evidence rule: {e:?}"));
}
