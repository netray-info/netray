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

    // RFC 8659 §4: an unknown critical tag means no CA is authorized
    if caa.records.iter().any(|r| r.issuer_critical) {
        return CheckStatus::Fail;
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

/// Match a cert issuer DN against a CAA `issue` domain value.
///
/// Looks up the CAA domain in the generated CA table (data/caa_domains.tsv)
/// and checks whether the issuer DN's O= or CN= field corresponds to that CA
/// name. Returns false for any CAA domain not present in the table.
fn issuer_domain_matches(issuer: &str, caa_domain: &str) -> bool {
    let Some(ca_name) = super::caa_issuers::lookup_caa_issuer(caa_domain) else {
        return false;
    };
    issuer_matches_ca_name(issuer, ca_name)
}

/// Check whether any O= or CN= field in the issuer DN corresponds to `ca_name`.
///
/// Uses two strategies:
/// - Bidirectional containment: one fully includes the other (handles cases
///   where the O= field is shorter or longer than the CA name, e.g.
///   "Amazon" ⊂ "Amazon Trust Services").
/// - Significant-word overlap: any word ≥6 chars from `ca_name` appears in
///   the normalized field value (handles "Cybertrust Japan / JCSI" vs
///   "Cybertrust Japan Co., Ltd.").
fn issuer_matches_ca_name(issuer: &str, ca_name: &str) -> bool {
    let ca_norm = normalize(ca_name);
    for part in issuer.split(',') {
        let part = part.trim();
        if !part.starts_with("O=") && !part.starts_with("CN=") {
            continue;
        }
        let value = part.split_once('=').map(|(_, v)| v).unwrap_or("");
        let field_norm = normalize(value);
        if field_norm.is_empty() {
            continue;
        }
        if ca_norm.contains(&field_norm) || field_norm.contains(&ca_norm) {
            return true;
        }
        // Word overlap for cases with diverging suffixes.
        if ca_name
            .split_whitespace()
            .map(normalize)
            .filter(|w| w.len() >= 6)
            .any(|w| field_norm.contains(&w))
        {
            return true;
        }
    }
    false
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

    #[test]
    fn google_trust_services_matches_pki_goog() {
        let caa = caa_with_issues(&["pki.goog"]);
        assert_eq!(
            check_caa_compliance(&caa, "CN=WE2, O=Google Trust Services, C=US"),
            CheckStatus::Pass
        );
    }

    #[test]
    fn unknown_caa_domain_is_fail() {
        // A CAA domain not in the table cannot be matched.
        let caa = caa_with_issues(&["unknown-ca-not-in-table.example"]);
        assert_eq!(
            check_caa_compliance(&caa, "O=Unknown CA"),
            CheckStatus::Fail
        );
    }

    #[test]
    fn amazon_issuer_matches_amazonaws_domain() {
        let caa = caa_with_issues(&["amazonaws.com"]);
        assert_eq!(
            check_caa_compliance(&caa, "C=US, O=Amazon, CN=Amazon RSA 2048 M04"),
            CheckStatus::Pass
        );
    }

    #[test]
    fn issuer_critical_blocks_all_issuance() {
        let caa = CaaLookup {
            records: vec![CaaRecord {
                tag: "issue".into(),
                value: "letsencrypt.org".into(),
                issuer_critical: true,
            }],
        };
        assert_eq!(
            check_caa_compliance(&caa, "CN=R3, O=Let's Encrypt, C=US"),
            CheckStatus::Fail
        );
    }

    // --- issuer_matches_ca_name ---

    #[test]
    fn exact_ca_name_match() {
        assert!(issuer_matches_ca_name(
            "CN=R3, O=Let's Encrypt, C=US",
            "Let's Encrypt"
        ));
    }

    #[test]
    fn ca_name_longer_than_o_field() {
        // "Amazon" ⊂ "Amazon Trust Services"
        assert!(issuer_matches_ca_name(
            "C=US, O=Amazon, CN=Amazon RSA 2048 M04",
            "Amazon Trust Services"
        ));
    }

    #[test]
    fn o_field_longer_than_ca_name() {
        // "DigiCert" ⊂ "DigiCert Inc"
        assert!(issuer_matches_ca_name("O=DigiCert Inc", "DigiCert"));
    }

    #[test]
    fn word_overlap_for_diverging_suffixes() {
        // "Cybertrust" (≥6 chars) appears in both
        assert!(issuer_matches_ca_name(
            "C=JP, O=Cybertrust Japan Co., Ltd.",
            "Cybertrust Japan / JCSI"
        ));
    }

    #[test]
    fn no_match_between_different_cas() {
        assert!(!issuer_matches_ca_name(
            "CN=R3, O=Let's Encrypt, C=US",
            "DigiCert"
        ));
    }
}
