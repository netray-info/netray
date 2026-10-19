// V2 evidence, Phase 3 (requirements 4 and 5, criteria C1-C6): the TLS register, the blocks
// `translate_with_facts` emits, the references of each judged check and the volatile paths.
//
// Spec: specs/features/v2-evidence/spec.md, Phase 3.
//
// Interface assumed (does not exist yet):
// - `netray_tls::module::REGISTER: netray_model::evidence::Register`
// - `netray_tls::module::translate_with_facts(&InspectResponse, &[BlockId]) -> SectionOutcome`;
//   `translate(resp)` is `translate_with_facts(resp, &[])`.
// - blocks `tls.addresses`, `tls.chain@<addr>:<port>`, `tls.handshake@<addr>:<port>`,
//   `tls.tlsa@<port>`, `tls.ct`, `tls.caa` (only when the facts lack `dns.caa`).
// - `TlsModule::volatile()` lists `tls.chain/days_remaining` and `tls.handshake/handshake_ms`.

use std::collections::BTreeSet;
use std::path::PathBuf;

use netray_engine::{EvidencePath, Module, SectionOutcome};
use netray_model::evidence::{
    Block, BlockId, Shape, Value, ValueType, check_evidence_rule, validate,
};
use netray_model::{CheckResult, Status};
use netray_tls::module::{REGISTER, translate_with_facts};
use netray_tls::routes::InspectResponse;
use netray_tls::{ModuleConfig, TlsModule, translate};
use serde_json::{Value as Json, json};

const A: &str = "192.0.2.10";
const B: &str = "192.0.2.11";

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

fn refs(c: &CheckResult) -> BTreeSet<String> {
    c.evidence.iter().map(ToString::to_string).collect()
}

fn dns_caa_facts() -> (Vec<BlockId>, Block) {
    let id = BlockId::parse("dns.caa").unwrap();
    let block = Block {
        id: id.clone(),
        title: "CAA".into(),
        shape: Shape::Records(vec![]),
    };
    (vec![id], block)
}

#[test]
fn fixture_evidence_validates_and_satisfies_the_evidence_rule() {
    let resp = response(fixture("tlsight-inspect.json"));

    // Own view: no facts.
    let (checks, blocks) = measured(translate(&resp));
    validate("tls", &REGISTER, &blocks)
        .unwrap_or_else(|e| panic!("evidence does not validate: {e:?}"));
    check_evidence_rule(&checks, &blocks).unwrap_or_else(|e| panic!("evidence rule: {e:?}"));
    assert_eq!(
        refs(check(&checks, "not_expired")),
        BTreeSet::from([format!("tls.chain@{A}:443")])
    );

    // With the facts' dns.caa: the rule holds once the run adds that block.
    let (ids, caa) = dns_caa_facts();
    let (checks, mut blocks) = measured(translate_with_facts(&resp, &ids));
    validate("tls", &REGISTER, &blocks)
        .unwrap_or_else(|e| panic!("evidence does not validate with facts: {e:?}"));
    blocks.push(caa);
    check_evidence_rule(&checks, &blocks)
        .unwrap_or_else(|e| panic!("evidence rule with facts: {e:?}"));
}

#[test]
fn caa_compliant_references_the_facts_block_or_its_own() {
    let resp = response(fixture("tlsight-inspect.json"));

    let (ids, _) = dns_caa_facts();
    let (checks, blocks) = measured(translate_with_facts(&resp, &ids));
    assert_eq!(
        refs(check(&checks, "caa_compliant")),
        BTreeSet::from(["dns.caa".to_string()])
    );
    assert!(
        !blocks.iter().any(|b| b.id.to_string() == "tls.caa"),
        "no tls.caa when the facts carry dns.caa"
    );

    let (checks, blocks) = measured(translate(&resp));
    assert_eq!(
        refs(check(&checks, "caa_compliant")),
        BTreeSet::from(["tls.caa".to_string()])
    );
    assert!(
        blocks.iter().any(|b| b.id.to_string() == "tls.caa"),
        "tls.caa exists without facts"
    );
}

