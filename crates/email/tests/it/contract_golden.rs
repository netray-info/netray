//! Contract golden: the SSE bytes beacon puts on the wire, consumed by
//! `crates/lens/tests/it/contract_beacon.rs`.
//!
//! The events go through beacon's real encoding path (`From<SseEvent> for sse::Event`,
//! then axum's `Sse` response body), so framing is exactly what a client receives.
//! Only JSON key order is canonicalised (`verdicts` is a `HashMap`, whose order is random).
//!
//! `UPDATE_GOLDEN=1 cargo test -p netray-email --test it contract_golden` rewrites beacon.sse,
//! beacon-no-mx.sse, beacon-mx-cname.sse, beacon-null-mx.sse, beacon-sending-no-dkim.sse and
//! beacon-partial.sse.

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::IpAddr;
use std::path::PathBuf;

use axum::response::IntoResponse;
use axum::response::sse::{Event, Sse};
use http_body_util::BodyExt;
use netray_email::checks::SKIPPED;
use netray_email::checks::cross_validation::SENDS_NO_MAIL;
use netray_email::checks::mx::{NO_MX, NULL_MX};
use netray_email::module::evidence_blocks;
use netray_email::quality::{
    BimiMeasured, Category, CheckResult, DkimKey, DmarcMeasured, DnsblAnswer, FcrdnsAnswer,
    Measured, MtaStsMeasured, MxHost, SpfMeasured, SseEvent, SubCheck, Verdict, compute_grade,
};
use serde_json::{Map, Value};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts")
        .join(name)
}

fn sub(name: &str, verdict: Verdict, detail: &str) -> SubCheck {
    SubCheck {
        name: name.to_string(),
        verdict,
        detail: detail.to_string(),
    }
}

fn canonical(v: Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<String> = m.keys().cloned().collect();
            keys.sort();
            let mut out = Map::new();
            for k in keys {
                let child = canonical(m[&k].clone());
                out.insert(k, child);
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.into_iter().map(canonical).collect()),
        other => other,
    }
}

/// Re-encode each `data:` line with sorted keys; all other framing is left untouched.
fn canonicalise_wire(wire: &str) -> String {
    wire.split_inclusive('\n')
        .map(|line| match line.strip_prefix("data: ") {
            Some(rest) => {
                let v: Value = serde_json::from_str(rest.trim_end()).expect("data is JSON");
                format!("data: {}\n", serde_json::to_string(&canonical(v)).unwrap())
            }
            None => line.to_string(),
        })
        .collect()
}

fn cat(category: Category, detail: &str, subs: Vec<SubCheck>) -> CheckResult {
    CheckResult::new(category, subs, detail.to_string())
}

