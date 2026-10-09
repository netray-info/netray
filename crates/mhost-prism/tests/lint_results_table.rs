// Pinning table: the lint results prism's own `lint_lookups` emits for lookups shaped like the
// ones prism's `+check` collects, recorded literally (variant and message, in emitted order).
//
// Spec: specs/features/backend-correctness/spec.md, Phase 1, requirement 2 (records counted
// once), scenarios C2, C4-C9. Earlier pins: specs/features/lens-goldens/spec.md, Phase 2,
// requirement 6, scenarios C3, C14-C16.
//
// The lints now run over the unique records of all resolvers (one record per name, type and
// data; of duplicates the one with the highest TTL), so a zone answered identically by two
// resolvers counts as often as by one.
//
// Construction: mhost's `Lookup::new_for_test` is `#[cfg(test)]` inside mhost and not reachable
// from here, so the lookups are built through their public serde form (`Lookups` is
// `Deserialize`), which is the same shape mhost itself serialises.

use mhost::lints::CheckResult;
use mhost::resolver::Lookups;
use prism::api::check::{lint_lookups, unique_lines};
use serde_json::{Value, json};

const DOMAIN: &str = "example.com.";

fn record(rtype: &str, ttl: u32, data: Value) -> Value {
    json!({ "name": DOMAIN, "type": rtype, "ttl": ttl, "data": data })
}

fn dnskey(flags: u16, key_tag: u16, sep: bool) -> Value {
    dnskey_alg(flags, key_tag, sep, "EcdsaP256Sha256")
}

fn dnskey_alg(flags: u16, key_tag: u16, sep: bool, algorithm: &str) -> Value {
    json!({ "DNSKEY": {
        "flags": flags, "protocol": 3, "algorithm": algorithm,
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
            name: "C2/C5 signed zone answered identically by two resolvers",
            lookups: both(signed_zone),
            dnssec: &[
                "Ok: Domain has DNSSEC records: DNSKEY",
                "Ok: Found 1 KSK(s) and 1 ZSK(s)",
                "Ok: Algorithm ECDSA P-256/SHA-256 is secure",
            ],
            ttl: &["Ok: No records with TTL below 60s", NS_MX_OK],
        },
        Row {
            name: "C2/C5 differing TTLs answered identically by two resolvers",
            lookups: both(differing_ttls),
            dnssec: &[NO_DNSSEC],
            ttl: &[
                "Warning: Records with very low TTL (<60s): example.com. (30s, A). This causes excessive query load",
                NS_MX_OK,
            ],
        },
    ];

    for row in rows {
        let out = lint_lookups(&row.lookups);
        assert_eq!(
            render(category(&out, "dnssec")),
            row.dnssec,
            "{}: dnssec",
            row.name
        );
        assert_eq!(
            render(category(&out, "ttl")),
            row.ttl,
            "{}: ttl",
            row.name
        );
    }
}

fn category<'a>(
    out: &'a [(&'static str, Vec<CheckResult>)],
    name: &str,
) -> &'a [CheckResult] {
    &out.iter()
        .find(|(c, _)| *c == name)
        .unwrap_or_else(|| panic!("category {name} missing"))
        .1
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// C4: an RRSIG covering DNSKEY, riding in the DNSKEY answer, expires in 3 days.
#[test]
fn near_expiry_rrsig_is_linted() {
    let now = now_secs();
    let rrsig = record(
        "RRSIG",
        3600,
        json!({ "RRSIG": {
            "type_covered": "DNSKEY", "algorithm": "EcdsaP256Sha256", "labels": 2,
            "original_ttl": 3600,
            "expiration": now + 3 * 86400 + 3600, "inception": now - 86400,
            "key_tag": 2371, "signer_name": DOMAIN, "signature": "c2ln"
        }}),
    );
    let one = "udp:1.1.1.1:53";
    let l = lookups(vec![lookup(
        one,
        "DNSKEY",
        vec![
            record("DNSKEY", 3600, dnskey(257, 2371, true)),
            record("DNSKEY", 3600, dnskey(256, 45104, false)),
            rrsig,
        ],
    )]);
    let out = lint_lookups(&l);
    let lines = render(category(&out, "dnssec"));
    assert!(
        lines
            .iter()
            .any(|x| x == "Warning: RRSIG covering DNSKEY expiring in 3 day(s) (key tag 2371)"),
        "near-expiry line missing: {lines:?}"
    );
}

/// C6: different records for one name from two resolvers both count.
#[test]
fn different_records_from_two_resolvers_both_count() {
    let mut v = vec![lookup(
        "udp:1.1.1.1:53",
        "A",
        vec![record("A", 300, json!({ "A": "192.0.2.1" }))],
    )];
    v.push(lookup(
        "udp:8.8.8.8:53",
        "A",
        vec![record("A", 30, json!({ "A": "192.0.2.2" }))],
    ));
    let out = lint_lookups(&lookups(v));
    assert_eq!(
        render(category(&out, "ttl")),
        [
            "Warning: Records with very low TTL (<60s): example.com. (30s, A). This causes excessive query load",
            NS_MX_OK,
        ]
    );

    // KSK from one resolver, ZSK from the other: 1 and 1.
    let l = lookups(vec![
        lookup(
            "udp:1.1.1.1:53",
            "DNSKEY",
            vec![record("DNSKEY", 3600, dnskey(257, 2371, true))],
        ),
        lookup(
            "udp:8.8.8.8:53",
            "DNSKEY",
            vec![record("DNSKEY", 3600, dnskey(256, 45104, false))],
        ),
    ]);
    let out = lint_lookups(&l);
    assert!(
        render(category(&out, "dnssec")).contains(&"Ok: Found 1 KSK(s) and 1 ZSK(s)".to_owned()),
        "{:?}",
        render(category(&out, "dnssec"))
    );
}

/// C7: one record, TTL 45 from one resolver and 300 from the other: the highest TTL wins.
#[test]
fn same_record_keeps_highest_ttl() {
    let l = lookups(vec![
        lookup(
            "udp:1.1.1.1:53",
            "A",
            vec![record("A", 45, json!({ "A": "192.0.2.1" }))],
        ),
        lookup(
            "udp:8.8.8.8:53",
            "A",
            vec![record("A", 300, json!({ "A": "192.0.2.1" }))],
        ),
    ]);
    let out = lint_lookups(&l);
    assert_eq!(
        render(category(&out, "ttl")),
        ["Ok: No records with TTL below 60s", NS_MX_OK]
    );
}

/// C8: identical lines collapse to one, order kept.
#[test]
fn unique_lines_collapses_duplicates() {
    let got = unique_lines(vec![
        CheckResult::Warning("x".into()),
        CheckResult::Ok("y".into()),
        CheckResult::Warning("x".into()),
    ]);
    assert_eq!(render(&got), ["Warning: x", "Ok: y"]);
}

/// C9: two distinct DNSKEYs with the same deprecated algorithm give one line.
#[test]
fn identical_algorithm_lines_collapse() {
    let l = lookups(vec![lookup(
        "udp:1.1.1.1:53",
        "DNSKEY",
        vec![
            record("DNSKEY", 3600, dnskey_alg(257, 2371, true, "RsaSha1")),
            record("DNSKEY", 3600, dnskey_alg(256, 45104, false, "RsaSha1")),
        ],
    )]);
    let out = lint_lookups(&l);
    assert_eq!(
        render(category(&out, "dnskey_algorithm")),
        ["Warning: DNSKEY uses RSASHA1 (algorithm 5) — deprecated, should not be used (RFC 8624)"]
    );
}
