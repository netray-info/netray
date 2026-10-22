// V2 evidence, Phase 3 review rows (C7-C13): reference accuracy, the ECH key, the chain items and
// the volatile TLSA/CAA TTLs.
//
// Spec: specs/features/v2-evidence/spec.md, Phase 3.

use std::collections::BTreeSet;
use std::path::PathBuf;

use netray_engine::{EvidencePath, Module, SectionOutcome};
use netray_model::CheckResult;
use netray_model::evidence::{Block, BlockId, Shape, Shape as S, Value, ValueType};
use netray_tls::inspection::InspectResponse;
use netray_tls::module::{REGISTER, translate_with_facts};
use netray_tls::{ModuleConfig, TlsModule, translate};
use serde_json::{Value as Json, json};

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

fn check<'a>(checks: &'a [CheckResult], short: &str) -> &'a CheckResult {
    let id = format!("tls.{short}");
    checks
        .iter()
        .find(|c| c.id.to_string() == id)
        .unwrap_or_else(|| panic!("{id} emitted"))
}

fn ref_ids(c: &CheckResult) -> Vec<String> {
    c.evidence.iter().map(ToString::to_string).collect()
}

fn dns_caa_facts() -> Vec<BlockId> {
    vec![BlockId::parse("dns.caa").unwrap()]
}

#[test]
fn caa_compliant_with_parent_level_records_is_not_only_the_facts_block() {
    let mut doc = fixture("tlsight-inspect.json");
    doc["dns"]["caa"]["records"] = json!(["0 issue \"ca.example.com\""]);
    doc["summary"]["checks"]["caa_compliant"] = json!("fail");
    let resp = response(doc);

    let (checks, _) = measured(translate_with_facts(&resp, &dns_caa_facts()));
    assert_ne!(
        ref_ids(check(&checks, "caa_compliant")),
        vec!["dns.caa".to_string()],
        "the verdict was judged on TLS's own records, not the facts' dns.caa"
    );
}

#[test]
fn ech_key_is_absent_when_the_https_lookup_failed() {
    let mut doc = fixture("tlsight-inspect.json");
    doc["dns"]["lookup_errors"] = json!(["https"]);
    doc["ports"][0]["ips"][0]["tls"]["ech_advertised"] = json!(false);
    let (_, blocks) = measured(translate(&response(doc)));

    let handshakes: Vec<&Block> = blocks
        .iter()
        .filter(|b| b.id.to_string().starts_with("tls.handshake@"))
        .collect();
    assert!(!handshakes.is_empty());
    for b in handshakes {
        let Shape::Fields(fields) = &b.shape else {
            panic!("handshake is Fields");
        };
        assert!(
            !fields.iter().any(|(k, _)| k == "ech"),
            "{}: ech present although the HTTPS lookup failed",
            b.id
        );
    }
}

#[test]
fn consistency_references_chains_and_handshakes() {
    let resp = response(fixture("tlsight-inspect.json"));
    let (checks, _) = measured(translate(&resp));
    let ids = ref_ids(check(&checks, "consistency"));
    assert!(ids.iter().any(|i| i.starts_with("tls.chain@")), "{ids:?}");
    assert!(
        ids.iter().any(|i| i.starts_with("tls.handshake@")),
        "{ids:?}"
    );
}

#[test]
fn chain_items_carry_ca_issuers_url_and_it_is_registered() {
    let mut doc = fixture("tlsight-inspect.json");
    doc["ports"][0]["ips"][0]["chain"][0]["ca_issuers_url"] = json!("http://ca.example.com/ca.crt");
    let (_, blocks) = measured(translate(&response(doc)));

    let chain = blocks
        .iter()
        .find(|b| b.id.to_string().starts_with("tls.chain@"))
        .expect("chain block");
    let Shape::Chain(items) = &chain.shape else {
        panic!("chain shape");
    };
    let got = items[0]
        .iter()
        .find(|(k, _)| k == "ca_issuers_url")
        .map(|(_, v)| v.clone());
    assert_eq!(
        got,
        Some(Value::Text("http://ca.example.com/ca.crt".into()))
    );
    assert!(
        REGISTER
            .keys
            .iter()
            .any(|(b, k, t)| *b == "tls.chain" && *k == "ca_issuers_url" && *t == ValueType::Text),
        "ca_issuers_url registered as Text"
    );
}

