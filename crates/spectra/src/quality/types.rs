use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Skip,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct QualityCheck {
    pub name: String,
    pub status: CheckStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct QualityReport {
    pub verdict: CheckStatus,
    pub checks: Vec<QualityCheck>,
}

impl QualityReport {
    pub fn from_checks(checks: Vec<QualityCheck>) -> Self {
        let verdict = checks
            .iter()
            .map(|c| &c.status)
            .max()
            .cloned()
            .unwrap_or(CheckStatus::Pass);
        Self { verdict, checks }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_is_max_status() {
        let checks = vec![
            QualityCheck {
                name: "a".into(),
                status: CheckStatus::Pass,
                message: None,
            },
            QualityCheck {
                name: "b".into(),
                status: CheckStatus::Warn,
                message: None,
            },
        ];
        let report = QualityReport::from_checks(checks);
        assert_eq!(report.verdict, CheckStatus::Warn);
    }

    #[test]
    fn empty_checks_is_pass() {
        let report = QualityReport::from_checks(vec![]);
        assert_eq!(report.verdict, CheckStatus::Pass);
    }

    #[test]
    fn fail_overrides_warn() {
        let checks = vec![
            QualityCheck {
                name: "a".into(),
                status: CheckStatus::Warn,
                message: None,
            },
            QualityCheck {
                name: "b".into(),
                status: CheckStatus::Fail,
                message: None,
            },
        ];
        let report = QualityReport::from_checks(checks);
        assert_eq!(report.verdict, CheckStatus::Fail);
    }

    #[test]
    fn status_ordering() {
        assert!(CheckStatus::Pass < CheckStatus::Skip);
        assert!(CheckStatus::Skip < CheckStatus::Warn);
        assert!(CheckStatus::Warn < CheckStatus::Fail);
    }
}