/// Sub-checks, verdicts and details below are the ones the check code in
/// `crates/email/src/checks/*.rs` emits; the category `detail` is the status line the same
/// code attaches. Beacon puts the reason in `sub_checks[].detail`, not in the category detail.
///
/// A healthy MX; DKIM fails (weak key), DMARC and MTA-STS warn.
fn scenario_healthy() -> Vec<CheckResult> {
    vec![
        cat(
            Category::Mx,
            "2 MX record(s), 4 IP(s)",
            vec![sub(
                "mx_ok",
                Verdict::Pass,
                "2 MX record(s), 4 IP(s) resolved",
            )],
        ),
        cat(
            Category::Spf,
            "SPF record: v=spf1 ip4:192.0.2.0/24 ip6:2001:db8::/32 -all",
            vec![sub(
                "spf_ok",
                Verdict::Pass,
                "valid SPF with 2 authorized prefix(es)",
            )],
        ),
        cat(
            Category::Dkim,
            "DKIM key(s) found",
            vec![
                sub(
                    "weak_rsa_key",
                    Verdict::Fail,
                    "selector 'default': RSA 512 bits < 1024",
                ),
                sub(
                    "rsa_key_ok",
                    Verdict::Pass,
                    "selector 'selector1': RSA 2048 bits",
                ),
            ],
        ),
        cat(
            Category::Dmarc,
            "DMARC: v=DMARC1; p=none; rua=mailto:dmarc@example.com",
            vec![
                sub("policy_none", Verdict::Warn, "no enforcement"),
                sub("no_ruf", Verdict::Info, "no forensic reports configured"),
            ],
        ),
        cat(
            Category::MtaSts,
            "MTA-STS id=20260101",
            vec![
                sub("mode", Verdict::Warn, "mode: testing"),
                sub(
                    "max_age_low",
                    Verdict::Warn,
                    "max_age 3600 < 86400; suggest >= 604800",
                ),
            ],
        ),
        cat(
            Category::TlsRpt,
            "TLS-RPT: v=TLSRPTv1; rua=mailto:tls@example.com",
            vec![sub("valid", Verdict::Pass, "valid TLS-RPT record")],
        ),
        cat(
            Category::Dane,
            "No DANE TLSA records",
            vec![sub(
                "absent",
                Verdict::Info,
                "no TLSA records on any MX host",
            )],
        ),
        cat(
            Category::Dnssec,
            "DNSSEC signed",
            vec![sub(
                "ad_set",
                Verdict::Pass,
                "DNSSEC signed (DNSKEY records present)",
            )],
        ),
        cat(
            Category::Bimi,
            "No BIMI record",
            vec![sub("absent", Verdict::Info, "no BIMI record")],
        ),
        cat(
            Category::Fcrdns,
            "FCrDNS checked for 4 IP(s)",
            vec![
                sub(
                    "fcrdns_pass",
                    Verdict::Pass,
                    "FCrDNS confirmed for 192.0.2.10",
                ),
                sub(
                    "fcrdns_pass",
                    Verdict::Pass,
                    "FCrDNS confirmed for 2001:db8::10",
                ),
                sub(
                    "fcrdns_pass",
                    Verdict::Pass,
                    "FCrDNS confirmed for 192.0.2.11",
                ),
                sub(
                    "fcrdns_pass",
                    Verdict::Pass,
                    "FCrDNS confirmed for 2001:db8::11",
                ),
            ],
        ),
        cat(
            Category::Dnsbl,
            "checked 3 zone(s) for 4 IP(s)",
            vec![sub("clean", Verdict::Pass, "not listed in any DNSBL")],
        ),
        cat(
            Category::CrossValidation,
            "all cross-validation checks passed",
            vec![],
        ),
    ]
}

/// A domain without MX records: only `no_mx` means that.
fn scenario_no_mx() -> Vec<CheckResult> {
    vec![
        cat(
            Category::Mx,
            "No MX records found",
            vec![sub(NO_MX, Verdict::Fail, "no MX records found")],
        ),
        cat(
            Category::Spf,
            "No SPF record",
            vec![sub("no_spf", Verdict::Fail, "no SPF record found")],
        ),
        cat(
            Category::Dkim,
            "No DKIM keys found",
            vec![sub(
                "no_dkim",
                Verdict::Info,
                "no DKIM keys found for any selector",
            )],
        ),
        cat(
            Category::Dmarc,
            "No DMARC record",
            vec![sub("no_dmarc", Verdict::Fail, "no DMARC record found")],
        ),
        cat(
            Category::MtaSts,
            "No MTA-STS record",
            vec![sub("absent", Verdict::Info, "no MTA-STS DNS record")],
        ),
        cat(
            Category::TlsRpt,
            "No TLS-RPT record",
            vec![sub("absent", Verdict::Info, "no TLS-RPT record")],
        ),
        cat(
            Category::Dane,
            "No MX hosts",
            vec![sub(
                "absent",
                Verdict::Info,
                "no MX hosts to check for DANE",
            )],
        ),
        cat(
            Category::Dnssec,
            "DNSSEC not validated",
            vec![sub("ad_not_set", Verdict::Info, "DNSSEC not validated")],
        ),
        cat(
            Category::Bimi,
            "No BIMI record",
            vec![sub("absent", Verdict::Info, "no BIMI record")],
        ),
        cat(
            Category::Fcrdns,
            "No MX IPs",
            vec![sub("no_ips", Verdict::Info, "no MX IPs to check")],
        ),
        cat(
            Category::Dnsbl,
            "No MX IPs",
            vec![sub("no_ips", Verdict::Info, "no MX IPs to check")],
        ),
        cat(
            Category::CrossValidation,
            "all cross-validation checks passed",
            vec![],
        ),
    ]
}

