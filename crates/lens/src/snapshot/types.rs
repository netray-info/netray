use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::backends::BackendResult;
use crate::check::SectionError;
use crate::scoring::engine::{CheckVerdict, OverallScore};
use super::store::MAX_FINDINGS_PER_SECTION;

/// One section's data passed to `Snapshot::from_check_output`.
pub struct SectionResult {
    pub name: String,
    pub display_name: String,
    pub grade: String,
    pub backend_result: Result<BackendResult, SectionError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub shortid: String,
    pub domain: String,
    pub grade: String,
    pub sections: Vec<SnapshotSection>,
    pub created_at: DateTime<Utc>,
    pub lens_version: String,
}

impl Snapshot {
    pub fn from_check_output(
        domain: String,
        shortid: String,
        score: &OverallScore,
        sections: &[SectionResult],
        lens_version: &str,
    ) -> Self {
        let snap_sections = sections
            .iter()
            .map(|sr| {
                if sr.grade == "error" {
                    SnapshotSection {
                        name: sr.display_name.clone(),
                        grade: sr.grade.clone(),
                        passes: 0,
                        warns: 0,
                        fails: 0,
                        findings: vec![SnapshotFinding {
                            check_name: "backend".to_string(),
                            verdict: "fail".to_string(),
                            message: "Section check failed — no details available".to_string(),
                            fix_hint: None,
                            fix_owner: None,
                            guide_url: None,
                        }],
                    }
                } else {
                    let checks = match &sr.backend_result {
                        Ok(r) => r.checks.as_slice(),
                        Err(_) => &[],
                    };
                    let findings: Vec<SnapshotFinding> = checks
                        .iter()
                        .map(|c| {
                            let message = c.messages.first().cloned().unwrap_or_default();
                            SnapshotFinding {
                                check_name: c.name.clone(),
                                verdict: verdict_str(&c.verdict).to_string(),
                                message,
                                fix_hint: None,
                                fix_owner: None,
                                guide_url: None,
                            }
                        })
                        .take(MAX_FINDINGS_PER_SECTION)
                        .collect();
                    let passes = findings.iter().filter(|f| f.verdict == "pass").count() as u32;
                    let warns = findings.iter().filter(|f| f.verdict == "warn").count() as u32;
                    let fails = findings
                        .iter()
                        .filter(|f| f.verdict == "fail")
                        .count() as u32;
                    SnapshotSection {
                        name: sr.display_name.clone(),
                        grade: sr.grade.clone(),
                        passes,
                        warns,
                        fails,
                        findings,
                    }
                }
            })
            .collect();

        Snapshot {
            shortid,
            domain,
            grade: score.grade.clone(),
            sections: snap_sections,
            created_at: Utc::now(),
            lens_version: lens_version.to_string(),
        }
    }
}

fn verdict_str(v: &CheckVerdict) -> &'static str {
    match v {
        CheckVerdict::Pass => "pass",
        CheckVerdict::Warn => "warn",
        CheckVerdict::Fail | CheckVerdict::NotFound => "fail",
        CheckVerdict::Skip => "skip",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotSection {
    pub name: String,
    pub grade: String,
    pub passes: u32,
    pub warns: u32,
    pub fails: u32,
    pub findings: Vec<SnapshotFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotFinding {
    pub check_name: String,
    pub verdict: String,
    pub message: String,
    pub fix_hint: Option<String>,
    pub fix_owner: Option<String>,
    pub guide_url: Option<String>,
}
