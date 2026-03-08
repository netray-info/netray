use rustls::client::danger::ServerCertVerifier;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};

use crate::tls::chain::CertInfo;
use crate::validate::ValidationResult;

/// Validate a certificate chain using webpki trust store verification.
pub fn validate_chain(
    chain: &[CertInfo],
    hostname: Option<&str>,
    verifier: &dyn ServerCertVerifier,
    raw_certs: &[CertificateDer<'_>],
) -> ValidationResult {
    let any_expired = chain.iter().any(|c| c.is_expired);
    let any_not_yet_valid = check_any_not_yet_valid(raw_certs);

    let terminates_at_self_signed = chain.last().is_some_and(|c| c.is_self_signed);
    let chain_order_correct = check_chain_order(chain);

    let leaf_covers_hostname = hostname
        .map(|h| check_hostname_match(chain, h))
        .unwrap_or(false);

    let (chain_trusted, chain_trust_reason) = verify_chain_trust(verifier, hostname, raw_certs);

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
        chain_trust_reason,
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

/// Verify chain trust using rustls WebPkiServerVerifier.
/// Returns `(trusted, reason)` where reason captures the error message on failure.
fn verify_chain_trust(
    verifier: &dyn ServerCertVerifier,
    hostname: Option<&str>,
    raw_certs: &[CertificateDer<'_>],
) -> (bool, Option<String>) {
    let Some(end_entity) = raw_certs.first() else {
        return (false, Some("empty certificate chain".to_string()));
    };
    let intermediates = if raw_certs.len() > 1 {
        &raw_certs[1..]
    } else {
        &[]
    };

    // We need a ServerName to verify. For IP input (no hostname), we can't do
    // standard hostname verification — return false gracefully since the summary
    // will mark hostname_match as Skip anyway.
    let server_name = match hostname {
        Some(h) => match ServerName::try_from(h.to_owned()) {
            Ok(sn) => sn,
            Err(_) => return (false, Some(format!("invalid server name: {h}"))),
        },
        None => return (false, Some("no hostname for trust verification".to_string())),
    };

    let now = UnixTime::now();
    match verifier.verify_server_cert(end_entity, intermediates, &server_name, &[], now) {
        Ok(_) => (true, None),
        Err(e) => (false, Some(e.to_string())),
    }
}

/// Check if any certificate in the chain has a not_before date in the future.
fn check_any_not_yet_valid(raw_certs: &[CertificateDer<'_>]) -> bool {
    use x509_parser::prelude::*;
    let now_ts = ::time::OffsetDateTime::now_utc().unix_timestamp();
    raw_certs.iter().any(|cert_der| {
        let Ok((_, cert)) = X509Certificate::from_der(cert_der.as_ref()) else {
            return false;
        };
        cert.validity().not_before.timestamp() > now_ts
    })
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

    // --- verify_chain_trust tests ---

    fn test_verifier() -> std::sync::Arc<dyn ServerCertVerifier> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        rustls::client::WebPkiServerVerifier::builder(std::sync::Arc::new(root_store))
            .build()
            .unwrap()
    }

    #[test]
    fn self_signed_cert_not_trusted() {
        let verifier = test_verifier();
        let params = rcgen::CertificateParams::new(vec!["example.com".to_string()]).unwrap();
        let cert = params
            .self_signed(&rcgen::KeyPair::generate().unwrap())
            .unwrap();
        let der = CertificateDer::from(cert.der().to_vec());
        let (trusted, reason) = verify_chain_trust(verifier.as_ref(), Some("example.com"), &[der]);
        assert!(!trusted);
        assert!(reason.is_some());
    }

    #[test]
    fn empty_chain_not_trusted() {
        let verifier = test_verifier();
        let (trusted, reason) = verify_chain_trust(verifier.as_ref(), Some("example.com"), &[]);
        assert!(!trusted);
        assert_eq!(reason.as_deref(), Some("empty certificate chain"));
    }

    #[test]
    fn no_hostname_not_trusted() {
        let verifier = test_verifier();
        let params = rcgen::CertificateParams::new(vec!["example.com".to_string()]).unwrap();
        let cert = params
            .self_signed(&rcgen::KeyPair::generate().unwrap())
            .unwrap();
        let der = CertificateDer::from(cert.der().to_vec());
        let (trusted, reason) = verify_chain_trust(verifier.as_ref(), None, &[der]);
        assert!(!trusted);
        assert_eq!(
            reason.as_deref(),
            Some("no hostname for trust verification")
        );
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

    // --- check_any_not_yet_valid tests ---

    #[test]
    fn valid_cert_is_not_future() {
        let params = rcgen::CertificateParams::new(vec!["example.com".to_string()]).unwrap();
        let cert = params
            .self_signed(&rcgen::KeyPair::generate().unwrap())
            .unwrap();
        let der = CertificateDer::from(cert.der().to_vec());
        assert!(!check_any_not_yet_valid(&[der]));
    }

    #[test]
    fn empty_certs_not_future() {
        assert!(!check_any_not_yet_valid(&[]));
    }
}