/// MX exists but its host is a CNAME (RFC 5321 5.1): `mx` fails via `mx_cname`, not `no_mx`.
/// The MX IPs are still resolved, so FCrDNS and DNSBL run (and one IP is listed).
fn scenario_mx_cname() -> Vec<CheckResult> {
    vec![
        cat(
            Category::Mx,
            "1 MX record(s), 2 IP(s)",
            vec![
                sub(
                    "mx_cname",
                    Verdict::Fail,
                    "MX `mail.example.com` points to a CNAME (RFC 5321 \u{a7}5.1)",
                ),
                sub(
                    "single_mx",
                    Verdict::Info,
                    "only one MX record, but it resolves to 2 IP addresses",
                ),
                sub("no_ipv6", Verdict::Info, "no MX host has an AAAA record"),
            ],
        ),
        cat(
            Category::Spf,
            "SPF record: v=spf1 ip4:192.0.2.0/24 ip4:198.51.100.0/24 -all",
            vec![sub(
                "spf_ok",
                Verdict::Pass,
                "valid SPF with 2 authorized prefix(es)",
            )],
        ),
        cat(
            Category::Dkim,
            "DKIM key(s) found",
            vec![sub(
                "rsa_key_ok",
                Verdict::Pass,
                "selector 'default': RSA 2048 bits",
            )],
        ),
        cat(
            Category::Dmarc,
            "DMARC: v=DMARC1; p=reject; rua=mailto:dmarc@example.com",
            vec![
                sub("policy_reject", Verdict::Pass, "p=reject"),
                sub("no_ruf", Verdict::Info, "no forensic reports configured"),
            ],
        ),
        cat(
            Category::MtaSts,
            "No MTA-STS record",
            vec![sub("absent", Verdict::Info, "no MTA-STS DNS record")],
        ),
        cat(
            Category::TlsRpt,
            "No TLS-RPT record",
            vec![sub("absent", Verdict::Info, "no TLS-RPT record")],
        ),
        cat(
            Category::Dane,
            "No DANE TLSA records",
            vec![sub(
                "absent",
                Verdict::Info,
                "no TLSA records on any MX host",
            )],
        ),
        cat(
            Category::Dnssec,
            "DNSSEC not validated",
            vec![sub("ad_not_set", Verdict::Info, "DNSSEC not validated")],
        ),
        cat(
            Category::Bimi,
            "No BIMI record",
            vec![sub("absent", Verdict::Info, "no BIMI record")],
        ),
        cat(
            Category::Fcrdns,
            "FCrDNS checked for 2 IP(s)",
            vec![
                sub(
                    "fcrdns_pass",
                    Verdict::Pass,
                    "FCrDNS confirmed for 192.0.2.10",
                ),
                sub(
                    "fcrdns_pass",
                    Verdict::Pass,
                    "FCrDNS confirmed for 198.51.100.10",
                ),
            ],
        ),
        cat(
            Category::Dnsbl,
            "checked 3 zone(s) for 2 IP(s)",
            vec![sub(
                "listed_zen_spamhaus_org",
                Verdict::Fail,
                "192.0.2.10 listed in zen.spamhaus.org (127.0.0.2)",
            )],
        ),
        cat(
            Category::CrossValidation,
            "all cross-validation checks passed",
            vec![],
        ),
    ]
}

