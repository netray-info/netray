use serde::Serialize;
use utoipa::ToSchema;

use crate::validate::CheckStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Certificate,
    Protocol,
    Configuration,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct HealthCheck {
    pub id: String,
    pub category: Category,
    pub status: CheckStatus,
    pub label: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct QualityResult {
    pub verdict: CheckStatus,
    pub checks: Vec<HealthCheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hsts: Option<HstsInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub https_redirect: Option<RedirectInfo>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PortQualityResult {
    pub verdict: CheckStatus,
    pub checks: Vec<HealthCheck>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct HstsInfo {
    pub present: bool,
    pub max_age: u64,
    pub include_sub_domains: bool,
    pub preload: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RedirectInfo {
    pub status: CheckStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_url: Option<String>,
}

pub fn compute_verdict(checks: &[HealthCheck]) -> CheckStatus {
    let mut worst = CheckStatus::Pass;
    for c in checks {
        match c.status {
            CheckStatus::Fail => return CheckStatus::Fail,
            CheckStatus::Warn => worst = CheckStatus::Warn,
            CheckStatus::Pass | CheckStatus::Skip => {}
        }
    }
    worst
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(status: CheckStatus) -> HealthCheck {
        HealthCheck {
            id: "test".to_string(),
            category: Category::Certificate,
            status,
            label: "test".to_string(),
            detail: "test".to_string(),
        }
    }

    #[test]
    fn verdict_all_pass() {
        assert_eq!(
            compute_verdict(&[check(CheckStatus::Pass), check(CheckStatus::Pass)]),
            CheckStatus::Pass
        );
    }

    #[test]
    fn verdict_skip_ignored() {
        assert_eq!(
            compute_verdict(&[check(CheckStatus::Pass), check(CheckStatus::Skip)]),
            CheckStatus::Pass
        );
    }

    #[test]
    fn verdict_warn() {
        assert_eq!(
            compute_verdict(&[check(CheckStatus::Pass), check(CheckStatus::Warn)]),
            CheckStatus::Warn
        );
    }

    #[test]
    fn verdict_fail_wins() {
        assert_eq!(
            compute_verdict(&[
                check(CheckStatus::Pass),
                check(CheckStatus::Warn),
                check(CheckStatus::Fail)
            ]),
            CheckStatus::Fail
        );
    }

    #[test]
    fn verdict_empty() {
        assert_eq!(compute_verdict(&[]), CheckStatus::Pass);
    }
}
