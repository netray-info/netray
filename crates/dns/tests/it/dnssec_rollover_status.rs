// A lone KSK with an empty DS set is `not_applicable` for `dns.dnssec_rollover` at the module
// level, not `fail` (specs/features/pustina-findings, requirement 3).
//
// Seam: `lint_lookups` over DNSKEY 257 + 256 and no DS, its categories handed to
// `netray_dns::translate` as lint events, the path that builds the run's status per check id.

use mhost::resolver::Lookups;
use netray_dns::api::LintEvent;
use netray_dns::api::check::lint_lookups;
use netray_dns::{CheckEvent, translate};
use netray_engine::SectionOutcome;
use netray_model::Status;
use serde_json::{Value, json};

const DOMAIN: &str = "example.com.";

fn dnskey(flags: u16, key_tag: u16, sep: bool) -> Value {
    json!({ "name": DOMAIN, "type": "DNSKEY", "ttl": 3600, "data": { "DNSKEY": {
        "flags": flags, "protocol": 3, "algorithm": "EcdsaP256Sha256",
        "public_key": "mdsswUyr3DPW132mOi8V9xESWE8jTo0dxCjjnopKl+GqJxpVXckHAeF+KkxLbxILfDLUT0rAK9iUzy1L53eKGQ==",
        "key_tag": key_tag, "is_zone_key": true, "is_secure_entry_point": sep, "is_revoked": false
    }}})
}

#[test]
fn lone_ksk_with_empty_ds_rollover_is_not_applicable() {
    let lookups: Lookups = serde_json::from_value(json!({ "lookups": [{
        "query": { "name": DOMAIN, "record_type": "DNSKEY" },
        "name_server": "udp:1.1.1.1:53",
        "result": { "Response": {
            "records": [dnskey(257, 2371, true), dnskey(256, 45104, false)],
            "response_time": { "secs": 0, "nanos": 20_000_000 },
            "valid_until": "2026-01-01T00:00:00Z"
        }}
    }]}))
    .expect("lookups deserialise");

    let events: Vec<CheckEvent> = lint_lookups(&lookups)
        .into_iter()
        .map(|(category, results)| {
            CheckEvent::Lint(LintEvent {
                request_id: "r".to_owned(),
                category,
                results,
            })
        })
        .collect();

    let SectionOutcome::Measured { checks, .. } = translate(&events) else {
        panic!("section is measured");
    };
    let rollover = checks
        .iter()
        .find(|c| c.id.to_string() == "dns.dnssec_rollover")
        .expect("dns.dnssec_rollover reported");
    assert_eq!(
        rollover.status,
        Status::NotApplicable,
        "findings: {:?}",
        rollover.findings
    );
}
