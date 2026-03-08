pub mod caa_compliance;
pub mod chain_trust;
pub mod ct;
#[allow(dead_code)] // Fully tested, not called at runtime until DNSSEC is available
pub mod dane;

use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
    Skip,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Summary {
    pub verdict: CheckStatus,
    pub checks: SummaryChecks,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SummaryChecks {
    pub chain_trusted: CheckStatus,
    pub not_expired: CheckStatus,
    pub hostname_match: CheckStatus,
    pub caa_compliant: CheckStatus,
    pub dane_valid: CheckStatus,
    pub ct_logged: CheckStatus,
    pub ocsp_stapled: CheckStatus,
    pub consistency: CheckStatus,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ValidationResult {
    pub chain_trusted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_trust_reason: Option<String>,
    pub terminates_at_self_signed: bool,
    pub chain_order_correct: bool,
    pub leaf_covers_hostname: bool,
    pub any_expired: bool,
    pub any_not_yet_valid: bool,
    pub weakest_signature: String,
    pub earliest_expiry: String,
    pub earliest_expiry_days: i64,
}

/// Compute the summary verdict from validation results.
pub fn summarize(
    validation: Option<&ValidationResult>,
    hostname: Option<&str>,
    ocsp_stapled: bool,
    has_consistency_mismatch: bool,
    caa_status: CheckStatus,
    dane_status: CheckStatus,
    ct_status: CheckStatus,
) -> Summary {
    let (chain_trusted, not_expired, hostname_match) = match validation {
        Some(v) => {
            let chain = if v.chain_trusted {
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            };
            let expired = if v.any_expired || v.any_not_yet_valid {
                CheckStatus::Fail
            } else {
                CheckStatus::Pass
            };
            let hostname = if hostname.is_some() {
                if v.leaf_covers_hostname {
                    CheckStatus::Pass
                } else {
                    CheckStatus::Fail
                }
            } else {
                CheckStatus::Skip
            };
            (chain, expired, hostname)
        }
        None => (CheckStatus::Skip, CheckStatus::Skip, CheckStatus::Skip),
    };

    let ocsp = if ocsp_stapled {
        CheckStatus::Pass
    } else {
        CheckStatus::Warn
    };

    let consistency = if has_consistency_mismatch {
        CheckStatus::Warn
    } else {
        CheckStatus::Pass
    };

    let checks = SummaryChecks {
        chain_trusted,
        not_expired,
        hostname_match,
        caa_compliant: caa_status,
        dane_valid: dane_status,
        ct_logged: ct_status,
        ocsp_stapled: ocsp,
        consistency,
    };

    let verdict = worst_status(&[
        checks.chain_trusted,
        checks.not_expired,
        checks.hostname_match,
        checks.caa_compliant,
        checks.ct_logged,
        checks.ocsp_stapled,
        checks.consistency,
    ]);

    Summary { verdict, checks }
}

fn worst_status(statuses: &[CheckStatus]) -> CheckStatus {
    if statuses.contains(&CheckStatus::Fail) {
        CheckStatus::Fail
    } else if statuses.contains(&CheckStatus::Warn) {
        CheckStatus::Warn
    } else {
        CheckStatus::Pass
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

    /// Helper: summarize with CAA/DANE/CT defaulting to Skip.
    fn summarize_skip(
        validation: Option<&ValidationResult>,
        hostname: Option<&str>,
        ocsp_stapled: bool,
        has_consistency_mismatch: bool,
    ) -> Summary {
        summarize(
            validation,
            hostname,
            ocsp_stapled,
            has_consistency_mismatch,
            CheckStatus::Skip,
            CheckStatus::Skip,
            CheckStatus::Skip,
        )
    }

    #[test]
    fn all_pass_verdict_is_pass() {
        let summary = summarize_skip(
            Some(&passing_validation()),
            Some("example.com"),
            true,
            false,
        );
        assert_eq!(summary.verdict, CheckStatus::Pass);
    }

    #[test]
    fn expired_cert_verdict_is_fail() {
        let mut v = passing_validation();
        v.any_expired = true;
        let summary = summarize_skip(Some(&v), Some("example.com"), true, false);
        assert_eq!(summary.verdict, CheckStatus::Fail);
        assert_eq!(summary.checks.not_expired, CheckStatus::Fail);
    }

    #[test]
    fn not_yet_valid_cert_verdict_is_fail() {
        let mut v = passing_validation();
        v.any_not_yet_valid = true;
        let summary = summarize_skip(Some(&v), Some("example.com"), true, false);
        assert_eq!(summary.verdict, CheckStatus::Fail);
        assert_eq!(summary.checks.not_expired, CheckStatus::Fail);
    }

    #[test]
    fn untrusted_chain_verdict_is_fail() {
        let mut v = passing_validation();
        v.chain_trusted = false;
        let summary = summarize_skip(Some(&v), Some("example.com"), true, false);
        assert_eq!(summary.verdict, CheckStatus::Fail);
        assert_eq!(summary.checks.chain_trusted, CheckStatus::Fail);
    }

    #[test]
    fn hostname_mismatch_verdict_is_fail() {
        let mut v = passing_validation();
        v.leaf_covers_hostname = false;
        let summary = summarize_skip(Some(&v), Some("example.com"), true, false);
        assert_eq!(summary.verdict, CheckStatus::Fail);
        assert_eq!(summary.checks.hostname_match, CheckStatus::Fail);
    }

    #[test]
    fn no_ocsp_staple_verdict_is_warn() {
        let summary = summarize_skip(
            Some(&passing_validation()),
            Some("example.com"),
            false,
            false,
        );
        assert_eq!(summary.verdict, CheckStatus::Warn);
        assert_eq!(summary.checks.ocsp_stapled, CheckStatus::Warn);
    }

    #[test]
    fn ip_input_skips_hostname_check() {
        let summary = summarize_skip(Some(&passing_validation()), None, true, false);
        assert_eq!(summary.checks.hostname_match, CheckStatus::Skip);
        // Skip does not affect verdict
        assert_eq!(summary.verdict, CheckStatus::Pass);
    }

    #[test]
    fn no_validation_result_skips_all_chain_checks() {
        let summary = summarize_skip(None, Some("example.com"), true, false);
        assert_eq!(summary.checks.chain_trusted, CheckStatus::Skip);
        assert_eq!(summary.checks.not_expired, CheckStatus::Skip);
        assert_eq!(summary.checks.hostname_match, CheckStatus::Skip);
    }

    #[test]
    fn deferred_checks_are_skip() {
        let summary = summarize_skip(
            Some(&passing_validation()),
            Some("example.com"),
            true,
            false,
        );
        assert_eq!(summary.checks.caa_compliant, CheckStatus::Skip);
        assert_eq!(summary.checks.dane_valid, CheckStatus::Skip);
    }

    #[test]
    fn caa_fail_affects_verdict() {
        let summary = summarize(
            Some(&passing_validation()),
            Some("example.com"),
            true,
            false,
            CheckStatus::Fail,
            CheckStatus::Skip,
            CheckStatus::Skip,
        );
        assert_eq!(summary.checks.caa_compliant, CheckStatus::Fail);
        assert_eq!(summary.verdict, CheckStatus::Fail);
    }

    #[test]
    fn caa_pass_does_not_degrade_verdict() {
        let summary = summarize(
            Some(&passing_validation()),
            Some("example.com"),
            true,
            false,
            CheckStatus::Pass,
            CheckStatus::Skip,
            CheckStatus::Skip,
        );
        assert_eq!(summary.checks.caa_compliant, CheckStatus::Pass);
        assert_eq!(summary.verdict, CheckStatus::Pass);
    }

    #[test]
    fn consistency_pass_when_no_mismatch() {
        let summary = summarize_skip(
            Some(&passing_validation()),
            Some("example.com"),
            true,
            false,
        );
        assert_eq!(summary.checks.consistency, CheckStatus::Pass);
    }

    #[test]
    fn consistency_warn_when_mismatch() {
        let summary = summarize_skip(Some(&passing_validation()), Some("example.com"), true, true);
        assert_eq!(summary.checks.consistency, CheckStatus::Warn);
        assert_eq!(summary.verdict, CheckStatus::Warn);
    }

    #[test]
    fn fail_beats_warn() {
        let mut v = passing_validation();
        v.any_expired = true;
        // Both fail (expired) and warn (no OCSP) present; verdict should be fail
        let summary = summarize_skip(Some(&v), Some("example.com"), false, false);
        assert_eq!(summary.verdict, CheckStatus::Fail);
    }

    #[test]
    fn worst_status_all_pass() {
        assert_eq!(
            worst_status(&[CheckStatus::Pass, CheckStatus::Pass]),
            CheckStatus::Pass
        );
    }

    #[test]
    fn worst_status_with_skip() {
        // Skip is not Fail or Warn, so treated as Pass
        assert_eq!(
            worst_status(&[CheckStatus::Pass, CheckStatus::Skip]),
            CheckStatus::Pass
        );
    }

    #[test]
    fn worst_status_empty() {
        assert_eq!(worst_status(&[]), CheckStatus::Pass);
    }
}
