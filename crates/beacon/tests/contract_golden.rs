//! Contract golden: the SSE bytes beacon puts on the wire, consumed by
//! `crates/lens/tests/contract_beacon.rs`.
//!
//! The events go through beacon's real encoding path (`From<SseEvent> for sse::Event`,
//! then axum's `Sse` response body), so framing is exactly what a client receives.
//! Only JSON key order is canonicalised (`verdicts` is a `HashMap`, whose order is random).
//!
//! `UPDATE_GOLDEN=1 cargo test -p beacon --test contract_golden` rewrites beacon.sse,
//! beacon-no-mx.sse and beacon-mx-cname.sse.

use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;

use axum::response::IntoResponse;
use axum::response::sse::{Event, Sse};
use beacon::quality::{Category, CheckResult, SseEvent, SubCheck, Verdict, compute_grade};
use http_body_util::BodyExt;
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
/// `crates/beacon/src/checks/*.rs` emits; the category `detail` is the status line the same
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
            "SPF record: v=spf1 ip4:192.0.2.0/24 -all",
            vec![sub(
                "spf_ok",
                Verdict::Pass,
                "valid SPF with 1 authorized prefix(es)",
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
            vec![sub("policy_none", Verdict::Warn, "no enforcement")],
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
            vec![sub(
                "fcrdns_pass",
                Verdict::Pass,
                "FCrDNS confirmed for 192.0.2.10",
            )],
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
            vec![sub("no_mx", Verdict::Fail, "no MX records found")],
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
            "SPF record: v=spf1 ip4:192.0.2.0/24 -all",
            vec![sub(
                "spf_ok",
                Verdict::Pass,
                "valid SPF with 1 authorized prefix(es)",
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
            vec![sub("policy_reject", Verdict::Pass, "p=reject")],
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
            vec![sub(
                "fcrdns_pass",
                Verdict::Pass,
                "FCrDNS confirmed for 192.0.2.10",
            )],
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

async fn produce(results: Vec<CheckResult>) -> String {
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

    let mut events: Vec<SseEvent> = results.into_iter().map(SseEvent::Category).collect();
    events.push(SseEvent::Summary {
        grade: compute_grade(&verdict_list),
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
    let scenarios: [(&str, Vec<CheckResult>); 3] = [
        ("beacon.sse", scenario_healthy()),
        ("beacon-no-mx.sse", scenario_no_mx()),
        ("beacon-mx-cname.sse", scenario_mx_cname()),
    ];
    let update = std::env::var("UPDATE_GOLDEN").as_deref() == Ok("1");

    for (file, results) in scenarios {
        let produced = produce(results).await;
        let path = fixture_path(file);

        if update {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &produced).unwrap();
            continue;
        }

        let golden = std::fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!(
                "golden {} is missing; run `UPDATE_GOLDEN=1 cargo test -p beacon --test contract_golden`",
                path.display()
            )
        });
        assert_eq!(
            golden,
            produced,
            "beacon's SSE output differs from {}; if the change is intended, regenerate with \
             `UPDATE_GOLDEN=1 cargo test -p beacon --test contract_golden` and check lens still parses it",
            path.display()
        );
    }
}
