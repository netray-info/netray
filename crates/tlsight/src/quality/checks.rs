use crate::routes::ConsistencyResult;
use crate::tls::CertInfo;
use crate::validate::{CheckStatus, ValidationResult};

use super::types::{Category, HealthCheck};

// ---------------------------------------------------------------------------
// Certificate checks
// ---------------------------------------------------------------------------

pub fn check_chain_trusted(validation: &ValidationResult) -> HealthCheck {
    let (status, detail) = if validation.chain_trusted {
        (
            CheckStatus::Pass,
            "chain verifies to a trusted root".to_string(),
        )
    } else {
        let reason = validation
            .chain_trust_reason
            .as_deref()
            .unwrap_or("unknown reason");
        (CheckStatus::Fail, format!("chain not trusted: {reason}"))
    };
    HealthCheck {
        id: "chain_trusted".to_string(),
        category: Category::Certificate,
        status,
        label: "Chain trusted".to_string(),
        detail,
    }
}

pub fn check_not_expired(validation: &ValidationResult) -> HealthCheck {
    let (status, detail) = if validation.any_expired {
        (
            CheckStatus::Fail,
            "one or more certificates have expired".to_string(),
        )
    } else if validation.any_not_yet_valid {
        (
            CheckStatus::Fail,
            "one or more certificates are not yet valid".to_string(),
        )
    } else {
        (
            CheckStatus::Pass,
            "all certificates are within validity period".to_string(),
        )
    };
    HealthCheck {
        id: "not_expired".to_string(),
        category: Category::Certificate,
        status,
        label: "Not expired".to_string(),
        detail,
    }
}

pub fn check_hostname_match(validation: &ValidationResult, is_hostname: bool) -> HealthCheck {
    if !is_hostname {
        return HealthCheck {
            id: "hostname_match".to_string(),
            category: Category::Certificate,
            status: CheckStatus::Skip,
            label: "Hostname match".to_string(),
            detail: "skipped for IP-mode inspection".to_string(),
        };
    }
    let (status, detail) = if validation.leaf_covers_hostname {
        (
            CheckStatus::Pass,
            "leaf certificate covers queried hostname".to_string(),
        )
    } else {
        (
            CheckStatus::Fail,
            "leaf certificate does not cover queried hostname".to_string(),
        )
    };
    HealthCheck {
        id: "hostname_match".to_string(),
        category: Category::Certificate,
        status,
        label: "Hostname match".to_string(),
        detail,
    }
}

pub fn check_chain_complete(validation: &ValidationResult) -> HealthCheck {
    let (status, detail) = if validation.chain_order_correct {
        (CheckStatus::Pass, "chain order is correct".to_string())
    } else if validation.chain_trusted {
        (
            CheckStatus::Warn,
            "chain order incorrect but trust verification succeeded".to_string(),
        )
    } else {
        (
            CheckStatus::Fail,
            "chain order incorrect and trust verification failed".to_string(),
        )
    };
    HealthCheck {
        id: "chain_complete".to_string(),
        category: Category::Certificate,
        status,
        label: "Chain complete".to_string(),
        detail,
    }
}

pub fn check_strong_signature(validation: &ValidationResult) -> HealthCheck {
    let algo = &validation.weakest_signature;
    let algo_lower = algo.to_lowercase();
    let (status, detail) = if algo_lower.contains("sha1") || algo_lower.contains("md5") {
        (
            CheckStatus::Fail,
            format!("weak signature algorithm: {algo}"),
        )
    } else {
        (CheckStatus::Pass, format!("signature algorithm: {algo}"))
    };
    HealthCheck {
        id: "strong_signature".to_string(),
        category: Category::Certificate,
        status,
        label: "Strong signature".to_string(),
        detail,
    }
}

pub fn check_key_strength(chain: &[CertInfo]) -> HealthCheck {
    for cert in chain {
        let key_type = &cert.key_type;
        let key_size = cert.key_size;
        if key_type.starts_with("RSA") && key_size > 0 && key_size < 2048 {
            return HealthCheck {
                id: "key_strength".to_string(),
                category: Category::Certificate,
                status: CheckStatus::Fail,
                label: "Key strength".to_string(),
                detail: format!(
                    "{} {} has {key_type} {key_size}-bit key (< 2048)",
                    cert.position, cert.subject
                ),
            };
        }
        if key_type.starts_with("ECDSA") && key_size > 0 && key_size < 256 {
            return HealthCheck {
                id: "key_strength".to_string(),
                category: Category::Certificate,
                status: CheckStatus::Fail,
                label: "Key strength".to_string(),
                detail: format!(
                    "{} {} has {key_type} {key_size}-bit key (< P-256)",
                    cert.position, cert.subject
                ),
            };
        }
    }
    let detail = chain
        .first()
        .map(|c| format!("{} {}-bit", c.key_type, c.key_size))
        .unwrap_or_else(|| "no certificates".to_string());
    HealthCheck {
        id: "key_strength".to_string(),
        category: Category::Certificate,
        status: CheckStatus::Pass,
        label: "Key strength".to_string(),
        detail,
    }
}

