use crate::tls::chain::CertInfo;
use crate::validate::ValidationResult;

/// Validate a certificate chain.
pub fn validate_chain(
    chain: &[CertInfo],
    hostname: Option<&str>,
    _trust_store: &rustls::RootCertStore,
    _raw_certs: &[rustls::pki_types::CertificateDer<'_>],
) -> ValidationResult {
    let any_expired = chain.iter().any(|c| c.is_expired);
    // TODO("check not_before > now for not-yet-valid certs")
    let any_not_yet_valid = false;

    let terminates_at_self_signed = chain.last().is_some_and(|c| c.is_self_signed);
    let chain_order_correct = check_chain_order(chain);

    let leaf_covers_hostname = hostname
        .map(|h| check_hostname_match(chain, h))
        .unwrap_or(false);

    // TODO("chain trust validation requires webpki EndEntityCert API integration")
    // Phase 1: return true; proper chain validation will use webpki::EndEntityCert
    // or rustls::client::WebPkiServerVerifier
    let chain_trusted = true;

    let weakest_signature = chain
        .iter()
        .map(|c| c.signature_algorithm.as_str())
        .min_by_key(|algo| sig_strength(algo))
        .unwrap_or("unknown")
        .to_string();

    let (earliest_expiry, earliest_expiry_days) = chain
        .iter()
        .min_by_key(|c| c.days_remaining)
        .map(|c| (c.not_after.clone(), c.days_remaining))
        .unwrap_or_else(|| (String::new(), 0));

    ValidationResult {
        chain_trusted,
        terminates_at_self_signed,
        chain_order_correct,
        leaf_covers_hostname,
        any_expired,
        any_not_yet_valid,
        weakest_signature,
        earliest_expiry,
        earliest_expiry_days,
    }
}

/// Check if the chain is ordered correctly (each cert's issuer matches next cert's subject).
fn check_chain_order(chain: &[CertInfo]) -> bool {
    if chain.len() <= 1 {
        return true;
    }
    chain
        .windows(2)
        .all(|pair| pair[0].issuer == pair[1].subject)
}

/// Check if the leaf certificate covers the given hostname via SAN matching.
fn check_hostname_match(chain: &[CertInfo], hostname: &str) -> bool {
    let Some(leaf) = chain.first() else {
        return false;
    };
    let hostname_lower = hostname.to_lowercase();
    leaf.sans.iter().any(|san| {
        let san_lower = san.to_lowercase();
        if let Some(wildcard_domain) = san_lower.strip_prefix("*.") {
            // Wildcard: *.example.com matches foo.example.com but not example.com
            // and not sub.foo.example.com (no multi-level wildcard)
            match hostname_lower.strip_suffix(wildcard_domain) {
                Some(prefix) => prefix.ends_with('.') && !prefix[..prefix.len() - 1].contains('.'),
                None => false,
            }
        } else {
            san_lower == hostname_lower
        }
    })
}

/// Return a numeric strength ordering for signature algorithms.
/// Lower = weaker.
fn sig_strength(algo: &str) -> u32 {
    match algo {
        "sha1WithRSAEncryption" => 1,
        "sha256WithRSAEncryption" => 2,
        "ecdsa-with-SHA256" => 3,
        "sha384WithRSAEncryption" => 4,
        "ecdsa-with-SHA384" => 5,
        "sha512WithRSAEncryption" => 6,
        "ecdsa-with-SHA512" => 7,
        "Ed25519" => 8,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(sans: Vec<&str>, issuer: &str) -> CertInfo {
        CertInfo {
            position: "leaf".to_string(),
            subject: "CN=leaf".to_string(),
            issuer: issuer.to_string(),
            sans: sans.into_iter().map(String::from).collect(),
            serial: "01".to_string(),
            not_before: "2024-01-01".to_string(),
            not_after: "2026-12-31".to_string(),
            days_remaining: 300,
            key_type: "RSA".to_string(),
            key_size: 2048,
            signature_algorithm: "sha256WithRSAEncryption".to_string(),
            fingerprint_sha256: "AA:BB".to_string(),
            is_expired: false,
            is_self_signed: false,
        }
    }

    fn intermediate(subject: &str, issuer: &str) -> CertInfo {
        CertInfo {
            position: "intermediate".to_string(),
            subject: subject.to_string(),
            issuer: issuer.to_string(),
            sans: vec![],
            serial: "02".to_string(),
            not_before: "2024-01-01".to_string(),
            not_after: "2026-12-31".to_string(),
            days_remaining: 300,
            key_type: "RSA".to_string(),
            key_size: 2048,
            signature_algorithm: "sha256WithRSAEncryption".to_string(),
            fingerprint_sha256: "CC:DD".to_string(),
            is_expired: false,
            is_self_signed: false,
        }
    }

    // --- check_chain_order tests ---

    #[test]
    fn empty_chain_is_ordered() {
        assert!(check_chain_order(&[]));
    }

    #[test]
    fn single_cert_is_ordered() {
        let chain = vec![leaf(vec!["example.com"], "CN=issuer")];
        assert!(check_chain_order(&chain));
    }

    #[test]
    fn correctly_ordered_chain() {
        let chain = vec![
            leaf(vec!["example.com"], "CN=intermediate"),
            intermediate("CN=intermediate", "CN=root"),
        ];
        assert!(check_chain_order(&chain));
    }

    #[test]
    fn incorrectly_ordered_chain() {
        let chain = vec![
            leaf(vec!["example.com"], "CN=intermediate"),
            intermediate("CN=wrong", "CN=root"),
        ];
        assert!(!check_chain_order(&chain));
    }

    // --- check_hostname_match tests ---

    #[test]
    fn exact_match() {
        let chain = vec![leaf(vec!["example.com"], "CN=issuer")];
        assert!(check_hostname_match(&chain, "example.com"));
    }

    #[test]
    fn exact_match_case_insensitive() {
        let chain = vec![leaf(vec!["Example.COM"], "CN=issuer")];
        assert!(check_hostname_match(&chain, "example.com"));
    }

    #[test]
    fn no_match() {
        let chain = vec![leaf(vec!["example.com"], "CN=issuer")];
        assert!(!check_hostname_match(&chain, "other.com"));
    }

    #[test]
    fn wildcard_match() {
        let chain = vec![leaf(vec!["*.example.com"], "CN=issuer")];
        assert!(check_hostname_match(&chain, "foo.example.com"));
    }

    #[test]
    fn wildcard_no_match_bare_domain() {
        let chain = vec![leaf(vec!["*.example.com"], "CN=issuer")];
        assert!(!check_hostname_match(&chain, "example.com"));
    }

    #[test]
    fn wildcard_no_match_multi_level() {
        let chain = vec![leaf(vec!["*.example.com"], "CN=issuer")];
        assert!(!check_hostname_match(&chain, "sub.foo.example.com"));
    }

    #[test]
    fn multiple_sans_any_match() {
        let chain = vec![leaf(vec!["a.com", "b.com", "c.com"], "CN=issuer")];
        assert!(check_hostname_match(&chain, "b.com"));
    }

    #[test]
    fn empty_chain_no_match() {
        assert!(!check_hostname_match(&[], "example.com"));
    }

    #[test]
    fn empty_sans_no_match() {
        let chain = vec![leaf(vec![], "CN=issuer")];
        assert!(!check_hostname_match(&chain, "example.com"));
    }

    // --- sig_strength tests ---

    #[test]
    fn sha1_weaker_than_sha256() {
        assert!(sig_strength("sha1WithRSAEncryption") < sig_strength("sha256WithRSAEncryption"));
    }

    #[test]
    fn ecdsa_stronger_than_rsa_same_hash() {
        assert!(sig_strength("ecdsa-with-SHA256") > sig_strength("sha256WithRSAEncryption"));
    }

    #[test]
    fn unknown_algo_is_weakest() {
        assert_eq!(sig_strength("some-unknown-algo"), 0);
    }
}
