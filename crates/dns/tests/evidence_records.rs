// V2 evidence, record-level rules of the records blocks (criteria C8-C10):
// C8 a record answered with different TTLs keeps the maximum TTL, in `translate` and in
//    `facts_from_lookups` alike;
// C9 records are in canonical order (owner, type, data), so the order of the answers does not
//    change the block;
// C10 `dns.dnssec` references `dns.rrsig` and `dns.nsec` besides `dns.dnskey` and `dns.ds`,
//    only blocks that exist.
//
// Spec: specs/features/v2-evidence/spec.md. Seams: `translate`, `facts_from_lookups`.

use std::collections::BTreeSet;

use mhost::resolver::Lookups;
use netray_dns::api::{BatchEvent, LintEvent};
use netray_dns::module::REGISTER;
use netray_dns::{CheckEvent, facts_from_lookups, translate};
use netray_engine::SectionOutcome;
use netray_model::evidence::{Block, Shape, Value as Ev, validate};
use serde_json::{Value, json};

const NAME: &str = "example.com.";
const R1: &str = "192.0.2.53";
const R2: &str = "192.0.2.54";

fn resp_time() -> Value {
    json!({ "secs": 0, "nanos": 12_000_000 })
}

fn response(records: Vec<Value>) -> Value {
    json!({ "Response": {
        "records": records,
        "response_time": resp_time(),
        "valid_until": "2030-01-01T00:00:00Z"
    }})
}

fn noerror_empty() -> Value {
    json!({ "NxDomain": { "response_time": resp_time(), "response_code": "NOERROR" } })
}

fn record(rtype: &str, data: Value, ttl: u32) -> Value {
    json!({ "data": { rtype: data }, "name": NAME, "ttl": ttl, "type": rtype })
}

fn lookups(record_type: &str, results: Vec<(&str, Value)>) -> Lookups {
    let entries: Vec<Value> = results
        .into_iter()
        .map(|(server, result)| {
            json!({
                "query": { "name": NAME, "record_type": record_type },
                "name_server": format!("udp:{server}:53"),
                "result": result
            })
        })
        .collect();
    serde_json::from_value(json!({ "lookups": entries })).expect("Lookups fixture")
}

fn batch(record_type: &str, lookups: Lookups) -> CheckEvent {
    CheckEvent::Batch(BatchEvent {
        request_id: "r".to_owned(),
        record_type: record_type.to_owned(),
        lookups,
        completed: 1,
        total: 1,
        transport: None,
        source: None,
    })
}

fn lint_not_found(category: &str) -> CheckEvent {
    CheckEvent::Lint(
        serde_json::from_value::<LintEvent>(json!({
            "request_id": "r", "category": category, "results": [{ "NotFound": [] }]
        }))
        .unwrap(),
    )
}

fn measured(events: &[CheckEvent]) -> (Vec<netray_model::CheckResult>, Vec<Block>) {
    match translate(events) {
        SectionOutcome::Measured {
            checks, evidence, ..
        } => (checks, evidence),
        other => panic!("expected Measured, got {other:?}"),
    }
}

fn block<'a>(blocks: &'a [Block], id: &str) -> &'a Block {
    blocks
        .iter()
        .find(|b| b.id.to_string() == id)
        .unwrap_or_else(|| panic!("no {id} block in {blocks:?}"))
}

fn records<'a>(blocks: &'a [Block], id: &str) -> &'a [netray_model::evidence::Record] {
    match &block(blocks, id).shape {
        Shape::Records(r) => r,
        other => panic!("{id} is not records: {other:?}"),
    }
}

#[test]
fn a_record_answered_with_different_ttls_keeps_the_maximum() {
    let a = lookups(
        "A",
        vec![
            (R1, response(vec![record("A", json!("192.0.2.10"), 34)])),
            (R2, response(vec![record("A", json!("192.0.2.10"), 3600)])),
        ],
    );

    let (_, evidence) = measured(&[batch("A", a.clone()), lint_not_found("caa")]);
    let from_translate = records(&evidence, "dns.a");
    assert_eq!(from_translate.len(), 1, "translate: {from_translate:?}");
    assert_eq!(from_translate[0].ttl, Ev::Int(3600), "translate: max TTL");

    let facts = facts_from_lookups(&a).blocks;
    let from_facts = records(&facts, "dns.a");
    assert_eq!(from_facts.len(), 1, "facts: {from_facts:?}");
    assert_eq!(from_facts[0].ttl, Ev::Int(3600), "facts: max TTL");
}

#[test]
fn records_are_in_canonical_order_whatever_the_answer_order() {
    let ns = |names: [&str; 2]| {
        let recs = names
            .iter()
            .map(|n| record("NS", json!(n), 300))
            .collect::<Vec<_>>();
        let l = lookups("NS", vec![(R1, response(recs))]);
        let (_, evidence) = measured(&[batch("NS", l), lint_not_found("ns")]);
        evidence
    };
    let forward = ns(["ns1.example.com.", "ns2.example.com."]);
    let reversed = ns(["ns2.example.com.", "ns1.example.com."]);

    assert_eq!(block(&forward, "dns.ns"), block(&reversed, "dns.ns"));
    let data: Vec<&Ev> = records(&reversed, "dns.ns")
        .iter()
        .map(|r| &r.data)
        .collect();
    assert_eq!(
        data,
        vec![
            &Ev::Text("ns1.example.com.".into()),
            &Ev::Text("ns2.example.com.".into())
        ]
    );
    validate("dns", &REGISTER, &reversed).expect("validates");
}

#[test]
fn dnssec_references_every_dnssec_block_that_exists() {
    let empty = |t: &str| {
        batch(
            t,
            lookups(t, vec![(R1, noerror_empty()), (R2, noerror_empty())]),
        )
    };
    let events = [
        empty("DNSKEY"),
        empty("DS"),
        empty("RRSIG"),
        empty("NSEC"),
        lint_not_found("dnssec"),
    ];
    let (checks, evidence) = measured(&events);
    let dnssec = checks
        .iter()
        .find(|c| c.id.to_string() == "dns.dnssec")
        .expect("dns.dnssec");
    let refs: BTreeSet<String> = dnssec.evidence.iter().map(ToString::to_string).collect();
    let want: BTreeSet<String> = ["dns.dnskey", "dns.ds", "dns.rrsig", "dns.nsec"]
        .map(String::from)
        .into();
    assert_eq!(refs, want);
    for id in &refs {
        block(&evidence, id);
    }

    // Only blocks that exist: without the NSEC batch there is no dns.nsec reference.
    let (checks, _) = measured(&[
        empty("DNSKEY"),
        empty("DS"),
        empty("RRSIG"),
        lint_not_found("dnssec"),
    ]);
    let dnssec = checks
        .iter()
        .find(|c| c.id.to_string() == "dns.dnssec")
        .unwrap();
    let refs: BTreeSet<String> = dnssec.evidence.iter().map(ToString::to_string).collect();
    assert!(!refs.contains("dns.nsec"), "{refs:?}");
    assert!(refs.contains("dns.rrsig"), "{refs:?}");
}
