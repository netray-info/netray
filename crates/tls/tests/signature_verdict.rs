// End-to-end signature verdict: chain of CertInfo -> weakest_signature -> ValidationResult ->
// check_strong_signature. Plus the OID names format_sig_algo must produce.

use netray_tls::quality::checks::check_strong_signature;
use netray_tls::tls::chain::{CertInfo, format_sig_algo};
use netray_tls::validate::chain_trust::weakest_signature;
use netray_tls::validate::{CheckStatus, ValidationResult};

fn cert(position: &str, self_signed: bool, sig: &str) -> CertInfo {
    CertInfo {
        position: position.to_string(),
        subject: "CN=example.com".to_string(),
        issuer: "CN=Example Test CA".to_string(),
        sans: vec!["example.com".to_string()],
        serial: "01".to_string(),
        not_before: "2030-01-01T00:00:00Z".to_string(),
        not_after: "2030-04-01T00:00:00Z".to_string(),
        days_remaining: 60,
        key_type: "RSA".to_string(),
        key_size: 2048,
        signature_algorithm: sig.to_string(),
        fingerprint_sha256: "AA:BB".to_string(),
        fingerprint_sha1: "CC:DD".to_string(),
        lifetime_days: 90,
        is_expired: false,
        is_self_signed: self_signed,
        cert_policy: "DV".to_string(),
        ocsp_url: None,
        ca_issuers_url: None,
    }
}

fn validation(weakest: String) -> ValidationResult {
    ValidationResult {
        chain_trusted: true,
        chain_trust_reason: None,
        terminates_at_self_signed: false,
        chain_order_correct: true,
        leaf_covers_hostname: true,
        any_expired: false,
        any_not_yet_valid: false,
        weakest_signature: weakest,
        earliest_expiry: "2030-04-01T00:00:00Z".to_string(),
        earliest_expiry_days: 60,
    }
}

#[test]
fn format_sig_algo_names_legacy_and_ed448_oids() {
    let rows = [
        ("1.2.840.113549.1.1.4", "md5WithRSAEncryption"),
        ("1.2.840.113549.1.1.2", "md2WithRSAEncryption"),
        ("1.2.840.10045.4.1", "ecdsa-with-SHA1"),
        ("1.2.840.10040.4.3", "dsa-with-SHA1"),
        ("1.3.14.3.2.29", "sha1WithRSASignature"),
        ("1.3.101.113", "Ed448"),
    ];
    for (oid, name) in rows {
        assert_eq!(format_sig_algo(oid), name, "oid {oid}");
    }
}

#[test]
fn signature_verdict_end_to_end() {
    const UNKNOWN: &str = "Unknown(1.2.840.113549.1.1.10)";
    // (label, chain, expected status, expected detail when pinned)
    let rows: Vec<(&str, Vec<CertInfo>, CheckStatus, Option<&str>)> = vec![
        (
            "C12 sha1 self-signed root is ignored",
            vec![
                cert("leaf", false, "sha256WithRSAEncryption"),
                cert("intermediate", false, "sha256WithRSAEncryption"),
                cert("root", true, "sha1WithRSAEncryption"),
            ],
            CheckStatus::Pass,
            None,
        ),
        (
            "C13 intermediate ecdsa-with-SHA1",
            vec![
                cert("leaf", false, "sha256WithRSAEncryption"),
                cert("intermediate", false, "ecdsa-with-SHA1"),
            ],
            CheckStatus::Fail,
            None,
        ),
        (
            "C14 intermediate dsa-with-SHA1",
            vec![
                cert("leaf", false, "sha256WithRSAEncryption"),
                cert("intermediate", false, "dsa-with-SHA1"),
            ],
            CheckStatus::Fail,
            None,
        ),
        (
            "C20 intermediate md2WithRSAEncryption",
            vec![
                cert("leaf", false, "sha256WithRSAEncryption"),
                cert("intermediate", false, "md2WithRSAEncryption"),
            ],
            CheckStatus::Fail,
            None,
        ),
        (
            "C15 single self-signed leaf sha1",
            vec![cert("leaf_self_signed", true, "sha1WithRSAEncryption")],
            CheckStatus::Fail,
            None,
        ),
        (
            "C16 unknown leaf algorithm",
            vec![cert("leaf", false, UNKNOWN)],
            CheckStatus::Warn,
            Some("unrecognised signature algorithm 1.2.840.113549.1.1.10"),
        ),
        (
            "C17 unknown leaf plus sha1 intermediate",
            vec![
                cert("leaf", false, UNKNOWN),
                cert("intermediate", false, "sha1WithRSAEncryption"),
            ],
            CheckStatus::Fail,
            None,
        ),
        (
            "C18 unreadable leaf signature",
            vec![cert("leaf", false, "")],
            CheckStatus::Warn,
            Some("unreadable certificate signature"),
        ),
    ];

    for (label, chain, status, detail) in rows {
        let check = check_strong_signature(&validation(weakest_signature(&chain)));
        assert_eq!(check.status, status, "{label}: {}", check.detail);
        if let Some(detail) = detail {
            assert_eq!(check.detail, detail, "{label}");
        }
    }
}