#[tokio::test]
async fn volatile_includes_tlsa_and_caa_ttl() {
    let cfg: ModuleConfig = toml::from_str("[limits]\nmax_ports = 3").unwrap();
    let module = TlsModule::new(cfg, None).await.expect("module builds");
    let got: BTreeSet<String> = module
        .volatile()
        .iter()
        .map(|p: &EvidencePath| p.0.to_string())
        .collect();
    for path in ["tls.tlsa/ttl", "tls.caa/ttl"] {
        assert!(got.contains(path), "{path} volatile: {got:?}");
        let (block, key, _) = EvidencePath::parse(path).unwrap();
        let name = format!("{}.{}", block.module(), block.name());
        assert!(
            REGISTER
                .keys
                .iter()
                .any(|(b, k, _)| *b == name && *k == key),
            "{path} registered"
        );
    }
}

#[test]
fn judged_per_address_checks_reference_their_block_kind() {
    let mut doc = fixture("tlsight-inspect.json");
    doc["summary"]["checks"]["ct_logged"] = json!("pass");
    let (checks, blocks) = measured(translate(&response(doc)));

    let handshake = [
        "tls_version",
        "forward_secrecy",
        "aead_cipher",
        "ech_advertised",
    ];
    let chain = [
        "not_expired",
        "chain_trusted",
        "hostname_match",
        "chain_complete",
        "strong_signature",
        "key_strength",
        "expiry_window",
        "cert_lifetime",
        "san_quality",
    ];
    let mut wrong = Vec::new();
    let mut verify = |name: &str, prefix: &str| {
        let ids = ref_ids(check(&checks, name));
        if ids.is_empty() || !ids.iter().all(|i| i == prefix || i.starts_with(prefix)) {
            wrong.push(format!("{name}: {ids:?}, want {prefix}*"));
        }
    };
    for n in handshake {
        verify(n, "tls.handshake@");
    }
    for n in chain {
        verify(n, "tls.chain@");
    }
    assert!(blocks.iter().any(|b| b.id.to_string() == "tls.ct"));
    verify("ct_logged", "tls.ct");
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn unknown_expiry_has_no_days_remaining_and_zero_key_size_is_just_the_type() {
    let mut doc = fixture("tlsight-inspect.json");
    let leaf = doc["ports"][0]["ips"][0]["chain"][0].clone();
    let mut placeholder = leaf.clone();
    placeholder["subject"] = json!("<parse error: truncated>");
    placeholder["not_before"] = json!("");
    placeholder["not_after"] = json!("");
    placeholder["days_remaining"] = json!(0);
    placeholder["key_type"] = json!("");
    placeholder["key_size"] = json!(0);
    let mut zero_key = leaf;
    zero_key["key_type"] = json!("RSA");
    zero_key["key_size"] = json!(0);
    doc["ports"][0]["ips"][0]["chain"] = json!([placeholder, zero_key]);
    let (_, blocks) = measured(translate(&response(doc)));

    let chain = blocks
        .iter()
        .find(|b| b.id.to_string().starts_with("tls.chain@"))
        .expect("chain block");
    let S::Chain(items) = &chain.shape else {
        panic!("chain shape");
    };
    assert!(
        !items[0].iter().any(|(k, _)| k == "days_remaining"),
        "placeholder carries days_remaining: {:?}",
        items[0]
    );
    let key = items[1].iter().find(|(k, _)| k == "key").map(|(_, v)| v);
    assert_eq!(key, Some(&Value::Text("RSA".into())));
}