/// A Null MX domain (RFC 7505) that publishes `-all` and `p=reject`: it sends and receives no
/// mail. MX-dependent categories see no hosts and no IPs. DKIM is absent, so `reject_no_dkim`
/// still fires next to `sends_no_mail`.
fn scenario_null_mx() -> Vec<CheckResult> {
    vec![
        cat(
            Category::Mx,
            "Null MX: domain does not accept mail",
            vec![sub(
                NULL_MX,
                Verdict::Info,
                "Null MX (RFC 7505): domain does not accept mail",
            )],
        ),
        cat(
            Category::Spf,
            "SPF record: v=spf1 -all",
            vec![sub(
                "spf_ok",
                Verdict::Pass,
                "valid SPF with 0 authorized prefix(es)",
            )],
        ),
        cat(
            Category::Dkim,
            "No DKIM keys found",
            vec![sub(
                "no_dkim",
                Verdict::Info,
                "no DKIM keys found for any selector",
            )],
        ),
        cat(
            Category::Dmarc,
            "DMARC: v=DMARC1; p=reject; rua=mailto:d@example.com",
            vec![
                sub("policy_reject", Verdict::Pass, "p=reject"),
                sub("no_ruf", Verdict::Info, "no forensic reports configured"),
            ],
        ),
        cat(
            Category::MtaSts,
            "No MTA-STS record",
            vec![sub("absent", Verdict::Info, "no MTA-STS DNS record")],
        ),
        cat(
            Category::TlsRpt,
            "No TLS-RPT record",
            vec![sub("absent", Verdict::Info, "no TLS-RPT record")],
        ),
        cat(
            Category::Dane,
            "No MX hosts",
            vec![sub(
                "absent",
                Verdict::Info,
                "no MX hosts to check for DANE",
            )],
        ),
        cat(
            Category::Dnssec,
            "DNSSEC not validated",
            vec![sub("ad_not_set", Verdict::Info, "DNSSEC not validated")],
        ),
        cat(
            Category::Bimi,
            "No BIMI record",
            vec![sub("absent", Verdict::Info, "no BIMI record")],
        ),
        cat(
            Category::Fcrdns,
            "No MX IPs",
            vec![sub("no_ips", Verdict::Info, "no MX IPs to check")],
        ),
        cat(
            Category::Dnsbl,
            "No MX IPs",
            vec![sub("no_ips", Verdict::Info, "no MX IPs to check")],
        ),
        cat(
            Category::CrossValidation,
            "1 cross-validation issue(s)",
            vec![
                sub(
                    SENDS_NO_MAIL,
                    Verdict::Info,
                    "domain declares it sends no mail (Null MX or SPF -all only)",
                ),
                sub(
                    "reject_no_dkim",
                    Verdict::Warn,
                    "DMARC p=reject and SPF -all but no DKIM keys found",
                ),
            ],
        ),
    ]
}

/// A sending domain with `p=reject` and SPF `-all` but no DKIM key: every other category
/// passes, so `reject_no_dkim` is the only finding (SPF covers the MX IPs).
fn scenario_sending_no_dkim() -> Vec<CheckResult> {
    vec![
        cat(
            Category::Mx,
            "2 MX record(s), 4 IP(s)",
            vec![sub(
                "mx_ok",
                Verdict::Pass,
                "2 MX record(s), 4 IP(s) resolved",
            )],
        ),
        cat(
            Category::Spf,
            "SPF record: v=spf1 ip4:192.0.2.0/24 ip6:2001:db8::/32 -all",
            vec![sub(
                "spf_ok",
                Verdict::Pass,
                "valid SPF with 2 authorized prefix(es)",
            )],
        ),
        cat(
            Category::Dkim,
            "No DKIM keys found",
            vec![sub(
                "no_dkim",
                Verdict::Info,
                "no DKIM keys found for any selector",
            )],
        ),
        cat(
            Category::Dmarc,
            "DMARC: v=DMARC1; p=reject; rua=mailto:dmarc@example.com",
            vec![
                sub("policy_reject", Verdict::Pass, "p=reject"),
                sub("no_ruf", Verdict::Info, "no forensic reports configured"),
            ],
        ),
        cat(
            Category::MtaSts,
            "MTA-STS id=20260101",
            vec![sub("mode", Verdict::Pass, "mode: enforce")],
        ),
        cat(
            Category::TlsRpt,
            "TLS-RPT: v=TLSRPTv1; rua=mailto:tls@example.com",
            vec![sub("valid", Verdict::Pass, "valid TLS-RPT record")],
        ),
        cat(
            Category::Dane,
            "No DANE TLSA records",
            vec![sub(
                "absent",
                Verdict::Info,
                "no TLSA records on any MX host",
            )],
        ),
        cat(
            Category::Dnssec,
            "DNSSEC signed",
            vec![sub(
                "ad_set",
                Verdict::Pass,
                "DNSSEC signed (DNSKEY records present)",
            )],
        ),
        cat(
            Category::Bimi,
            "No BIMI record",
            vec![sub("absent", Verdict::Info, "no BIMI record")],
        ),
        cat(
            Category::Fcrdns,
            "FCrDNS checked for 4 IP(s)",
            vec![
                sub(
                    "fcrdns_pass",
                    Verdict::Pass,
                    "FCrDNS confirmed for 192.0.2.10",
                ),
                sub(
                    "fcrdns_pass",
                    Verdict::Pass,
                    "FCrDNS confirmed for 2001:db8::10",
                ),
                sub(
                    "fcrdns_pass",
                    Verdict::Pass,
                    "FCrDNS confirmed for 192.0.2.11",
                ),
                sub(
                    "fcrdns_pass",
                    Verdict::Pass,
                    "FCrDNS confirmed for 2001:db8::11",
                ),
            ],
        ),
        cat(
            Category::Dnsbl,
            "checked 3 zone(s) for 4 IP(s)",
            vec![sub("clean", Verdict::Pass, "not listed in any DNSBL")],
        ),
        cat(
            Category::CrossValidation,
            "1 cross-validation issue(s)",
            vec![sub(
                "reject_no_dkim",
                Verdict::Warn,
                "DMARC p=reject and SPF -all but no DKIM keys found",
            )],
        ),
    ]
}