pub fn check_expiry_window(chain: &[CertInfo]) -> HealthCheck {
    let min = chain.iter().min_by_key(|c| c.days_remaining);
    let Some(cert) = min else {
        return HealthCheck {
            id: "expiry_window".to_string(),
            category: Category::Certificate,
            status: CheckStatus::Skip,
            label: "Expiry window".to_string(),
            detail: "no certificates".to_string(),
        };
    };
    let days = cert.days_remaining;
    let who = if cert.position == "leaf" || cert.position == "leaf_self_signed" {
        "leaf certificate".to_string()
    } else {
        format!("{} certificate ({})", cert.position, cert.subject)
    };
    let (status, detail) = if days <= 7 {
        (CheckStatus::Fail, format!("{who} expires in {days} days"))
    } else if days <= 30 {
        (CheckStatus::Warn, format!("{who} expires in {days} days"))
    } else {
        (CheckStatus::Pass, format!("{who} expires in {days} days"))
    };
    HealthCheck {
        id: "expiry_window".to_string(),
        category: Category::Certificate,
        status,
        label: "Expiry window".to_string(),
        detail,
    }
}

pub fn check_cert_lifetime(chain: &[CertInfo]) -> HealthCheck {
    let Some(leaf) = chain
        .iter()
        .find(|c| c.position == "leaf" || c.position == "leaf_self_signed")
    else {
        return HealthCheck {
            id: "cert_lifetime".to_string(),
            category: Category::Certificate,
            status: CheckStatus::Skip,
            label: "Certificate lifetime".to_string(),
            detail: "no leaf cert".to_string(),
        };
    };
    let days = leaf.lifetime_days;
    let (status, detail) = if days > 825 {
        (
            CheckStatus::Fail,
            format!("validity period {days} days exceeds 825-day limit"),
        )
    } else if days > 398 {
        (
            CheckStatus::Warn,
            format!("validity period {days} days exceeds 398-day recommendation"),
        )
    } else {
        (CheckStatus::Pass, format!("validity period {days} days"))
    };
    HealthCheck {
        id: "cert_lifetime".to_string(),
        category: Category::Certificate,
        status,
        label: "Certificate lifetime".to_string(),
        detail,
    }
}

// ---------------------------------------------------------------------------
// Protocol checks
// ---------------------------------------------------------------------------

pub fn check_tls_version(version: &str) -> HealthCheck {
    let (status, detail) = match version {
        "TLSv1.3" => (CheckStatus::Pass, "TLS 1.3".to_string()),
        "TLSv1.2" => (
            CheckStatus::Warn,
            "TLS 1.2 (upgrade to TLS 1.3 recommended)".to_string(),
        ),
        other => (CheckStatus::Fail, format!("outdated: {other}")),
    };
    HealthCheck {
        id: "tls_version".to_string(),
        category: Category::Protocol,
        status,
        label: "TLS version".to_string(),
        detail,
    }
}

/// Cipher suite classification for forward secrecy and AEAD checks.
pub struct CipherProperties {
    pub forward_secrecy: bool,
    pub aead: bool,
}

pub fn classify_cipher_suite(name: &str) -> CipherProperties {
    // TLS 1.3 suites are always ECDHE + AEAD
    if name.starts_with("TLS13_")
        || name.starts_with("TLS_AES_")
        || name.starts_with("TLS_CHACHA20_")
    {
        return CipherProperties {
            forward_secrecy: true,
            aead: true,
        };
    }
    let forward_secrecy = name.contains("_ECDHE_") || name.contains("_DHE_");
    let aead =
        name.contains("_GCM_") || name.contains("_CHACHA20_POLY1305") || name.contains("_CCM_");
    CipherProperties {
        forward_secrecy,
        aead,
    }
}

