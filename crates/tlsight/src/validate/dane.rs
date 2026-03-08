//! DANE/TLSA certificate matching.
//!
//! Implements full DANE matching logic per RFC 6698. Currently unit-tested only;
//! not invoked at runtime until DNSSEC validation is available.

use rustls::pki_types::CertificateDer;
use sha2::{Digest, Sha256, Sha512};

use crate::dns::TlsaRecord;

/// Check if a TLSA record matches against a certificate chain.
///
/// - `cert_usage` 0 (PKIX-TA) / 2 (DANE-TA): match any cert in chain
/// - `cert_usage` 1 (PKIX-EE) / 3 (DANE-EE): match leaf only
/// - `selector` 0: full DER-encoded certificate
/// - `selector` 1: SubjectPublicKeyInfo from the certificate
/// - `matching_type` 0: exact match
/// - `matching_type` 1: SHA-256 hash
/// - `matching_type` 2: SHA-512 hash
pub fn dane_match(record: &TlsaRecord, chain: &[CertificateDer<'_>]) -> bool {
    if chain.is_empty() {
        return false;
    }

    let certs_to_check: &[CertificateDer<'_>] = match record.cert_usage {
        1 | 3 => &chain[..1], // EE: leaf only
        0 | 2 => chain,       // CA/TA: any cert in chain
        _ => return false,
    };

    certs_to_check.iter().any(|cert| match_cert(record, cert))
}

fn match_cert(record: &TlsaRecord, cert: &CertificateDer<'_>) -> bool {
    let selected = match record.selector {
        0 => cert.as_ref().to_vec(),
        1 => match extract_spki(cert) {
            Some(spki) => spki,
            None => return false,
        },
        _ => return false,
    };

    let computed = match record.matching_type {
        0 => selected,
        1 => Sha256::digest(&selected).to_vec(),
        2 => Sha512::digest(&selected).to_vec(),
        _ => return false,
    };

    computed == record.cert_data
}

/// Extract SubjectPublicKeyInfo bytes from a DER-encoded certificate.
fn extract_spki(cert: &CertificateDer<'_>) -> Option<Vec<u8>> {
    use x509_parser::prelude::*;
    let (_, x509) = X509Certificate::from_der(cert.as_ref()).ok()?;
    Some(x509.tbs_certificate.subject_pki.raw.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256, Sha512};

    fn test_cert() -> (CertificateDer<'static>, Vec<u8>) {
        let key = rcgen::KeyPair::generate().unwrap();
        let params = rcgen::CertificateParams::new(vec!["example.com".to_string()]).unwrap();
        let cert = params.self_signed(&key).unwrap();
        let der = cert.der().to_vec();
        let cert_der = CertificateDer::from(der.clone());
        (cert_der, der)
    }

    fn spki_of(cert: &CertificateDer<'_>) -> Vec<u8> {
        extract_spki(cert).unwrap()
    }

    // --- Usage × Selector × Matching combinations ---

    // Usage 3 (DANE-EE), Selector 0 (Full), Matching 0 (Exact)
    #[test]
    fn dane_ee_full_exact() {
        let (cert, der) = test_cert();
        let record = TlsaRecord {
            cert_usage: 3,
            selector: 0,
            matching_type: 0,
            display: String::new(),
            cert_data: der,
        };
        assert!(dane_match(&record, &[cert]));
    }

    // Usage 3 (DANE-EE), Selector 0 (Full), Matching 1 (SHA-256)
    #[test]
    fn dane_ee_full_sha256() {
        let (cert, der) = test_cert();
        let hash = Sha256::digest(&der).to_vec();
        let record = TlsaRecord {
            cert_usage: 3,
            selector: 0,
            matching_type: 1,
            display: String::new(),
            cert_data: hash,
        };
        assert!(dane_match(&record, &[cert]));
    }

    // Usage 3 (DANE-EE), Selector 0 (Full), Matching 2 (SHA-512)
    #[test]
    fn dane_ee_full_sha512() {
        let (cert, der) = test_cert();
        let hash = Sha512::digest(&der).to_vec();
        let record = TlsaRecord {
            cert_usage: 3,
            selector: 0,
            matching_type: 2,
            display: String::new(),
            cert_data: hash,
        };
        assert!(dane_match(&record, &[cert]));
    }

    // Usage 3 (DANE-EE), Selector 1 (SPKI), Matching 0 (Exact)
    #[test]
    fn dane_ee_spki_exact() {
        let (cert, _) = test_cert();
        let spki = spki_of(&cert);
        let record = TlsaRecord {
            cert_usage: 3,
            selector: 1,
            matching_type: 0,
            display: String::new(),
            cert_data: spki,
        };
        assert!(dane_match(&record, &[cert]));
    }

    // Usage 3 (DANE-EE), Selector 1 (SPKI), Matching 1 (SHA-256)
    #[test]
    fn dane_ee_spki_sha256() {
        let (cert, _) = test_cert();
        let spki = spki_of(&cert);
        let hash = Sha256::digest(&spki).to_vec();
        let record = TlsaRecord {
            cert_usage: 3,
            selector: 1,
            matching_type: 1,
            display: String::new(),
            cert_data: hash,
        };
        assert!(dane_match(&record, &[cert]));
    }

    // Usage 3 (DANE-EE), Selector 1 (SPKI), Matching 2 (SHA-512)
    #[test]
    fn dane_ee_spki_sha512() {
        let (cert, _) = test_cert();
        let spki = spki_of(&cert);
        let hash = Sha512::digest(&spki).to_vec();
        let record = TlsaRecord {
            cert_usage: 3,
            selector: 1,
            matching_type: 2,
            display: String::new(),
            cert_data: hash,
        };
        assert!(dane_match(&record, &[cert]));
    }

    // Usage 0/2 (CA/TA) match any cert in chain
    #[test]
    fn ca_usage_matches_intermediate() {
        let (leaf, _) = test_cert();
        let (intermediate, intermediate_der) = test_cert();
        let hash = Sha256::digest(&intermediate_der).to_vec();
        let record = TlsaRecord {
            cert_usage: 2, // DANE-TA
            selector: 0,
            matching_type: 1,
            display: String::new(),
            cert_data: hash,
        };
        assert!(dane_match(&record, &[leaf, intermediate]));
    }

    // Usage 1/3 (EE) only matches leaf, not intermediate
    #[test]
    fn ee_usage_does_not_match_intermediate() {
        let (leaf, _) = test_cert();
        let (intermediate, intermediate_der) = test_cert();
        let hash = Sha256::digest(&intermediate_der).to_vec();
        let record = TlsaRecord {
            cert_usage: 3, // DANE-EE
            selector: 0,
            matching_type: 1,
            display: String::new(),
            cert_data: hash,
        };
        // Should NOT match because EE only checks leaf
        assert!(!dane_match(&record, &[leaf, intermediate]));
    }

    // Wrong data → no match
    #[test]
    fn wrong_data_no_match() {
        let (cert, _) = test_cert();
        let record = TlsaRecord {
            cert_usage: 3,
            selector: 0,
            matching_type: 1,
            display: String::new(),
            cert_data: vec![0xde, 0xad, 0xbe, 0xef],
        };
        assert!(!dane_match(&record, &[cert]));
    }

    // Empty chain → no match
    #[test]
    fn empty_chain_no_match() {
        let record = TlsaRecord {
            cert_usage: 3,
            selector: 0,
            matching_type: 0,
            display: String::new(),
            cert_data: vec![],
        };
        assert!(!dane_match(&record, &[]));
    }

    // Unknown selector → no match
    #[test]
    fn unknown_selector_no_match() {
        let (cert, der) = test_cert();
        let record = TlsaRecord {
            cert_usage: 3,
            selector: 99,
            matching_type: 0,
            display: String::new(),
            cert_data: der,
        };
        assert!(!dane_match(&record, &[cert]));
    }

    // Unknown matching type → no match
    #[test]
    fn unknown_matching_type_no_match() {
        let (cert, der) = test_cert();
        let record = TlsaRecord {
            cert_usage: 3,
            selector: 0,
            matching_type: 99,
            display: String::new(),
            cert_data: der,
        };
        assert!(!dane_match(&record, &[cert]));
    }

    // Unknown usage → no match
    #[test]
    fn unknown_usage_no_match() {
        let (cert, der) = test_cert();
        let record = TlsaRecord {
            cert_usage: 99,
            selector: 0,
            matching_type: 0,
            display: String::new(),
            cert_data: der,
        };
        assert!(!dane_match(&record, &[cert]));
    }
}
