// Pinning table: the lint results mhost 0.12.0 emits for lookups shaped like the ones prism's
// `+check` collects, recorded literally (variant and message, in emitted order).
//
// Spec: specs/features/lens-goldens/spec.md, Phase 2, requirement 6, scenarios C3, C14-C16.
//
// Moved from mhost 0.11.3 to 0.12.0 by specs/features/advisories requirement 7: the signed-zone
// rows no longer carry "DNSKEY present but no RRSIG records found", and `check_ttl` lists a record
// answered by two nameservers once. The KSK/ZSK count still doubles over two nameservers.
//
// Construction: mhost's `Lookup::new_for_test` is `#[cfg(test)]` inside mhost and not reachable
// from here, so the lookups are built through their public serde form (`Lookups` is
// `Deserialize`), which is the same shape mhost itself serialises.

use mhost::lints::{CheckResult, check_dnssec, check_ttl};
use mhost::resolver::Lookups;
use serde_json::{Value, json};

const DOMAIN: &str = "example.com.";

fn record(rtype: &str, ttl: u32, data: Value) -> Value {
    json!({ "name": DOMAIN, "type": rtype, "ttl": ttl, "data": data })
}

fn dnskey(flags: u16, key_tag: u16, sep: bool) -> Value {
    json!({ "DNSKEY": {
        "flags": flags, "protocol": 3, "algorithm": "EcdsaP256Sha256",
        "public_key": "mdsswUyr3DPW132mOi8V9xESWE8jTo0dxCjjnopKl+GqJxpVXckHAeF+KkxLbxILfDLUT0rAK9iUzy1L53eKGQ==",
        "key_tag": key_tag, "is_zone_key": true, "is_secure_entry_point": sep, "is_revoked": false
    }})
}

fn lookup(ns: &str, rtype: &str, records: Vec<Value>) -> Value {
    json!({
        "query": { "name": DOMAIN, "record_type": rtype },
        "name_server": ns,
        "result": { "Response": {
            "records": records,
            "response_time": { "secs": 0, "nanos": 20_000_000 },
            "valid_until": "2026-01-01T00:00:00Z"
        }}
    })
}

fn lookups(items: Vec<Value>) -> Lookups {
    serde_json::from_value(json!({ "lookups": items })).expect("lookups deserialise")
}

/// Signed zone as a stub sees it: DNSKEY answered, no RRSIG in any answer (no DO bit).
fn signed_zone(ns: &str) -> Vec<Value> {
    vec![
        lookup(
            ns,
            "DNSKEY",
            vec![
                record("DNSKEY", 3600, dnskey(257, 2371, true)),
                record("DNSKEY", 3600, dnskey(256, 45104, false)),
            ],
        ),
        lookup(
            ns,
            "A",
            vec![record("A", 300, json!({ "A": "93.184.216.34" }))],
        ),
    ]
}

/// One A record set whose records carry differing TTLs.
fn differing_ttls(ns: &str) -> Vec<Value> {
    vec![lookup(
        ns,
        "A",
        vec![
            record("A", 300, json!({ "A": "192.0.2.1" })),
            record("A", 30, json!({ "A": "192.0.2.2" })),
            record("A", 3600, json!({ "A": "192.0.2.3" })),
        ],
    )]
}

fn render(results: &[CheckResult]) -> Vec<String> {
    results
        .iter()
        .map(|r| match r {
            CheckResult::Ok(m) => format!("Ok: {m}"),
            CheckResult::Warning(m) => format!("Warning: {m}"),
            CheckResult::Failed(m) => format!("Failed: {m}"),
            CheckResult::NotFound() => "NotFound".to_owned(),
        })
        .collect()
}

struct Row {
    name: &'static str,
    lookups: Lookups,
    dnssec: &'static [&'static str],
    ttl: &'static [&'static str],
}

const NO_DNSSEC: &str = "Warning: No DNSSEC records found: domain is not DNSSEC-signed, DNS responses cannot be authenticated";
const NS_MX_OK: &str = "Ok: No NS/MX records with excessively high TTL";

#[test]
fn lint_results_table() {
    let one = "udp:1.1.1.1:53";
    let two = "udp:8.8.8.8:53";
    let both = |f: fn(&str) -> Vec<Value>| {
        let mut v = f(one);
        v.extend(f(two));
        lookups(v)
    };

    let rows = vec![
        // C14: DNSKEY answered, no RRSIG anywhere.
        Row {
            name: "C14 signed zone answered to a stub lookup",
            lookups: lookups(signed_zone(one)),
            dnssec: &[
                "Ok: Domain has DNSSEC records: DNSKEY",
                "Ok: Found 1 KSK(s) and 1 ZSK(s)",
                "Ok: Algorithm ECDSA P-256/SHA-256 is secure",
            ],
            ttl: &["Ok: No records with TTL below 60s", NS_MX_OK],
        },
        // C15: one A record set, TTLs 300 / 30 / 3600.
        Row {
            name: "C15 differing TTLs in one record set",
            lookups: lookups(differing_ttls(one)),
            dnssec: &[NO_DNSSEC],
            ttl: &[
                "Warning: Records with very low TTL (<60s): example.com. (30s, A). This causes excessive query load",
                NS_MX_OK,
            ],
        },
        // C16: identical answers from two nameservers.
        Row {
            name: "C16 signed zone answered identically by two nameservers",
            lookups: both(signed_zone),
            dnssec: &[
                "Ok: Domain has DNSSEC records: DNSKEY",
                "Ok: Found 2 KSK(s) and 2 ZSK(s)",
                "Ok: Algorithm ECDSA P-256/SHA-256 is secure",
            ],
            ttl: &["Ok: No records with TTL below 60s", NS_MX_OK],
        },
        Row {
            name: "C16 differing TTLs answered identically by two nameservers",
            lookups: both(differing_ttls),
            dnssec: &[NO_DNSSEC],
            ttl: &[
                "Warning: Records with very low TTL (<60s): example.com. (30s, A). This causes excessive query load",
                NS_MX_OK,
            ],
        },
    ];

    for row in rows {
        assert_eq!(
            render(&check_dnssec(&row.lookups)),
            row.dnssec,
            "{}: check_dnssec",
            row.name
        );
        assert_eq!(
            render(&check_ttl(&row.lookups)),
            row.ttl,
            "{}: check_ttl",
            row.name
        );
    }
}