/// The healthy scenario with DNSBL replaced by the shape of a category that did not complete:
/// one `skipped` sub-check with verdict Skip, category detail "skipped".
fn scenario_partial() -> Vec<CheckResult> {
    scenario_healthy()
        .into_iter()
        .map(|r| {
            if r.category == Category::Dnsbl {
                cat(
                    Category::Dnsbl,
                    "skipped",
                    vec![sub(
                        SKIPPED,
                        Verdict::Skip,
                        "check did not complete in time",
                    )],
                )
            } else {
                r
            }
        })
        .collect()
}

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

fn records(records: &[&str]) -> Vec<String> {
    records.iter().map(|r| r.to_string()).collect()
}

/// One evaluated SPF record whose mechanisms need no DNS lookup.
fn spf(record: &str, prefixes: &[&str]) -> Option<SpfMeasured> {
    Some(SpfMeasured {
        records: vec![record.to_string()],
        authorized_prefixes: prefixes.iter().map(|p| p.parse().unwrap()).collect(),
        lookup_count: Some(0),
        void_lookups: Some(0),
    })
}

fn dmarc(record: &str) -> Option<DmarcMeasured> {
    Some(DmarcMeasured {
        records: records(&[record]),
        ..DmarcMeasured::default()
    })
}

/// A policy fetched with 200 and `text/plain`.
fn mta_sts(record: &str, policy: &str) -> Option<MtaStsMeasured> {
    Some(MtaStsMeasured {
        records: records(&[record]),
        policy: Some(policy.to_string()),
        policy_status: Some(200),
        content_type: Some("text/plain".to_string()),
        fetch_error: None,
    })
}

/// RSA keys whose size was read.
fn dkim(keys: &[(&str, usize)]) -> Option<Vec<DkimKey>> {
    Some(
        keys.iter()
            .map(|(selector, bits)| DkimKey {
                selector: selector.to_string(),
                state: "ok".to_string(),
                key_type: "rsa".to_string(),
                bits: Some(*bits),
            })
            .collect(),
    )
}

/// One answered query per zone and address; `listed` holds the (address, zone, code) that are
/// listed.
fn dnsbl(zones: &[&str], addresses: &[&str], listed: &[(&str, &str, &str)]) -> Vec<DnsblAnswer> {
    zones
        .iter()
        .flat_map(|zone| {
            addresses.iter().map(move |a| DnsblAnswer {
                zone: zone.to_string(),
                target: a.to_string(),
                answered: true,
                answer: listed
                    .iter()
                    .filter(|(la, lz, _)| la == a && lz == zone)
                    .map(|(_, _, code)| code.parse().unwrap())
                    .collect(),
            })
        })
        .collect()
}

fn fcrdns(answers: &[(&str, &str)]) -> Option<Vec<FcrdnsAnswer>> {
    Some(
        answers
            .iter()
            .map(|(a, ptr)| FcrdnsAnswer {
                address: ip(a),
                ptr: vec![ptr.to_string()],
                confirmed: true,
            })
            .collect(),
    )
}

