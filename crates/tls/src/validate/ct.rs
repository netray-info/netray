//! Certificate Transparency SCT extraction from leaf certificates.
//!
//! Extracts embedded Signed Certificate Timestamps (SCTs) from the X.509
//! SCT list extension (OID 1.3.6.1.4.1.11129.2.4.2). No external API calls.

use std::fmt::Write;

use serde::Serialize;
use utoipa::ToSchema;
use x509_parser::prelude::*;

use super::CheckStatus;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CtInfo {
    pub sct_count: usize,
    pub scts: Vec<SctEntry>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SctEntry {
    pub version: u8,
    pub log_id: String,
    pub timestamp: String,
}

/// Extract CT info from a DER-encoded leaf certificate.
///
/// Returns `None` if the certificate has no SCT list extension.
pub fn extract_ct_info(cert_der: &[u8]) -> Option<CtInfo> {
    let (_, cert) = X509Certificate::from_der(cert_der).ok()?;

    for ext in cert.extensions() {
        if let ParsedExtension::SCT(scts) = ext.parsed_extension() {
            let entries: Vec<SctEntry> = scts
                .iter()
                .map(|sct| SctEntry {
                    version: sct.version.0,
                    log_id: hex_encode(sct.id.key_id),
                    timestamp: format_sct_timestamp(sct.timestamp),
                })
                .collect();

            return Some(CtInfo {
                sct_count: entries.len(),
                scts: entries,
            });
        }
    }

    None
}

/// Determine CT check status from extracted CT info.
///
/// - No SCT extension → Warn (cert not CT-logged)
/// - 1 SCT → Warn (insufficient log diversity for Chrome CT policy)
/// - 2+ SCTs → Pass (meets Chrome CT policy)
pub fn check_ct_status(ct_info: Option<&CtInfo>) -> CheckStatus {
    match ct_info {
        None => CheckStatus::Warn,
        Some(info) if info.sct_count < 2 => CheckStatus::Warn,
        Some(_) => CheckStatus::Pass,
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Format milliseconds-since-epoch as ISO 8601 UTC string.
fn format_sct_timestamp(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    let nanos = ((ms % 1000) * 1_000_000) as i32;
    match ::time::OffsetDateTime::from_unix_timestamp(secs)
        .ok()
        .and_then(|dt| {
            dt.replace_nanosecond(nanos as u32).ok().map(|dt| {
                dt.format(&::time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| format!("{secs}"))
            })
        }) {
        Some(s) => s,
        None => format!("{ms}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_signed_has_no_scts() {
        let params = rcgen::CertificateParams::new(vec!["example.com".to_string()]).unwrap();
        let cert = params
            .self_signed(&rcgen::KeyPair::generate().unwrap())
            .unwrap();
        let der = cert.der().to_vec();
        assert!(extract_ct_info(&der).is_none());
    }

    #[test]
    fn check_status_none_is_warn() {
        assert_eq!(check_ct_status(None), CheckStatus::Warn);
    }

    #[test]
    fn check_status_zero_scts_is_warn() {
        let info = CtInfo {
            sct_count: 0,
            scts: vec![],
        };
        assert_eq!(check_ct_status(Some(&info)), CheckStatus::Warn);
    }

    #[test]
    fn check_status_one_sct_is_warn() {
        let info = CtInfo {
            sct_count: 1,
            scts: vec![SctEntry {
                version: 0,
                log_id: "aa".to_string(),
                timestamp: "2025-01-01T00:00:00Z".to_string(),
            }],
        };
        assert_eq!(check_ct_status(Some(&info)), CheckStatus::Warn);
    }

    #[test]
    fn check_status_two_scts_is_pass() {
        let info = CtInfo {
            sct_count: 2,
            scts: vec![
                SctEntry {
                    version: 0,
                    log_id: "aa".to_string(),
                    timestamp: "2025-01-01T00:00:00Z".to_string(),
                },
                SctEntry {
                    version: 0,
                    log_id: "bb".to_string(),
                    timestamp: "2025-01-01T00:00:00Z".to_string(),
                },
            ],
        };
        assert_eq!(check_ct_status(Some(&info)), CheckStatus::Pass);
    }

    #[test]
    fn format_timestamp_known_value() {
        // 2025-01-15T12:00:00.000Z = 1736942400000 ms
        let ts = format_sct_timestamp(1_736_942_400_000);
        assert!(ts.starts_with("2025-01-15T12:00:00"));
    }
}