pub fn check_forward_secrecy(cipher_suite: &str) -> HealthCheck {
    let props = classify_cipher_suite(cipher_suite);
    let (status, detail) = if props.forward_secrecy {
        (
            CheckStatus::Pass,
            format!("{cipher_suite} provides forward secrecy"),
        )
    } else {
        (
            CheckStatus::Fail,
            format!("{cipher_suite} does not provide forward secrecy"),
        )
    };
    HealthCheck {
        id: "forward_secrecy".to_string(),
        category: Category::Protocol,
        status,
        label: "Forward secrecy".to_string(),
        detail,
    }
}

pub fn check_aead_cipher(cipher_suite: &str) -> HealthCheck {
    let props = classify_cipher_suite(cipher_suite);
    let (status, detail) = if props.aead {
        (
            CheckStatus::Pass,
            format!("{cipher_suite} uses AEAD encryption"),
        )
    } else {
        (
            CheckStatus::Fail,
            format!("{cipher_suite} does not use AEAD encryption"),
        )
    };
    HealthCheck {
        id: "aead_cipher".to_string(),
        category: Category::Protocol,
        status,
        label: "AEAD cipher".to_string(),
        detail,
    }
}

pub fn check_ct_logged(ct_enabled: bool, sct_count: Option<usize>) -> HealthCheck {
    if !ct_enabled {
        return HealthCheck {
            id: "ct_logged".to_string(),
            category: Category::Protocol,
            status: CheckStatus::Skip,
            label: "CT logged".to_string(),
            detail: "CT checking disabled".to_string(),
        };
    }
    let count = sct_count.unwrap_or(0);
    let (status, detail) = if count >= 2 {
        (CheckStatus::Pass, format!("{count} SCTs found"))
    } else {
        (
            CheckStatus::Warn,
            format!("{count} SCTs found (2+ recommended for CT policy compliance)"),
        )
    };
    HealthCheck {
        id: "ct_logged".to_string(),
        category: Category::Protocol,
        status,
        label: "CT logged".to_string(),
        detail,
    }
}

// ---------------------------------------------------------------------------
// Configuration checks (per-port)
// ---------------------------------------------------------------------------

pub fn check_ocsp_stapled(stapled: bool) -> HealthCheck {
    let (status, detail) = if stapled {
        (CheckStatus::Pass, "OCSP response stapled".to_string())
    } else {
        (
            CheckStatus::Warn,
            "no OCSP staple — clients must contact the CA for revocation checks".to_string(),
        )
    };
    HealthCheck {
        id: "ocsp_stapled".to_string(),
        category: Category::Protocol,
        status,
        label: "OCSP stapled".to_string(),
        detail,
    }
}

pub fn check_dane_valid(dane_status: CheckStatus) -> HealthCheck {
    let detail = match dane_status {
        CheckStatus::Pass => "TLSA records match presented certificate".to_string(),
        CheckStatus::Fail => "TLSA records do not match presented certificate".to_string(),
        CheckStatus::Warn => "DANE check inconclusive".to_string(),
        CheckStatus::Skip => "DANE check skipped (requires DNSSEC)".to_string(),
    };
    HealthCheck {
        id: "dane_valid".to_string(),
        category: Category::Configuration,
        status: dane_status,
        label: "DANE valid".to_string(),
        detail,
    }
}

pub fn check_caa_compliant(caa_status: CheckStatus) -> HealthCheck {
    let detail = match caa_status {
        CheckStatus::Pass => "issuing CA authorized by CAA records".to_string(),
        CheckStatus::Fail => "issuing CA not authorized by CAA records".to_string(),
        CheckStatus::Warn => "CAA check inconclusive".to_string(),
        CheckStatus::Skip => "CAA check skipped".to_string(),
    };
    HealthCheck {
        id: "caa_compliant".to_string(),
        category: Category::Configuration,
        status: caa_status,
        label: "CAA compliant".to_string(),
        detail,
    }
}