/// The domain answered every name of the scenario without a record.
fn none_published() -> Measured {
    Measured {
        mx: Some(Vec::new()),
        spf: Some(SpfMeasured::default()),
        dmarc: Some(DmarcMeasured::default()),
        tls_rpt: Some(Vec::new()),
        bimi: Some(BimiMeasured::default()),
        mta_sts: Some(MtaStsMeasured::default()),
        dnssec: Some(false),
        dkim: Some(Vec::new()),
        ..Measured::default()
    }
}

const MX_ADDRESSES: [&str; 4] = ["192.0.2.10", "2001:db8::10", "192.0.2.11", "2001:db8::11"];
const ZONES: [&str; 3] = ["bl1.example.com", "bl2.example.com", "bl3.example.com"];

fn two_mx_hosts() -> Option<Vec<MxHost>> {
    Some(vec![
        MxHost {
            preference: 10,
            host: "mx1.example.com".to_string(),
            cname: Some(String::new()),
            addresses: vec![ip(MX_ADDRESSES[0]), ip(MX_ADDRESSES[1])],
        },
        MxHost {
            preference: 20,
            host: "mx2.example.com".to_string(),
            cname: Some(String::new()),
            addresses: vec![ip(MX_ADDRESSES[2]), ip(MX_ADDRESSES[3])],
        },
    ])
}

fn two_mx_fcrdns() -> Option<Vec<FcrdnsAnswer>> {
    fcrdns(&[
        (MX_ADDRESSES[0], "mx1.example.com"),
        (MX_ADDRESSES[1], "mx1.example.com"),
        (MX_ADDRESSES[2], "mx2.example.com"),
        (MX_ADDRESSES[3], "mx2.example.com"),
    ])
}

/// What the checks of `scenario_healthy` measured.
fn measured_healthy() -> Measured {
    Measured {
        mx: two_mx_hosts(),
        spf: spf(
            "v=spf1 ip4:192.0.2.0/24 ip6:2001:db8::/32 -all",
            &["192.0.2.0/24", "2001:db8::/32"],
        ),
        dmarc: dmarc("v=DMARC1; p=none; rua=mailto:dmarc@example.com"),
        tls_rpt: Some(records(&["v=TLSRPTv1; rua=mailto:tls@example.com"])),
        bimi: Some(BimiMeasured::default()),
        mta_sts: mta_sts(
            "v=STSv1; id=20260101",
            "version: STSv1\nmode: testing\nmx: *.example.com\nmax_age: 3600\n",
        ),
        dnssec: Some(true),
        dkim: dkim(&[("default", 512), ("selector1", 2048)]),
        dane: Some(Vec::new()),
        dnsbl: Some(dnsbl(&ZONES, &MX_ADDRESSES, &[])),
        fcrdns: two_mx_fcrdns(),
    }
}

/// What the checks of `scenario_no_mx` measured.
fn measured_no_mx() -> Measured {
    none_published()
}

/// What the checks of `scenario_mx_cname` measured.
fn measured_mx_cname() -> Measured {
    let addresses = ["192.0.2.10", "198.51.100.10"];
    let zones = ["zen.spamhaus.org", "bl2.example.com", "bl3.example.com"];
    Measured {
        mx: Some(vec![MxHost {
            preference: 10,
            host: "mail.example.com".to_string(),
            cname: Some("relay.example.com".to_string()),
            addresses: addresses.iter().map(|a| ip(a)).collect(),
        }]),
        spf: spf(
            "v=spf1 ip4:192.0.2.0/24 ip4:198.51.100.0/24 -all",
            &["192.0.2.0/24", "198.51.100.0/24"],
        ),
        dmarc: dmarc("v=DMARC1; p=reject; rua=mailto:dmarc@example.com"),
        dkim: dkim(&[("default", 2048)]),
        dane: Some(Vec::new()),
        dnsbl: Some(dnsbl(
            &zones,
            &addresses,
            &[("192.0.2.10", "zen.spamhaus.org", "127.0.0.2")],
        )),
        fcrdns: fcrdns(&[
            ("192.0.2.10", "mail.example.com"),
            ("198.51.100.10", "mail.example.com"),
        ]),
        ..none_published()
    }
}