#[test]
fn timed_out_address_has_a_row_but_no_chain_or_handshake() {
    let mut doc = fixture("tlsight-inspect.json");
    let port = doc["ports"][0].as_object_mut().unwrap();
    port.remove("quality");
    port.remove("consistency");
    let mut valid = port["ips"][0].clone();
    valid["ip"] = json!(A);
    let timeout = json!({
        "ip": B,
        "ip_version": "v4",
        "error": { "code": "HANDSHAKE_TIMEOUT", "message": "handshake timed out" }
    });
    port.insert("ips".into(), json!([valid, timeout]));

    let (_, blocks) = measured(translate(&response(doc)));
    validate("tls", &REGISTER, &blocks).expect("validates");

    let ids: Vec<String> = blocks.iter().map(|b| b.id.to_string()).collect();
    assert!(
        ids.iter().any(|i| i == &format!("tls.chain@{A}:443")),
        "{ids:?}"
    );
    assert!(
        !ids.iter().any(|i| i.contains(B)),
        "no chain or handshake block for the timed-out address: {ids:?}"
    );

    let table = blocks
        .iter()
        .find(|b| b.id.to_string() == "tls.addresses")
        .expect("tls.addresses emitted");
    let Shape::Table { columns, rows, .. } = &table.shape else {
        panic!("tls.addresses must be a table");
    };
    let col = |name: &str| columns.iter().position(|c| c == name).expect(name);
    let row = rows
        .iter()
        .find(|r| r[col("address")] == Value::Addr(B.parse().unwrap()))
        .unwrap_or_else(|| panic!("row for {B}: {rows:?}"));
    assert_eq!(row[col("outcome")], Value::Text("HANDSHAKE_TIMEOUT".into()));
}

#[test]
fn unreachable_fail_references_the_addresses_block() {
    let resp = response(fixture("tlsight-unreachable.json"));
    let (checks, blocks) = measured(translate(&resp));
    let reachable = check(&checks, "tls_reachable");
    assert_eq!(reachable.status, Status::Fail);
    assert_eq!(
        refs(reachable),
        BTreeSet::from(["tls.addresses".to_string()])
    );
    check_evidence_rule(&checks, &blocks).unwrap_or_else(|e| panic!("evidence rule: {e:?}"));
}

#[tokio::test]
async fn volatile_paths_are_registered_keys() {
    let cfg: ModuleConfig = toml::from_str("[limits]\nmax_ports = 3").unwrap();
    let module = TlsModule::new(cfg).await.expect("module builds");
    let got: BTreeSet<String> = module
        .volatile()
        .iter()
        .map(|p: &EvidencePath| p.0.to_string())
        .collect();
    let want: BTreeSet<String> = [
        "tls.chain/days_remaining",
        "tls.handshake/handshake_ms",
        "tls.handshake/ocsp_this_update",
        "tls.handshake/ocsp_next_update",
        "tls.handshake/ocsp_live_status",
        "tls.handshake/ocsp_live_revoked_at",
        "tls.handshake/ocsp_live_reason",
        "tls.tlsa/ttl",
        "tls.caa/ttl",
    ]
    .map(String::from)
    .into();
    assert_eq!(got, want);

    for path in module.volatile() {
        let (block, key, row) = EvidencePath::parse(path.0).expect(path.0);
        assert!(row.is_none() && block.qualifier().is_none(), "{}", path.0);
        let name = format!("{}.{}", block.module(), block.name());
        let typed: Option<ValueType> = REGISTER
            .keys
            .iter()
            .find(|(b, k, _)| *b == name && *k == key)
            .map(|(_, _, t)| *t);
        assert!(typed.is_some(), "{}: key {key} not registered", path.0);
    }
}
