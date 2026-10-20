use rustls::RootCertStore;
use rustls::pki_types::{CertificateDer, UnixTime};
use rustls::server::ParsedCertificate;

use crate::tls::chain::CertInfo;
use crate::validate::ValidationResult;

/// Validate a certificate chain using webpki trust store verification.
pub fn validate_chain(
    chain: &[CertInfo],
    hostname: Option<&str>,
    roots: &RootCertStore,
    raw_certs: &[CertificateDer<'_>],
) -> ValidationResult {
    let any_expired = chain.iter().any(|c| c.is_expired);
    let any_not_yet_valid = check_any_not_yet_valid(raw_certs);

    let terminates_at_self_signed = chain.last().is_some_and(|c| c.is_self_signed);
    let chain_order_correct = check_chain_order(chain);

    let leaf_covers_hostname = hostname
        .map(|h| check_hostname_match(chain, h))
        .unwrap_or(false);

    let (chain_trusted, chain_trust_reason) = verify_chain_trust(roots, raw_certs);
    let ends_at_anchor = chain_ends_at_anchor(chain, roots, raw_certs);

    let weakest_signature = weakest_signature(chain);

    let (earliest_expiry, earliest_expiry_days) = chain
        .iter()
        .min_by_key(|c| c.days_remaining)
        .map(|c| (c.not_after.clone(), c.days_remaining))
        .unwrap_or_else(|| (String::new(), 0));

    ValidationResult {
        chain_trusted,
        chain_trust_reason,
        terminates_at_self_signed,
        ends_at_anchor,
        chain_order_correct,
        leaf_covers_hostname,
        any_expired,
        any_not_yet_valid,
        weakest_signature,
        earliest_expiry,
        earliest_expiry_days,
    }
}

/// The root set `chain_trusted` verifies against: the webpki-roots copy of Mozilla's roots.
pub const TRUST_STORE: &str = "the Mozilla root program";

/// Verify chain trust against the root store. The hostname plays no part in it.
/// Returns `(trusted, reason)` where reason captures the error message on failure.
fn verify_chain_trust(
    roots: &RootCertStore,
    raw_certs: &[CertificateDer<'_>],
) -> (bool, Option<String>) {
    let Some(end_entity) = raw_certs.first() else {
        return (false, Some("empty certificate chain".to_string()));
    };
    let intermediates = &raw_certs[1..];

    let parsed = match ParsedCertificate::try_from(end_entity) {
        Ok(p) => p,
        Err(e) => return (false, Some(e.to_string())),
    };
    let algs = rustls::crypto::ring::default_provider()
        .signature_verification_algorithms
        .all;
    match rustls::client::verify_server_cert_signed_by_trust_anchor(
        &parsed,
        roots,
        intermediates,
        UnixTime::now(),
        algs,
    ) {
        Ok(()) => (true, None),
        Err(e) => (false, Some(e.to_string())),
    }
}

/// True when a served certificate is self-signed, or its subject or issuer is the subject
/// of a trust anchor in the store: the served chain reaches the end of a path, so no
/// intermediate is missing. Extra certificates served past that point do not count.
fn chain_ends_at_anchor(
    chain: &[CertInfo],
    roots: &RootCertStore,
    raw_certs: &[CertificateDer<'_>],
) -> bool {
    use x509_parser::prelude::*;
    if chain.iter().any(|c| c.is_self_signed) {
        return true;
    }
    raw_certs.iter().any(|der| {
        let Ok((_, cert)) = X509Certificate::from_der(der.as_ref()) else {
            return false;
        };
        let subject = der_contents(cert.subject().as_raw());
        let issuer = der_contents(cert.issuer().as_raw());
        roots.roots.iter().any(|anchor| {
            let name = anchor.subject.as_ref();
            name == subject || name == issuer
        })
    })
}

/// Strip the outer tag and length of a DER element; webpki stores a Name without them.
fn der_contents(der: &[u8]) -> &[u8] {
    let Some(&len_byte) = der.get(1) else {
        return der;
    };
    let header = if len_byte & 0x80 == 0 {
        2
    } else {
        2 + usize::from(len_byte & 0x7f)
    };
    der.get(header..).unwrap_or(der)
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

/// Weakest signature algorithm in the chain, ignoring a root's self-signature (index > 0).
pub fn weakest_signature(chain: &[CertInfo]) -> String {
    chain
        .iter()
        .enumerate()
        .filter(|(i, c)| *i == 0 || !c.is_self_signed)
        .map(|(_, c)| c.signature_algorithm.as_str())
        .min_by_key(|algo| sig_strength(algo))
        .unwrap_or("unknown")
        .to_string()
}

/// Return a numeric strength ordering for signature algorithms.
/// Lower = weaker: weak (SHA-1/MD5/MD2) < unreadable or unrecognised < known strong.
fn sig_strength(algo: &str) -> u32 {
    let lower = algo.to_lowercase();
    if ["sha1", "md5", "md2"].iter().any(|w| lower.contains(w)) {
        return 0;
    }
    match algo {
        "sha256WithRSAEncryption" => 2,
        "ecdsa-with-SHA256" => 3,
        "sha384WithRSAEncryption" => 4,
        "ecdsa-with-SHA384" => 5,
        "sha512WithRSAEncryption" => 6,
        "ecdsa-with-SHA512" => 7,
        "Ed25519" => 8,
        "Ed448" => 9,
        _ => 1,
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
            fingerprint_sha1: "AA:BB".to_string(),
            lifetime_days: 365,
            is_expired: false,
            is_self_signed: false,
            cert_policy: "DV".to_string(),
            ocsp_url: None,
            ca_issuers_url: None,
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
            fingerprint_sha1: "CC:DD".to_string(),
            lifetime_days: 365,
            is_expired: false,
            is_self_signed: false,
            cert_policy: "DV".to_string(),
            ocsp_url: None,
            ca_issuers_url: None,
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

    fn self_signed_der() -> CertificateDer<'static> {
        let params = rcgen::CertificateParams::new(vec!["example.com".to_string()]).unwrap();
        let cert = params
            .self_signed(&rcgen::KeyPair::generate().unwrap())
            .unwrap();
        CertificateDer::from(cert.der().to_vec())
    }

    fn webpki_roots_store() -> RootCertStore {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut root_store = RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        root_store
    }

    #[test]
    fn self_signed_cert_not_trusted() {
        let roots = webpki_roots_store();
        let (trusted, reason) = verify_chain_trust(&roots, &[self_signed_der()]);
        assert!(!trusted);
        assert!(reason.is_some());
    }

    #[test]
    fn empty_chain_not_trusted() {
        let roots = webpki_roots_store();
        let (trusted, reason) = verify_chain_trust(&roots, &[]);
        assert!(!trusted);
        assert_eq!(reason.as_deref(), Some("empty certificate chain"));
    }

    #[test]
    fn cert_in_store_is_trusted_without_hostname() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let der = self_signed_der();
        let mut roots = RootCertStore::empty();
        roots.add(der.clone()).unwrap();
        let (trusted, reason) = verify_chain_trust(&roots, &[der]);
        assert!(trusted, "{reason:?}");
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
    fn unknown_algo_ranks_between_weak_and_strong() {
        assert!(sig_strength("sha1WithRSAEncryption") < sig_strength("some-unknown-algo"));
        assert!(sig_strength("some-unknown-algo") < sig_strength("sha256WithRSAEncryption"));
        assert_eq!(sig_strength(""), sig_strength("Unknown(1.2.3)"));
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

    #[test]
    fn der_contents_strips_short_and_long_form_headers() {
        assert_eq!(der_contents(&[0x30, 0x02, 0xAA, 0xBB]), &[0xAA, 0xBB]);
        let mut long = vec![0x30, 0x81, 0x80];
        long.extend(std::iter::repeat_n(0xCC, 0x80));
        assert_eq!(der_contents(&long).len(), 0x80);
        let mut longer = vec![0x30, 0x82, 0x01, 0x00];
        longer.extend(std::iter::repeat_n(0xDD, 0x100));
        assert_eq!(der_contents(&longer).len(), 0x100);
    }
}