/// What the checks of `scenario_null_mx` measured.
fn measured_null_mx() -> Measured {
    Measured {
        mx: Some(vec![MxHost {
            preference: 0,
            host: ".".to_string(),
            cname: None,
            addresses: Vec::new(),
        }]),
        spf: spf("v=spf1 -all", &[]),
        dmarc: dmarc("v=DMARC1; p=reject; rua=mailto:d@example.com"),
        ..none_published()
    }
}

/// What the checks of `scenario_sending_no_dkim` measured.
fn measured_sending_no_dkim() -> Measured {
    Measured {
        mx: two_mx_hosts(),
        spf: spf(
            "v=spf1 ip4:192.0.2.0/24 ip6:2001:db8::/32 -all",
            &["192.0.2.0/24", "2001:db8::/32"],
        ),
        dmarc: dmarc("v=DMARC1; p=reject; rua=mailto:dmarc@example.com"),
        tls_rpt: Some(records(&["v=TLSRPTv1; rua=mailto:tls@example.com"])),
        bimi: Some(BimiMeasured::default()),
        mta_sts: mta_sts(
            "v=STSv1; id=20260101",
            "version: STSv1\nmode: enforce\nmx: *.example.com\nmax_age: 604800\n",
        ),
        dnssec: Some(true),
        dkim: Some(Vec::new()),
        dane: Some(Vec::new()),
        dnsbl: Some(dnsbl(&ZONES, &MX_ADDRESSES, &[])),
        fcrdns: two_mx_fcrdns(),
    }
}

/// `measured_healthy` without the DNSBL answers: that check did not complete.
fn measured_partial() -> Measured {
    Measured {
        dnsbl: None,
        ..measured_healthy()
    }
}

async fn produce(results: Vec<CheckResult>, measured: Measured) -> String {
    let verdict_list: Vec<Verdict> = results.iter().map(|r| r.verdict).collect();
    let verdicts: HashMap<String, Verdict> = results
        .iter()
        .map(|r| {
            let key = serde_json::to_value(&r.category)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            (key, r.verdict)
        })
        .collect();

    let grade = compute_grade(&verdict_list);
    let mut events: Vec<SseEvent> = results.into_iter().map(SseEvent::Category).collect();
    events.push(SseEvent::Evidence {
        blocks: evidence_blocks(&measured, grade),
    });
    events.push(SseEvent::Summary {
        grade,
        verdicts,
        duration_ms: 1234,
    });

    let stream = futures::stream::iter(events.into_iter().map(|e| {
        let ev: Event = e.into();
        Ok::<_, Infallible>(ev)
    }));
    let body = Sse::new(stream).into_response().into_body();
    let bytes = body.collect().await.expect("body").to_bytes();
    canonicalise_wire(std::str::from_utf8(&bytes).expect("utf-8"))
}

#[tokio::test]
async fn sse_wire_format_matches_golden() {
    let scenarios: [(&str, Vec<CheckResult>, Measured); 6] = [
        ("beacon.sse", scenario_healthy(), measured_healthy()),
        ("beacon-no-mx.sse", scenario_no_mx(), measured_no_mx()),
        (
            "beacon-mx-cname.sse",
            scenario_mx_cname(),
            measured_mx_cname(),
        ),
        ("beacon-null-mx.sse", scenario_null_mx(), measured_null_mx()),
        (
            "beacon-sending-no-dkim.sse",
            scenario_sending_no_dkim(),
            measured_sending_no_dkim(),
        ),
        ("beacon-partial.sse", scenario_partial(), measured_partial()),
    ];
    let update = std::env::var("UPDATE_GOLDEN").as_deref() == Ok("1");

    for (file, results, measured) in scenarios {
        let produced = produce(results, measured).await;
        let path = fixture_path(file);

        if update {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &produced).unwrap();
            continue;
        }

        let golden = std::fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!(
                "golden {} is missing; run `UPDATE_GOLDEN=1 cargo test -p netray-email --test it contract_golden`",
                path.display()
            )
        });
        assert_eq!(
            golden,
            produced,
            "beacon's SSE output differs from {}; if the change is intended, regenerate with \
             `UPDATE_GOLDEN=1 cargo test -p netray-email --test it contract_golden` and check lens still parses it",
            path.display()
        );
    }
}
