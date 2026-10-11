//! C22-C25: for a wildcard leaf, `issuewild` records decide over `issue` (RFC 8659 §4.3);
//! the leaf's non-wildcard names stay judged by `issue`.

use netray_tls::dns::CaaLookup;
use netray_tls::dns::caa::CaaRecord;
use netray_tls::validate::CheckStatus;
use netray_tls::validate::caa_compliance::check_caa_compliance;

const LE_ISSUER: &str = "CN=R3, O=Let's Encrypt, C=US";

fn rec(tag: &str, value: &str) -> CaaRecord {
    CaaRecord {
        tag: tag.into(),
        value: value.into(),
        issuer_critical: false,
    }
}

fn sans(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| n.to_string()).collect()
}

#[test]
fn issuewild_governs_wildcard_leaves() {
    // (label, records, leaf SANs, expected)
    type Row = (&'static str, Vec<CaaRecord>, Vec<String>, CheckStatus);
    let rows: Vec<Row> = vec![
        (
            "C22 issuewild ; denies a wildcard leaf the issue set allows",
            vec![rec("issue", "letsencrypt.org"), rec("issuewild", ";")],
            sans(&["*.example.com"]),
            CheckStatus::Fail,
        ),
        (
            "C23 issuewild names the CA, issue names another",
            vec![
                rec("issue", "digicert.com"),
                rec("issuewild", "letsencrypt.org"),
            ],
            sans(&["*.example.com"]),
            CheckStatus::Pass,
        ),
        (
            "C23 wildcard plus apex: issue must authorise the apex too",
            vec![
                rec("issue", "digicert.com"),
                rec("issuewild", "letsencrypt.org"),
            ],
            sans(&["*.example.com", "example.com"]),
            CheckStatus::Fail,
        ),
        (
            "C24 no issuewild: issue decides for a wildcard leaf",
            vec![rec("issue", "letsencrypt.org")],
            sans(&["*.example.com"]),
            CheckStatus::Pass,
        ),
        (
            "C25 non-wildcard leaf ignores issuewild ;",
            vec![rec("issue", "letsencrypt.org"), rec("issuewild", ";")],
            sans(&["www.example.com"]),
            CheckStatus::Pass,
        ),
    ];
    for (name, records, leaf_sans, want) in rows {
        let got = check_caa_compliance(&CaaLookup { records }, LE_ISSUER, &leaf_sans);
        assert_eq!(got, want, "{name}");
    }
}
