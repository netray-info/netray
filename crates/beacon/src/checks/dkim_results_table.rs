//! Pinning table for today's `check_dkim` results.
//!
//! Each row builds one stub resolver and drives the real `check_dkim` with no
//! user selectors and no provider MX host, so the only probed selector is
//! `default`. Per row it asserts the category verdict, every sub-check name and
//! verdict, the category detail and the returned `dkim_found` flag, all
//! recorded literally from today's behaviour.
//!
//! `check_dkim` reads neither MX nor SPF; the Null MX / `v=spf1 -all` records
//! of the parked row and the MX / SPF records of the sending rows are present
//! only so each stub looks like the kind of domain the row names.
//!
//! Requirement 4 of email-scoring: a key with an empty `p=` is revoked, not
//! found. It is Info on a parked domain (`sends_no_mail`), Warn on a sending one.

use crate::checks::dkim::check_dkim;
use crate::dns::test_support::TestDnsResolver;
use crate::quality::{Category, Verdict};

/// A throwaway 2048-bit RSA SubjectPublicKeyInfo (base64 DER), used as the `p=` value.
const RSA_2048_P: &str = "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAv06e3j461rNqkC2J1zvYTmm2ttXIUL3plNVJDrUdqHnB14KnAbMOz2RCdBwdDEi1BZ6uESPzTkKSz/EdkDHRSaULx6XzgEGhAfXiXiVgWeh5e+fhNjVkFSquPx57PuYZAnaotyNi9ppQs/MyF+XTdHTl4GQfJG0y8Wyaj6gndzGubPaoZq3gI5j0hoqsVDpwzraw9BtWZVUMhhhIhDrITiJCY8PCmqFcFCOH22T+0zFFEor+mfkagBUOfF6cw/QVD/6MLxK/Ull2Tvi8xtW0crx5YHACMZDedLfVkCX6JokHCFHu2fSem/2T/HxQn1MD73uW2ZKTsgGJ3cWZsHYC/QIDAQAB";

struct Row {
    name: &'static str,
    /// Parked: Null MX and `v=spf1 -all`; otherwise a sending domain.
    parked: bool,
    /// TXT at `default._domainkey.example.com`, if any (`{key}` = valid RSA p=).
    key_record: Option<&'static str>,
    /// Extra TXT at `google._domainkey.example.com`; also makes the MX a Google host.
    google_record: Option<&'static str>,
    verdict: Verdict,
    sub_checks: &'static [(&'static str, Verdict)],
    detail: &'static str,
    found: bool,
}

const ROWS: &[Row] = &[
    Row {
        name: "C13 parked domain, only key has empty p=",
        parked: true,
        key_record: Some("v=DKIM1; k=rsa; p="),
        google_record: None,
        verdict: Verdict::Info,
        sub_checks: &[("key_revoked", Verdict::Info)],
        detail: "only revoked DKIM keys",
        found: false,
    },
    Row {
        name: "C14 sending domain, only key has empty p=",
        parked: false,
        key_record: Some("v=DKIM1; k=rsa; p="),
        google_record: None,
        verdict: Verdict::Warn,
        sub_checks: &[("key_revoked", Verdict::Info)],
        detail: "only revoked DKIM keys",
        found: false,
    },
    Row {
        name: "C15 sending domain, no key at any probed selector",
        parked: false,
        key_record: None,
        google_record: None,
        verdict: Verdict::Info,
        sub_checks: &[("no_dkim", Verdict::Info)],
        detail: "No DKIM keys found",
        found: false,
    },
    Row {
        name: "C9 sending domain, one valid key",
        parked: false,
        key_record: Some("v=DKIM1; k=rsa; p={key}"),
        google_record: None,
        verdict: Verdict::Pass,
        sub_checks: &[("rsa_key_ok", Verdict::Pass)],
        detail: "DKIM key(s) found",
        found: true,
    },
    Row {
        name: "C17 sending domain, valid provider key plus empty p= at default",
        parked: false,
        key_record: Some("v=DKIM1; k=rsa; p="),
        google_record: Some("v=DKIM1; k=rsa; p={key}"),
        verdict: Verdict::Pass,
        sub_checks: &[
            ("rsa_key_ok", Verdict::Pass),
            ("key_revoked", Verdict::Info),
        ],
        detail: "DKIM key(s) found",
        found: true,
    },
];

#[tokio::test]
async fn dkim_results_table() {
    let mut failures = Vec::new();

    for row in ROWS {
        let mut resolver = TestDnsResolver::new();
        resolver = if row.parked {
            resolver
                .with_mx("example.com", vec![(0, ".")])
                .with_txt("example.com", vec!["v=spf1 -all"])
        } else {
            resolver
                .with_mx("example.com", vec![(10, "mail.example.com")])
                .with_txt("example.com", vec!["v=spf1 include:_spf.example.com ~all"])
        };
        if let Some(record) = row.key_record {
            let record = record.replace("{key}", RSA_2048_P);
            resolver = resolver.with_txt("default._domainkey.example.com", vec![record.as_str()]);
        }

        let mut mx_hosts: Vec<String> = Vec::new();
        if let Some(record) = row.google_record {
            let record = record.replace("{key}", RSA_2048_P);
            resolver = resolver.with_txt("google._domainkey.example.com", vec![record.as_str()]);
            mx_hosts.push("aspmx.l.google.com".to_string());
        }

        let (result, found) =
            check_dkim("example.com", &mx_hosts, &[], 3, &resolver, row.parked).await;
        let got: Vec<(String, Verdict)> = result
            .sub_checks
            .iter()
            .map(|s| (s.name.clone(), s.verdict))
            .collect();
        let want: Vec<(String, Verdict)> = row
            .sub_checks
            .iter()
            .map(|(n, v)| (n.to_string(), *v))
            .collect();

        if result.category != Category::Dkim
            || result.verdict != row.verdict
            || got != want
            || result.detail != row.detail
            || found != row.found
        {
            failures.push(format!(
                "row {}: category={:?} verdict={:?} sub_checks={got:?} detail={:?} found={found}; want verdict={:?} {want:?} detail={:?} found={}",
                row.name,
                result.category,
                result.verdict,
                result.detail,
                row.verdict,
                row.detail,
                row.found
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