pub fn check_consistency(consistency: Option<&ConsistencyResult>) -> HealthCheck {
    let Some(c) = consistency else {
        return HealthCheck {
            id: "consistency".to_string(),
            category: Category::Configuration,
            status: CheckStatus::Skip,
            label: "IP consistency".to_string(),
            detail: "fewer than 2 IPs inspected".to_string(),
        };
    };
    if c.mismatches.is_empty() {
        HealthCheck {
            id: "consistency".to_string(),
            category: Category::Configuration,
            status: CheckStatus::Pass,
            label: "IP consistency".to_string(),
            detail: "all IPs serve matching configuration".to_string(),
        }
    } else {
        let fields: Vec<&str> = c.mismatches.iter().map(|m| m.field.as_str()).collect();
        HealthCheck {
            id: "consistency".to_string(),
            category: Category::Configuration,
            status: CheckStatus::Warn,
            label: "IP consistency".to_string(),
            detail: format!("mismatch in: {}", fields.join(", ")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn passing_validation() -> ValidationResult {
        ValidationResult {
            chain_trusted: true,
            chain_trust_reason: None,
            terminates_at_self_signed: false,
            chain_order_correct: true,
            leaf_covers_hostname: true,
            any_expired: false,
            any_not_yet_valid: false,
            weakest_signature: "sha256WithRSAEncryption".to_string(),
            earliest_expiry: "2026-12-31T23:59:59Z".to_string(),
            earliest_expiry_days: 300,
        }
    }

    fn leaf_cert(days: i64, key_type: &str, key_size: u32) -> CertInfo {
        CertInfo {
            position: "leaf".to_string(),
            subject: "CN=example.com".to_string(),
            issuer: "CN=issuer".to_string(),
            sans: vec!["example.com".to_string()],
            serial: "01".to_string(),
            not_before: "2024-01-01".to_string(),
            not_after: "2026-12-31".to_string(),
            days_remaining: days,
            key_type: key_type.to_string(),
            key_size,
            signature_algorithm: "sha256WithRSAEncryption".to_string(),
            fingerprint_sha256: "AA:BB".to_string(),
            fingerprint_sha1: "AA:BB".to_string(),
            lifetime_days: 365,
            is_expired: days < 0,
            is_self_signed: false,
        }
    }

    // --- Chain trusted ---

    #[test]
    fn chain_trusted_pass() {
        let v = passing_validation();
        assert_eq!(check_chain_trusted(&v).status, CheckStatus::Pass);
    }

    #[test]
    fn chain_trusted_fail() {
        let mut v = passing_validation();
        v.chain_trusted = false;
        v.chain_trust_reason = Some("self-signed".to_string());
        let c = check_chain_trusted(&v);
        assert_eq!(c.status, CheckStatus::Fail);
        assert!(c.detail.contains("self-signed"));
    }

    // --- Not expired ---

    #[test]
    fn not_expired_pass() {
        let v = passing_validation();
        assert_eq!(check_not_expired(&v).status, CheckStatus::Pass);
    }

    #[test]
    fn not_expired_fail_expired() {
        let mut v = passing_validation();
        v.any_expired = true;
        assert_eq!(check_not_expired(&v).status, CheckStatus::Fail);
    }

    #[test]
    fn not_expired_fail_not_yet_valid() {
        let mut v = passing_validation();
        v.any_not_yet_valid = true;
        assert_eq!(check_not_expired(&v).status, CheckStatus::Fail);
    }

    // --- Hostname match ---

    #[test]
    fn hostname_match_pass() {
        let v = passing_validation();
        assert_eq!(check_hostname_match(&v, true).status, CheckStatus::Pass);
    }

    #[test]
    fn hostname_match_fail() {
        let mut v = passing_validation();
        v.leaf_covers_hostname = false;
        assert_eq!(check_hostname_match(&v, true).status, CheckStatus::Fail);
    }

    #[test]
    fn hostname_match_skip_ip_mode() {
        let v = passing_validation();
        assert_eq!(check_hostname_match(&v, false).status, CheckStatus::Skip);
    }

    // --- Chain complete ---

    #[test]
    fn chain_complete_pass() {
        let v = passing_validation();
        assert_eq!(check_chain_complete(&v).status, CheckStatus::Pass);
    }

    #[test]
    fn chain_complete_warn_order_wrong_but_trusted() {
        let mut v = passing_validation();
        v.chain_order_correct = false;
        v.chain_trusted = true;
        assert_eq!(check_chain_complete(&v).status, CheckStatus::Warn);
    }

    #[test]
    fn chain_complete_fail_order_wrong_not_trusted() {
        let mut v = passing_validation();
        v.chain_order_correct = false;
        v.chain_trusted = false;
        assert_eq!(check_chain_complete(&v).status, CheckStatus::Fail);
    }

    // --- Strong signature ---

    #[test]
    fn strong_signature_pass() {
        let v = passing_validation();
        assert_eq!(check_strong_signature(&v).status, CheckStatus::Pass);
    }

    #[test]
    fn strong_signature_fail_sha1() {
        let mut v = passing_validation();
        v.weakest_signature = "sha1WithRSAEncryption".to_string();
        assert_eq!(check_strong_signature(&v).status, CheckStatus::Fail);
    }

    #[test]
    fn strong_signature_fail_md5() {
        let mut v = passing_validation();
        v.weakest_signature = "md5WithRSAEncryption".to_string();
        assert_eq!(check_strong_signature(&v).status, CheckStatus::Fail);
    }

    // --- Key strength ---

    #[test]
    fn key_strength_rsa_2048_pass() {
        let chain = vec![leaf_cert(300, "RSA", 2048)];
        assert_eq!(check_key_strength(&chain).status, CheckStatus::Pass);
    }

    #[test]
    fn key_strength_rsa_1024_fail() {
        let chain = vec![leaf_cert(300, "RSA", 1024)];
        assert_eq!(check_key_strength(&chain).status, CheckStatus::Fail);
    }

    #[test]
    fn key_strength_ecdsa_pass() {
        let chain = vec![leaf_cert(300, "ECDSA P-256", 256)];
        assert_eq!(check_key_strength(&chain).status, CheckStatus::Pass);
    }

    #[test]
    fn key_strength_ecdsa_p192_fail() {
        let chain = vec![leaf_cert(300, "ECDSA P-192", 192)];
        assert_eq!(check_key_strength(&chain).status, CheckStatus::Fail);
    }

    #[test]
    fn key_strength_rsa_4096_pass() {
        let chain = vec![leaf_cert(300, "RSA", 4096)];
        assert_eq!(check_key_strength(&chain).status, CheckStatus::Pass);
    }

    // --- Expiry window ---

    #[test]
    fn expiry_window_pass_31_days() {
        let chain = vec![leaf_cert(31, "RSA", 2048)];
        assert_eq!(check_expiry_window(&chain).status, CheckStatus::Pass);
    }

    #[test]
    fn expiry_window_warn_30_days() {
        let chain = vec![leaf_cert(30, "RSA", 2048)];
        assert_eq!(check_expiry_window(&chain).status, CheckStatus::Warn);
    }

    #[test]
    fn expiry_window_warn_8_days() {
        let chain = vec![leaf_cert(8, "RSA", 2048)];
        assert_eq!(check_expiry_window(&chain).status, CheckStatus::Warn);
    }

    #[test]
    fn expiry_window_fail_7_days() {
        let chain = vec![leaf_cert(7, "RSA", 2048)];
        assert_eq!(check_expiry_window(&chain).status, CheckStatus::Fail);
    }

    #[test]
    fn expiry_window_fail_0_days() {
        let chain = vec![leaf_cert(0, "RSA", 2048)];
        assert_eq!(check_expiry_window(&chain).status, CheckStatus::Fail);
    }

    #[test]
    fn expiry_window_skip_empty_chain() {
        assert_eq!(check_expiry_window(&[]).status, CheckStatus::Skip);
    }

    // --- TLS version ---

    #[test]
    fn tls_version_13_pass() {
        assert_eq!(check_tls_version("TLSv1.3").status, CheckStatus::Pass);
    }

    #[test]
    fn tls_version_12_warn() {
        assert_eq!(check_tls_version("TLSv1.2").status, CheckStatus::Warn);
    }

    #[test]
    fn tls_version_11_fail() {
        assert_eq!(check_tls_version("TLSv1.1").status, CheckStatus::Fail);
    }

    // --- Cipher suite classification ---

    #[test]
    fn classify_tls13_aes_128() {
        let props = classify_cipher_suite("TLS13_AES_128_GCM_SHA256");
        assert!(props.forward_secrecy);
        assert!(props.aead);
    }

    #[test]
    fn classify_tls13_chacha() {
        let props = classify_cipher_suite("TLS13_CHACHA20_POLY1305_SHA256");
        assert!(props.forward_secrecy);
        assert!(props.aead);
    }

    #[test]
    fn classify_tls12_ecdhe_gcm() {
        let props = classify_cipher_suite("TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256");
        assert!(props.forward_secrecy);
        assert!(props.aead);
    }

    #[test]
    fn classify_tls12_ecdhe_chacha() {
        let props = classify_cipher_suite("TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256");
        assert!(props.forward_secrecy);
        assert!(props.aead);
    }

    #[test]
    fn classify_tls12_static_rsa_cbc() {
        let props = classify_cipher_suite("TLS_RSA_WITH_AES_128_CBC_SHA256");
        assert!(!props.forward_secrecy);
        assert!(!props.aead);
    }

    #[test]
    fn classify_tls12_dhe_gcm() {
        let props = classify_cipher_suite("TLS_DHE_RSA_WITH_AES_256_GCM_SHA384");
        assert!(props.forward_secrecy);
        assert!(props.aead);
    }

    // --- Forward secrecy ---

    #[test]
    fn forward_secrecy_pass() {
        assert_eq!(
            check_forward_secrecy("TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256").status,
            CheckStatus::Pass
        );
    }

    #[test]
    fn forward_secrecy_fail() {
        assert_eq!(
            check_forward_secrecy("TLS_RSA_WITH_AES_128_CBC_SHA256").status,
            CheckStatus::Fail
        );
    }

    // --- AEAD cipher ---

    #[test]
    fn aead_pass() {
        assert_eq!(
            check_aead_cipher("TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256").status,
            CheckStatus::Pass
        );
    }

    #[test]
    fn aead_fail_cbc() {
        assert_eq!(
            check_aead_cipher("TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA256").status,
            CheckStatus::Fail
        );
    }

    // --- CT logged ---

    #[test]
    fn ct_skip_when_disabled() {
        assert_eq!(check_ct_logged(false, None).status, CheckStatus::Skip);
    }

    #[test]
    fn ct_warn_zero_scts() {
        assert_eq!(check_ct_logged(true, Some(0)).status, CheckStatus::Warn);
    }

    #[test]
    fn ct_warn_one_sct() {
        assert_eq!(check_ct_logged(true, Some(1)).status, CheckStatus::Warn);
    }

    #[test]
    fn ct_pass_two_scts() {
        assert_eq!(check_ct_logged(true, Some(2)).status, CheckStatus::Pass);
    }

    #[test]
    fn ct_warn_none_scts() {
        assert_eq!(check_ct_logged(true, None).status, CheckStatus::Warn);
    }

    // --- OCSP stapled ---

    #[test]
    fn ocsp_stapled_pass() {
        assert_eq!(check_ocsp_stapled(true).status, CheckStatus::Pass);
    }

    #[test]
    fn ocsp_stapled_warn() {
        let c = check_ocsp_stapled(false);
        assert_eq!(c.status, CheckStatus::Warn);
        assert!(c.detail.contains("no OCSP staple"));
    }

    // --- DANE valid ---

    #[test]
    fn dane_valid_pass_through() {
        assert_eq!(
            check_dane_valid(CheckStatus::Pass).status,
            CheckStatus::Pass
        );
        assert_eq!(
            check_dane_valid(CheckStatus::Fail).status,
            CheckStatus::Fail
        );
        assert_eq!(
            check_dane_valid(CheckStatus::Skip).status,
            CheckStatus::Skip
        );
    }

    // --- CAA compliant ---

    #[test]
    fn caa_pass_through() {
        assert_eq!(
            check_caa_compliant(CheckStatus::Pass).status,
            CheckStatus::Pass
        );
        assert_eq!(
            check_caa_compliant(CheckStatus::Fail).status,
            CheckStatus::Fail
        );
        assert_eq!(
            check_caa_compliant(CheckStatus::Skip).status,
            CheckStatus::Skip
        );
    }

    // --- Consistency ---

    #[test]
    fn consistency_skip_no_data() {
        assert_eq!(check_consistency(None).status, CheckStatus::Skip);
    }

    #[test]
    fn consistency_pass_no_mismatches() {
        let c = ConsistencyResult {
            certificates_match: true,
            tls_versions_match: true,
            cipher_suites_match: true,
            mismatches: vec![],
        };
        assert_eq!(check_consistency(Some(&c)).status, CheckStatus::Pass);
    }

    #[test]
    fn consistency_warn_with_mismatches() {
        use crate::routes::ConsistencyMismatch;
        use std::collections::HashMap;
        let c = ConsistencyResult {
            certificates_match: false,
            tls_versions_match: true,
            cipher_suites_match: true,
            mismatches: vec![ConsistencyMismatch {
                field: "leaf_certificate".to_string(),
                values: HashMap::new(),
            }],
        };
        let check = check_consistency(Some(&c));
        assert_eq!(check.status, CheckStatus::Warn);
        assert!(check.detail.contains("leaf_certificate"));
    }
}
