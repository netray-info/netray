//! CAA critical-flag handling: only a critical record with an unknown tag fails.

use netray_tls::dns::CaaLookup;
use netray_tls::dns::caa::CaaRecord;
use netray_tls::validate::CheckStatus;
use netray_tls::validate::caa_compliance::check_caa_compliance;

const LE_ISSUER: &str = "CN=R3, O=Let's Encrypt, C=US";

fn rec(tag: &str, value: &str, critical: bool) -> CaaRecord {
    CaaRecord {
        tag: tag.into(),
        value: value.into(),
        issuer_critical: critical,
    }
}

#[test]
fn caa_critical_flag_handling() {
    let cases: Vec<(&str, Vec<CaaRecord>, CheckStatus)> = vec![
        (
            "C7 critical unknown tag fails",
            vec![
                rec("issue", "letsencrypt.org", false),
                rec("foo", "bar", true),
            ],
            CheckStatus::Fail,
        ),
        (
            "C8 critical issue authorising the CA passes",
            vec![rec("issue", "letsencrypt.org", true)],
            CheckStatus::Pass,
        ),
        (
            "C9 critical issue not authorising the CA fails",
            vec![rec("issue", "digicert.com", true)],
            CheckStatus::Fail,
        ),
        (
            "C10 critical IODEF (case-insensitive) passes",
            vec![
                rec("issue", "letsencrypt.org", false),
                rec("IODEF", "mailto:admin@example.com", true),
            ],
            CheckStatus::Pass,
        ),
        (
            "C10 critical contactemail passes",
            vec![
                rec("issue", "letsencrypt.org", false),
                rec("contactemail", "admin@example.com", true),
            ],
            CheckStatus::Pass,
        ),
    ];
    for (name, records, want) in cases {
        let got = check_caa_compliance(
            &CaaLookup { records },
            LE_ISSUER,
            &["www.example.com".to_string()],
        );
        assert_eq!(got, want, "{name}");
    }
}
