// v2-tls-versions-coop, Phase 2 (requirements 5 and 6, criteria C2-C9, C11, C12): the probes
// of an address become a `tls.probes@<address>:<port>` fields block, the probes of a port a
// `tls.protocols` block; an unanswered probe is omitted, addresses that differ read `mixed`,
// and neither block is volatile.
//
// Spec: specs/features/v2-tls-versions-coop/spec.md, Phase 2.
//
// Interface assumed (does not exist yet):
// - `netray_tls::tls::IpInspectionResult::probes: Option<ProtocolProbes>`, `#[serde(skip)]`

use std::collections::BTreeSet;
use std::path::PathBuf;

use netray_engine::{Module, SectionOutcome};
use netray_model::evidence::{Block, Shape, Value, validate};
use netray_tls::inspection::InspectResponse;
use netray_tls::module::{REGISTER, translate_ports};
use netray_tls::tls::probe::{ProbeOutcome, ProtocolProbes, Verdict};
use netray_tls::{ModuleConfig, TlsModule, translate};
use serde_json::{Value as Json, json};

fn fixture() -> Json {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts/tlsight-inspect.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn response(doc: Json) -> InspectResponse {
    serde_json::from_value(doc).unwrap()
}

/// The fixture with its first address cloned to `192.0.2.11` on port 443.
fn two_addresses() -> Json {
    let mut doc = fixture();
    let mut second = doc["ports"][0]["ips"][0].clone();
    second["ip"] = json!("192.0.2.11");
    doc["ports"][0]["ips"].as_array_mut().unwrap().push(second);
    doc
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

fn out(verdict: Verdict, detail: &str) -> ProbeOutcome {
    ProbeOutcome {
        verdict,
        version: None,
        cipher_suite: None,
        detail: detail.into(),
    }
}

fn canned() -> ProtocolProbes {
    ProtocolProbes {
        ssl3: out(Verdict::Refused, "alert protocol_version"),
        tls1_0: out(
            Verdict::Accepted,
            "server_hello TLSv1.0 TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA",
        ),
        tls1_1: out(Verdict::Refused, "alert protocol_version"),
        tls1_2: out(Verdict::Accepted, "handshake TLSv1.2"),
        tls1_3: out(Verdict::Refused, "handshake TLSv1.2"),
        suites_insecure: out(Verdict::Refused, "alert handshake_failure"),
        suites_cbc: out(
            Verdict::Accepted,
            "server_hello TLSv1.2 TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA",
        ),
        suites_static_rsa: out(Verdict::Refused, "alert handshake_failure"),
    }
}

fn set(resp: &mut InspectResponse, port: usize, ip: usize, probes: ProtocolProbes) {
    resp.ports[port].ips[ip].probes = Some(probes);
}

fn evidence(outcome: &SectionOutcome) -> &Vec<Block> {
    match outcome {
        SectionOutcome::Measured { evidence, .. } => evidence,
        other => panic!("expected Measured, got {other:?}"),
    }
}

fn block<'a>(outcome: &'a SectionOutcome, id: &str) -> Option<&'a Vec<(String, Value)>> {
    evidence(outcome)
        .iter()
        .find(|b| b.id.to_string() == id)
        .map(|b| match &b.shape {
            Shape::Fields(f) => f,
            other => panic!("{id} is a fields block: {other:?}"),
        })
}

fn get<'a>(fields: &'a [(String, Value)], key: &str) -> Option<&'a Value> {
    fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn valid(outcome: &SectionOutcome) {
    validate("tls", &REGISTER, evidence(outcome)).unwrap_or_else(|e| panic!("{e:?}"));
}

#[test]
fn canned_probes_give_probes_and_protocols_blocks() {
    let mut resp = response(fixture());
    set(&mut resp, 0, 0, canned());
    let outcome = translate(&resp);

    let probes = block(&outcome, "tls.probes@192.0.2.10:443").expect("tls.probes block");
    let want: &[(&str, &str, &str)] = &[
        ("ssl3", "disabled", "alert protocol_version"),
        (
            "tls1_0",
            "enabled",
            "server_hello TLSv1.0 TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA",
        ),
        ("tls1_1", "disabled", "alert protocol_version"),
        ("tls1_2", "enabled", "handshake TLSv1.2"),
        ("tls1_3", "disabled", "handshake TLSv1.2"),
        ("suites_insecure", "refused", "alert handshake_failure"),
        (
            "suites_cbc",
            "accepted",
            "server_hello TLSv1.2 TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA",
        ),
        ("suites_static_rsa", "refused", "alert handshake_failure"),
    ];
    for (key, value, answer) in want {
        assert_eq!(get(probes, key), Some(&text(value)), "{key}");
        assert_eq!(
            get(probes, &format!("{key}_answer")),
            Some(&text(answer)),
            "{key}_answer"
        );
    }
    assert_eq!(probes.len(), 16, "{probes:?}");

    let protocols = block(&outcome, "tls.protocols").expect("tls.protocols block");
    for (key, value, _) in want {
        assert_eq!(get(protocols, key), Some(&text(value)), "{key}");
    }
    assert_eq!(
        get(protocols, "forward_secrecy_all_suites"),
        Some(&Value::Bool(true))
    );
    valid(&outcome);
}

