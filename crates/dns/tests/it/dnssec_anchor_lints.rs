// Table: `dns.dnssec` and `dns.dnssec_rollover` over DNSKEY and DS sets
// (specs/features/pustina-findings, C2, C3, C8, C9, C10).
//
// Lookups are built through their public serde form, as in `lint_results_table.rs`.

use mhost::lints::CheckResult;
use mhost::resolver::Lookups;
use netray_dns::api::check::lint_lookups;
use serde_json::{Value, json};

const DOMAIN: &str = "example.com.";
const NS: &str = "udp:1.1.1.1:53";

fn record(rtype: &str, data: Value) -> Value {
    json!({ "name": DOMAIN, "type": rtype, "ttl": 3600, "data": data })
}

fn dnskey(flags: u16, key_tag: u16, sep: bool) -> Value {
    record(
        "DNSKEY",
        json!({ "DNSKEY": {
            "flags": flags, "protocol": 3, "algorithm": "EcdsaP256Sha256",
            "public_key": "mdsswUyr3DPW132mOi8V9xESWE8jTo0dxCjjnopKl+GqJxpVXckHAeF+KkxLbxILfDLUT0rAK9iUzy1L53eKGQ==",
            "key_tag": key_tag, "is_zone_key": true, "is_secure_entry_point": sep, "is_revoked": false
        }}),
    )
}

fn ds(key_tag: u16) -> Value {
    record(
        "DS",
        json!({ "DS": {
            "key_tag": key_tag, "algorithm": "EcdsaP256Sha256", "digest_type": "Sha256",
            "digest": "2BB183AF5F22588179A53B0A98631FAD1A292118"
        }}),
    )
}

fn lookup(rtype: &str, records: Vec<Value>) -> Value {
    json!({
        "query": { "name": DOMAIN, "record_type": rtype },
        "name_server": NS,
        "result": { "Response": {
            "records": records,
            "response_time": { "secs": 0, "nanos": 20_000_000 },
            "valid_until": "2026-01-01T00:00:00Z"
        }}
    })
}

fn lookups(keys: Vec<Value>, ds_set: Vec<Value>) -> Lookups {
    let mut items = Vec::new();
    if !keys.is_empty() {
        items.push(lookup("DNSKEY", keys));
    }
    if !ds_set.is_empty() {
        items.push(lookup("DS", ds_set));
    }
    serde_json::from_value(json!({ "lookups": items })).expect("lookups deserialise")
}

fn category<'a>(out: &'a [(&'static str, Vec<CheckResult>)], name: &str) -> &'a [CheckResult] {
    &out.iter().find(|(c, _)| *c == name).expect("category").1
}

#[derive(Debug, PartialEq)]
enum Outcome {
    NotApplicable,
    Pass,
    Warn,
    Fail,
}

fn outcome(results: &[CheckResult]) -> Outcome {
    if results.iter().any(|r| r.is_failed()) {
        Outcome::Fail
    } else if results.iter().any(|r| r.is_warning()) {
        Outcome::Warn
    } else if results.iter().all(|r| matches!(r, CheckResult::NotFound())) {
        Outcome::NotApplicable
    } else {
        Outcome::Pass
    }
}

fn text(results: &[CheckResult]) -> String {
    format!("{results:?}")
}

type Row = (
    &'static str,
    Vec<Value>,
    Vec<Value>,
    Outcome,
    &'static str,
    Outcome,
    &'static str,
);

#[test]
fn dnssec_anchor_lints_table() {
    // (name, keys, ds, dnssec, dnssec finding fragment, rollover, rollover fragment)
    let rows: Vec<Row> = vec![
        (
            "C2/C8 signed, empty DS",
            vec![dnskey(257, 2371, true), dnskey(256, 45104, false)],
            vec![],
            Outcome::Fail,
            "not anchored in its parent zone",
            Outcome::NotApplicable,
            "",
        ),
        (
            "C9 signed, matching DS",
            vec![dnskey(257, 2371, true), dnskey(256, 45104, false)],
            vec![ds(2371)],
            Outcome::Pass,
            "",
            Outcome::Pass,
            "",
        ),
        (
            "C10 two KSKs, DS matches one",
            vec![
                dnskey(257, 2371, true),
                dnskey(257, 5555, true),
                dnskey(256, 45104, false),
            ],
            vec![ds(2371)],
            Outcome::Pass,
            "",
            Outcome::Warn,
            "KSK with key tag 5555 has no matching DS",
        ),
        (
            "C3 two KSKs, empty DS",
            vec![dnskey(257, 2371, true), dnskey(257, 5555, true)],
            vec![],
            Outcome::Fail,
            "not anchored in its parent zone",
            Outcome::Warn,
            "KSK with key tag 2371 has no matching DS",
        ),
        (
            "C3 orphaned DS unchanged",
            vec![dnskey(257, 2371, true)],
            vec![ds(2371), ds(9999)],
            Outcome::Fail,
            "DS key tag 9999 has no matching DNSKEY",
            Outcome::Fail,
            "DS key tag 9999 has no matching DNSKEY",
        ),
        (
            "unsigned zone unchanged",
            vec![],
            vec![],
            Outcome::Warn,
            "not DNSSEC-signed",
            Outcome::NotApplicable,
            "",
        ),
    ];

    for (name, keys, ds_set, dnssec, dnssec_text, rollover, rollover_text) in rows {
        let out = lint_lookups(&lookups(keys, ds_set));
        let d = category(&out, "dnssec");
        let r = category(&out, "dnssec_rollover");
        assert_eq!(
            outcome(d),
            dnssec,
            "{name}: dnssec outcome, got {}",
            text(d)
        );
        assert!(
            text(d).contains(dnssec_text),
            "{name}: dnssec finding, got {}",
            text(d)
        );
        assert_eq!(
            outcome(r),
            rollover,
            "{name}: rollover outcome, got {}",
            text(r)
        );
        assert!(
            text(r).contains(rollover_text),
            "{name}: rollover finding, got {}",
            text(r)
        );
    }
}
