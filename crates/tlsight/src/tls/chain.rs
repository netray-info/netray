use rustls::pki_types::CertificateDer;
use serde::Serialize;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use utoipa::ToSchema;
use x509_parser::prelude::*;

/// Parsed certificate info.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CertInfo {
    pub position: String,
    pub subject: String,
    pub issuer: String,
    pub sans: Vec<String>,
    pub serial: String,
    pub not_before: String,
    pub not_after: String,
    pub days_remaining: i64,
    pub key_type: String,
    pub key_size: u32,
    pub signature_algorithm: String,
    pub fingerprint_sha256: String,
    pub fingerprint_sha1: String,
    pub lifetime_days: i64,
    pub is_expired: bool,
    pub is_self_signed: bool,
}

/// Parse a chain of DER-encoded certificates into structured CertInfo.
pub fn parse_chain(certs: &[CertificateDer<'_>]) -> Vec<CertInfo> {
    certs
        .iter()
        .enumerate()
        .map(|(i, cert_der)| parse_cert(cert_der.as_ref(), i, certs.len()))
        .collect()
}

fn parse_cert(der: &[u8], index: usize, chain_len: usize) -> CertInfo {
    let fingerprint_sha256 = format_fingerprint(&Sha256::digest(der));
    let fingerprint_sha1 = format_fingerprint(&Sha1::digest(der));

    let (_, cert) = match X509Certificate::from_der(der) {
        Ok(parsed) => parsed,
        Err(e) => {
            return CertInfo {
                position: classify_position(index, chain_len, false),
                subject: format!("<parse error: {e}>"),
                issuer: String::new(),
                sans: vec![],
                serial: String::new(),
                not_before: String::new(),
                not_after: String::new(),
                days_remaining: 0,
                key_type: String::new(),
                key_size: 0,
                signature_algorithm: String::new(),
                fingerprint_sha256,
                fingerprint_sha1,
                lifetime_days: 0,
                is_expired: false,
                is_self_signed: false,
            };
        }
    };

    let subject = cert.subject().to_string();
    let issuer = cert.issuer().to_string();
    let is_self_signed = subject == issuer;

    let sans = extract_sans(&cert);
    let serial = format_serial(cert.raw_serial());

    let not_before = cert.validity().not_before.to_string();
    let not_after = cert.validity().not_after.to_string();

    let not_before_ts = cert.validity().not_before.timestamp();
    let not_after_ts = cert.validity().not_after.timestamp();
    let now_ts = ::time::OffsetDateTime::now_utc().unix_timestamp();
    let days_remaining = (not_after_ts - now_ts) / 86400;
    let lifetime_days = (not_after_ts - not_before_ts) / 86400;
    let is_expired = days_remaining < 0;

    let (key_type, key_size) = extract_key_info(&cert);
    let signature_algorithm = format_sig_algo(&cert.signature_algorithm.algorithm.to_id_string());

    CertInfo {
        position: classify_position(index, chain_len, is_self_signed),
        subject,
        issuer,
        sans,
        serial,
        not_before,
        not_after,
        days_remaining,
        key_type,
        key_size,
        signature_algorithm,
        fingerprint_sha256,
        fingerprint_sha1,
        lifetime_days,
        is_expired,
        is_self_signed,
    }
}

/// Classify certificate position per SDD 7.4.
fn classify_position(index: usize, chain_len: usize, is_self_signed: bool) -> String {
    if index == 0 {
        if is_self_signed {
            "leaf_self_signed"
        } else {
            "leaf"
        }
    } else if index == chain_len - 1 {
        if is_self_signed {
            "root"
        } else {
            "intermediate"
        }
    } else {
        "intermediate"
    }
    .to_string()
}

fn extract_sans(cert: &X509Certificate) -> Vec<String> {
    cert.subject_alternative_name()
        .ok()
        .flatten()
        .map(|san| {
            san.value
                .general_names
                .iter()
                .filter_map(|name| match name {
                    GeneralName::DNSName(dns) => Some(dns.to_string()),
                    GeneralName::IPAddress(ip_bytes) => format_ip_san(ip_bytes),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn format_ip_san(ip_bytes: &[u8]) -> Option<String> {
    match ip_bytes.len() {
        4 => Some(format!(
            "{}.{}.{}.{}",
            ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3]
        )),
        16 => {
            let octets: [u8; 16] = ip_bytes.try_into().ok()?;
            Some(std::net::Ipv6Addr::from(octets).to_string())
        }
        _ => None,
    }
}

fn extract_key_info(cert: &X509Certificate) -> (String, u32) {
    let spki = cert.public_key();
    let algo_oid = spki.algorithm.algorithm.to_id_string();

    match algo_oid.as_str() {
        // RSA
        "1.2.840.113549.1.1.1" => {
            let key_size = spki.parsed().map(|pk| pk.key_size() as u32).unwrap_or(0);
            ("RSA".to_string(), key_size)
        }
        // EC
        "1.2.840.10045.2.1" => {
            let curve = spki
                .algorithm
                .parameters
                .as_ref()
                .and_then(|p| p.as_oid().ok())
                .map(|oid| oid.to_id_string());
            let (name, size) = match curve.as_deref() {
                Some("1.2.840.10045.3.1.7") => ("ECDSA P-256", 256),
                Some("1.3.132.0.34") => ("ECDSA P-384", 384),
                Some("1.3.132.0.35") => ("ECDSA P-521", 521),
                _ => ("ECDSA", 0),
            };
            (name.to_string(), size)
        }
        // Ed25519
        "1.3.101.112" => ("Ed25519".to_string(), 256),
        // Ed448
        "1.3.101.113" => ("Ed448".to_string(), 456),
        other => (format!("Unknown({other})"), 0),
    }
}

fn format_sig_algo(oid: &str) -> String {
    match oid {
        "1.2.840.113549.1.1.5" => "sha1WithRSAEncryption".to_string(),
        "1.2.840.113549.1.1.11" => "sha256WithRSAEncryption".to_string(),
        "1.2.840.113549.1.1.12" => "sha384WithRSAEncryption".to_string(),
        "1.2.840.113549.1.1.13" => "sha512WithRSAEncryption".to_string(),
        "1.2.840.10045.4.3.2" => "ecdsa-with-SHA256".to_string(),
        "1.2.840.10045.4.3.3" => "ecdsa-with-SHA384".to_string(),
        "1.2.840.10045.4.3.4" => "ecdsa-with-SHA512".to_string(),
        "1.3.101.112" => "Ed25519".to_string(),
        other => format!("Unknown({other})"),
    }
}

fn format_fingerprint(hash: &[u8]) -> String {
    hash.iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn format_serial(serial: &[u8]) -> String {
    serial
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_classification_single_cert() {
        assert_eq!(classify_position(0, 1, false), "leaf");
        assert_eq!(classify_position(0, 1, true), "leaf_self_signed");
    }

    #[test]
    fn position_classification_two_certs() {
        assert_eq!(classify_position(0, 2, false), "leaf");
        assert_eq!(classify_position(1, 2, true), "root");
        assert_eq!(classify_position(1, 2, false), "intermediate");
    }

    #[test]
    fn position_classification_three_certs() {
        assert_eq!(classify_position(0, 3, false), "leaf");
        assert_eq!(classify_position(1, 3, false), "intermediate");
        assert_eq!(classify_position(2, 3, true), "root");
        assert_eq!(classify_position(2, 3, false), "intermediate");
    }

    #[test]
    fn fingerprint_format() {
        let hash = [0xAB, 0xCD, 0xEF, 0x01];
        assert_eq!(format_fingerprint(&hash), "AB:CD:EF:01");
    }

    #[test]
    fn fingerprint_empty() {
        assert_eq!(format_fingerprint(&[]), "");
    }

    #[test]
    fn serial_format() {
        let serial = [0x04, 0xAB, 0xCD];
        assert_eq!(format_serial(&serial), "04:AB:CD");
    }

    #[test]
    fn serial_single_byte() {
        assert_eq!(format_serial(&[0x01]), "01");
    }

    #[test]
    fn sig_algo_mapping() {
        assert_eq!(
            format_sig_algo("1.2.840.113549.1.1.11"),
            "sha256WithRSAEncryption"
        );
        assert_eq!(
            format_sig_algo("1.2.840.113549.1.1.12"),
            "sha384WithRSAEncryption"
        );
        assert_eq!(
            format_sig_algo("1.2.840.113549.1.1.13"),
            "sha512WithRSAEncryption"
        );
        assert_eq!(
            format_sig_algo("1.2.840.113549.1.1.5"),
            "sha1WithRSAEncryption"
        );
        assert_eq!(format_sig_algo("1.2.840.10045.4.3.2"), "ecdsa-with-SHA256");
        assert_eq!(format_sig_algo("1.2.840.10045.4.3.3"), "ecdsa-with-SHA384");
        assert_eq!(format_sig_algo("1.2.840.10045.4.3.4"), "ecdsa-with-SHA512");
        assert_eq!(format_sig_algo("1.3.101.112"), "Ed25519");
        assert!(format_sig_algo("9.9.9").starts_with("Unknown"));
    }

    #[test]
    fn ip_san_v4() {
        assert_eq!(
            format_ip_san(&[192, 168, 1, 1]),
            Some("192.168.1.1".to_string())
        );
    }

    #[test]
    fn ip_san_v6() {
        let mut bytes = [0u8; 16];
        bytes[15] = 1; // ::1
        assert_eq!(format_ip_san(&bytes), Some("::1".to_string()));
    }

    #[test]
    fn ip_san_invalid_length() {
        assert_eq!(format_ip_san(&[1, 2, 3]), None);
    }

    #[test]
    fn parse_chain_empty() {
        let certs: Vec<CertificateDer<'_>> = vec![];
        let result = parse_chain(&certs);
        assert!(result.is_empty());
    }

    #[test]
    fn parse_chain_invalid_der() {
        let garbage = CertificateDer::from(vec![0xFF, 0xFF, 0xFF]);
        let result = parse_chain(&[garbage]);
        assert_eq!(result.len(), 1);
        assert!(result[0].subject.starts_with("<parse error:"));
        assert_eq!(result[0].position, "leaf");
        assert!(!result[0].fingerprint_sha256.is_empty());
    }

    #[test]
    fn parse_chain_self_signed_cert() {
        // Generate a self-signed cert using rcgen for testing
        let params = rcgen::CertificateParams::new(vec!["example.com".to_string()]).unwrap();
        let cert = params
            .self_signed(&rcgen::KeyPair::generate().unwrap())
            .unwrap();
        let der = CertificateDer::from(cert.der().to_vec());

        let result = parse_chain(&[der]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].position, "leaf_self_signed");
        assert!(result[0].is_self_signed);
        assert!(result[0].sans.contains(&"example.com".to_string()));
        assert!(!result[0].fingerprint_sha256.is_empty());
        assert!(result[0].days_remaining > 0);
        assert!(!result[0].is_expired);
    }
}
