// V2 tls backend, Phase 1 (R2, R3; criteria C2, C3, C8-C11): the `tls.handshake` block carries
// the SNI, the STARTTLS protocol and the OCSP staple's detail.
//
// Spec: specs/features/v2-tls-backend/spec.md, Phase 1.

use std::path::PathBuf;

use netray_engine::SectionOutcome;
use netray_model::evidence::{Shape, Value, validate};
use netray_tls::inspection::InspectResponse;
use netray_tls::module::REGISTER;
use netray_tls::translate;
use serde_json::{Value as Json, json};

const BLOCK: &str = "tls.handshake@192.0.2.10:443";

fn fixture() -> Json {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts/tlsight-inspect.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Translates the document and returns the handshake block's fields; the whole evidence
/// must validate against the register.
fn handshake(doc: Json) -> Vec<(String, Value)> {
    let resp: InspectResponse = serde_json::from_value(doc).unwrap();
    let SectionOutcome::Measured { evidence, .. } = translate(&resp) else {
        panic!("expected Measured");
    };
    validate("tls", &REGISTER, &evidence)
        .unwrap_or_else(|e| panic!("evidence does not validate: {e:?}"));
    let block = evidence
        .iter()
        .find(|b| b.id.to_string() == BLOCK)
        .unwrap_or_else(|| panic!("{BLOCK} emitted"));
    let Shape::Fields(fields) = &block.shape else {
        panic!("{BLOCK} must be a fields block");
    };
    fields.clone()
}

fn get<'a>(fields: &'a [(String, Value)], key: &str) -> Option<&'a Value> {
    fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn tls_mut(doc: &mut Json) -> &mut serde_json::Map<String, Json> {
    doc["ports"][0]["ips"][0]["tls"].as_object_mut().unwrap()
}

#[test]
fn handshake_carries_sni_and_starttls_when_sent() {
    let mut doc = fixture();
    tls_mut(&mut doc).insert("starttls".into(), json!("smtp"));
    let fields = handshake(doc);
    assert_eq!(
        get(&fields, "sni"),
        Some(&Value::Text("example.com".into()))
    );
    assert_eq!(get(&fields, "starttls"), Some(&Value::Text("smtp".into())));
}

#[test]
fn handshake_omits_sni_and_starttls_when_absent() {
    let mut doc = fixture();
    let tls = tls_mut(&mut doc);
    tls.remove("sni");
    tls.remove("starttls");
    let fields = handshake(doc);
    assert_eq!(get(&fields, "sni"), None);
    assert_eq!(get(&fields, "starttls"), None);
}

#[test]
fn handshake_carries_the_ocsp_staple_detail() {
    let mut doc = fixture();
    tls_mut(&mut doc).insert(
        "ocsp".into(),
        json!({
            "stapled": true,
            "status": "good",
            "this_update": "2030-01-01T00:00:00Z",
            "next_update": "2030-01-08T00:00:00Z"
        }),
    );
    let fields = handshake(doc);
    assert_eq!(
        get(&fields, "ocsp_status"),
        Some(&Value::Text("good".into()))
    );
    assert_eq!(
        get(&fields, "ocsp_this_update"),
        Some(&Value::Time("2030-01-01T00:00:00Z".into()))
    );
    assert_eq!(
        get(&fields, "ocsp_next_update"),
        Some(&Value::Time("2030-01-08T00:00:00Z".into()))
    );
}

#[test]
fn handshake_without_a_staple_has_only_ocsp_stapled() {
    let fields = handshake(fixture());
    assert_eq!(get(&fields, "ocsp_stapled"), Some(&Value::Bool(false)));
    let extra: Vec<&str> = fields
        .iter()
        .map(|(k, _)| k.as_str())
        .filter(|k| k.starts_with("ocsp_") && *k != "ocsp_stapled")
        .collect();
    assert!(extra.is_empty(), "unexpected keys: {extra:?}");
}
