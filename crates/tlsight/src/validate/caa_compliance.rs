//! CAA compliance checking.
//!
//! Checks whether the issuing CA is authorized by the domain's CAA records
//! per RFC 8659.

use crate::dns::CaaLookup;
use crate::validate::CheckStatus;

/// Check if the certificate issuer is authorized by CAA records.
///
/// - No CAA records → Pass (RFC 8659: absence is permissive)
/// - Records exist but no `issue` tags → Pass (no issuance constraint)
/// - Records with `issue` tags, issuer matches → Pass
/// - Records with `issue` tags, no match → Fail
pub fn check_caa_compliance(caa: &CaaLookup, leaf_issuer: &str) -> CheckStatus {
    if caa.is_empty() {
        return CheckStatus::Pass;
    }

    let issue_domains = caa.issue_domains();
    if issue_domains.is_empty() {
        return CheckStatus::Pass;
    }

    let matches = issue_domains.iter().any(|&domain| {
        if domain.is_empty() {
            return false; // Empty issue value = deny all issuance
        }
        issuer_domain_matches(leaf_issuer, domain)
    });

    if matches {
        CheckStatus::Pass
    } else {
        CheckStatus::Fail
    }
}

/// Heuristic match between a cert issuer DN field and a CAA domain.
///
/// The CAA `issue` value is a domain (e.g., "letsencrypt.org") while the cert
/// issuer is a Distinguished Name (e.g., "O=Let's Encrypt"). We check:
/// 1. Issuer DN contains the domain's second-level label ("digicert" in "digicert.com")
/// 2. Domain label starts with any word from the issuer DN ("amazonaws" starts with "amazon")
fn issuer_domain_matches(issuer: &str, caa_domain: &str) -> bool {
    let issuer_norm = normalize(issuer);
    // Use the second-level domain label: "letsencrypt.org" → "letsencrypt"
    let domain_label = caa_domain.split('.').next().unwrap_or(caa_domain);
    let domain_norm = normalize(domain_label);

    if domain_norm.is_empty() {
        return false;
    }

    // Direct: issuer DN contains domain label
    if issuer_norm.contains(&domain_norm) {
        return true;
    }

    // Reverse: domain label starts with an issuer DN word.
    // Handles "amazonaws.com" matching issuer "O=Amazon" — "amazonaws" starts with "amazon".
    issuer_words(issuer).any(|word| word.len() >= 4 && domain_norm.starts_with(&word))
}

/// Extract normalized words (len >= 4) from an issuer DN's O= and CN= fields.
fn issuer_words(issuer: &str) -> impl Iterator<Item = String> + '_ {
    issuer
        .split(',')
        .flat_map(|part| {
            let part = part.trim();
            if part.starts_with("O=") || part.starts_with("CN=") {
                let value = part.split_once('=').map(|(_, v)| v).unwrap_or("");
                Some(value.split_whitespace().map(normalize).collect::<Vec<_>>())
            } else {
                None
            }
        })
        .flatten()
}

/// Normalize a string for comparison: lowercase, keep only alphanumeric.
fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns::caa::CaaRecord;

    fn caa_with_issues(issues: &[&str]) -> CaaLookup {
        CaaLookup {
            records: issues
                .iter()
                .map(|v| CaaRecord {
                    tag: "issue".into(),
                    value: v.to_string(),
                    issuer_critical: false,
                })
                .collect(),
        }
    }

    // --- check_caa_compliance ---

    #[test]
    fn no_records_is_pass() {
        let caa = CaaLookup { records: vec![] };
        assert_eq!(
            check_caa_compliance(&caa, "O=Let's Encrypt"),
            CheckStatus::Pass
        );
    }

    #[test]
    fn no_issue_tags_is_pass() {
        let caa = CaaLookup {
            records: vec![CaaRecord {
                tag: "iodef".into(),
                value: "mailto:admin@example.com".into(),
                issuer_critical: false,
            }],
        };
        assert_eq!(
            check_caa_compliance(&caa, "O=Let's Encrypt"),
            CheckStatus::Pass
        );
    }

    #[test]
    fn matching_issuer_is_pass() {
        let caa = caa_with_issues(&["letsencrypt.org"]);
        assert_eq!(
            check_caa_compliance(&caa, "CN=R3, O=Let's Encrypt, C=US"),
            CheckStatus::Pass
        );
    }

    #[test]
    fn non_matching_issuer_is_fail() {
        let caa = caa_with_issues(&["letsencrypt.org"]);
        assert_eq!(
            check_caa_compliance(&caa, "CN=DigiCert Global Root G2, O=DigiCert Inc"),
            CheckStatus::Fail
        );
    }

    #[test]
    fn multiple_issuers_any_match() {
        let caa = caa_with_issues(&["letsencrypt.org", "digicert.com"]);
        assert_eq!(
            check_caa_compliance(&caa, "CN=DigiCert SHA2, O=DigiCert Inc"),
            CheckStatus::Pass
        );
    }

    #[test]
    fn empty_issue_value_denies() {
        // CAA with issue ";" means deny all
        let caa = CaaLookup {
            records: vec![CaaRecord {
                tag: "issue".into(),
                value: ";".into(),
                issuer_critical: false,
            }],
        };
        assert_eq!(check_caa_compliance(&caa, "O=Any CA"), CheckStatus::Fail);
    }

    #[test]
    fn issue_with_parameters_matches() {
        let caa = caa_with_issues(&["digicert.com; cansignhttpexchanges=yes"]);
        assert_eq!(
            check_caa_compliance(&caa, "O=DigiCert Inc"),
            CheckStatus::Pass
        );
    }

    #[test]
    fn sectigo_matches() {
        let caa = caa_with_issues(&["sectigo.com"]);
        assert_eq!(
            check_caa_compliance(&caa, "O=Sectigo Limited"),
            CheckStatus::Pass
        );
    }

    // --- issuer_domain_matches ---

    #[test]
    fn letsencrypt_match() {
        assert!(issuer_domain_matches("O=Let's Encrypt", "letsencrypt.org"));
    }

    #[test]
    fn digicert_match() {
        assert!(issuer_domain_matches("O=DigiCert Inc", "digicert.com"));
    }

    #[test]
    fn case_insensitive() {
        assert!(issuer_domain_matches("O=DIGICERT INC", "DigiCert.com"));
    }

    #[test]
    fn no_match() {
        assert!(!issuer_domain_matches("O=Let's Encrypt", "digicert.com"));
    }

    #[test]
    fn amazonaws_matches_amazon_issuer() {
        assert!(issuer_domain_matches(
            "C=US, O=Amazon, CN=Amazon RSA 2048 M04",
            "amazonaws.com"
        ));
    }

    #[test]
    fn amazon_issuer_matches_amazon_domain() {
        let caa = caa_with_issues(&["amazonaws.com"]);
        assert_eq!(
            check_caa_compliance(&caa, "C=US, O=Amazon, CN=Amazon RSA 2048 M04"),
            CheckStatus::Pass
        );
    }

    #[test]
    fn short_issuer_word_not_matched() {
        // "US" is too short (< 4 chars) to match via reverse heuristic
        assert!(!issuer_domain_matches("C=US, O=AB", "uscdn.com"));
    }

    #[test]
    fn empty_domain_no_match() {
        assert!(!issuer_domain_matches("O=Any", ""));
    }
}