#[test]
fn an_unanswered_probe_is_omitted() {
    let mut resp = response(fixture());
    let mut probes = canned();
    probes.tls1_0 = out(Verdict::Unanswered, "timeout");
    set(&mut resp, 0, 0, probes);
    let outcome = translate(&resp);

    let probes = block(&outcome, "tls.probes@192.0.2.10:443").unwrap();
    assert!(get(probes, "tls1_0").is_none(), "{probes:?}");
    assert!(get(probes, "tls1_0_answer").is_none(), "{probes:?}");
    assert!(get(probes, "tls1_1").is_some());
    let protocols = block(&outcome, "tls.protocols").unwrap();
    assert!(get(protocols, "tls1_0").is_none(), "{protocols:?}");
    valid(&outcome);
}

#[test]
fn differing_addresses_read_mixed_and_agreeing_ones_their_value() {
    let table = [
        (Verdict::Accepted, Verdict::Refused, Some("mixed")),
        (Verdict::Refused, Verdict::Refused, Some("disabled")),
        (Verdict::Accepted, Verdict::Accepted, Some("enabled")),
        (Verdict::Refused, Verdict::Unanswered, None),
    ];
    for (a, b, want) in table {
        let mut resp = response(two_addresses());
        let mut first = canned();
        first.tls1_0 = out(a, "first");
        let mut second = canned();
        second.tls1_0 = out(b, "second");
        set(&mut resp, 0, 0, first);
        set(&mut resp, 0, 1, second);
        let outcome = translate(&resp);
        let protocols = block(&outcome, "tls.protocols").unwrap();
        assert_eq!(
            get(protocols, "tls1_0"),
            want.map(text).as_ref(),
            "{a:?} / {b:?}"
        );
        valid(&outcome);
    }
}

#[test]
fn forward_secrecy_all_suites_follows_the_static_rsa_probe() {
    let table = [
        (Verdict::Accepted, Verdict::Refused, Some(false)),
        (Verdict::Refused, Verdict::Refused, Some(true)),
        (Verdict::Unanswered, Verdict::Refused, None),
        (Verdict::Unanswered, Verdict::Accepted, Some(false)),
    ];
    for (a, b, want) in table {
        let mut resp = response(two_addresses());
        let mut first = canned();
        first.suites_static_rsa = out(a, "first");
        let mut second = canned();
        second.suites_static_rsa = out(b, "second");
        set(&mut resp, 0, 0, first);
        set(&mut resp, 0, 1, second);
        let outcome = translate(&resp);
        let protocols = block(&outcome, "tls.protocols").unwrap();
        assert_eq!(
            get(protocols, "forward_secrecy_all_suites"),
            want.map(Value::Bool).as_ref(),
            "{a:?} / {b:?}"
        );
        valid(&outcome);
    }
}

#[test]
fn no_probes_no_blocks() {
    let outcome = translate(&response(fixture()));
    for b in evidence(&outcome) {
        let id = b.id.to_string();
        assert!(
            !id.starts_with("tls.probes") && !id.starts_with("tls.protocols"),
            "{id}"
        );
    }
}

#[test]
fn each_port_holds_its_own_protocols_block() {
    let mut resp = response(two_ports());
    set(&mut resp, 0, 0, canned());
    set(&mut resp, 1, 0, canned());
    let outcomes = translate_ports(&resp);
    for (port, outcome) in &outcomes {
        assert!(
            block(outcome, &format!("tls.protocols@{port}")).is_some(),
            "port {port}"
        );
        assert!(
            block(outcome, &format!("tls.probes@192.0.2.10:{port}")).is_some(),
            "port {port}"
        );
        assert!(block(outcome, "tls.protocols").is_none(), "port {port}");
        valid(outcome);
    }
}

#[test]
fn probes_are_not_serialised() {
    let mut resp = response(fixture());
    set(&mut resp, 0, 0, canned());
    let doc = serde_json::to_value(&resp.ports[0].ips[0]).unwrap();
    assert!(doc.get("probes").is_none(), "{doc}");
}

#[tokio::test]
async fn probe_blocks_are_not_volatile() {
    let cfg: ModuleConfig = toml::from_str("[limits]\nmax_ports = 3").unwrap();
    let module = TlsModule::new(cfg, None).await.expect("module builds");
    let got: BTreeSet<&str> = module.volatile().iter().map(|p| p.0).collect();
    for path in &got {
        assert!(
            !path.starts_with("tls.probes") && !path.starts_with("tls.protocols"),
            "{path} is volatile"
        );
    }
}
