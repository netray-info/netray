use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::store::MAX_FINDINGS_PER_SECTION;
use crate::backends::{BackendExtra, BackendResult};
use crate::check::SectionError;
use crate::scoring::engine::{CheckVerdict, OverallScore};
use crate::scoring::profile::ScoringProfile;

/// One section's data passed to `Snapshot::from_check_output`.
pub struct SectionResult {
    pub name: String,
    pub display_name: String,
    pub grade: String,
    pub backend_result: Result<BackendResult, SectionError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotAddress {
    pub role: String,
    pub ip: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub shortid: String,
    pub domain: String,
    pub grade: String,
    #[serde(default)]
    pub score: f64,
    pub sections: Vec<SnapshotSection>,
    #[serde(default)]
    pub server_addresses: Vec<SnapshotAddress>,
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
        profile: &ScoringProfile,
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
                        skips: 0,
                        findings: vec![SnapshotFinding {
                            check_name: "backend".to_string(),
                            verdict: "fail".to_string(),
                            message: "Section check failed — no details available".to_string(),
                            earned: 0,
                            possible: 0,
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
                    let section_weights = profile.sections.get(&sr.name).map(|s| &s.checks);
                    let findings: Vec<SnapshotFinding> = checks
                        .iter()
                        .map(|c| {
                            let weight = section_weights
                                .and_then(|w| w.get(&c.name))
                                .copied()
                                .unwrap_or(0);
                            let (earned, possible) = match c.verdict {
                                CheckVerdict::Pass => (weight, weight),
                                CheckVerdict::Warn => (weight / 2, weight),
                                CheckVerdict::Fail | CheckVerdict::NotFound => (0, weight),
                                CheckVerdict::Skip => (0, 0),
                            };
                            SnapshotFinding {
                                check_name: c.name.clone(),
                                verdict: verdict_str(&c.verdict).to_string(),
                                message: c.messages.first().cloned().unwrap_or_default(),
                                earned,
                                possible,
                                fix_hint: None,
                                fix_owner: None,
                                guide_url: None,
                            }
                        })
                        .take(MAX_FINDINGS_PER_SECTION)
                        .collect();
                    let passes = findings.iter().filter(|f| f.verdict == "pass").count() as u32;
                    let warns = findings.iter().filter(|f| f.verdict == "warn").count() as u32;
                    let fails = findings.iter().filter(|f| f.verdict == "fail").count() as u32;
                    let skips = findings.iter().filter(|f| f.verdict == "skip").count() as u32;
                    SnapshotSection {
                        name: sr.display_name.clone(),
                        grade: sr.grade.clone(),
                        passes,
                        warns,
                        fails,
                        skips,
                        findings,
                    }
                }
            })
            .collect();

        // Extract server addresses from HTTP (server_ip) and IP (resolved addresses)
        let mut server_addresses: Vec<SnapshotAddress> = Vec::new();
        for sr in sections {
            if let Ok(br) = &sr.backend_result {
                match &br.extra {
                    BackendExtra::Http {
                        server_ip: Some(ip),
                        server_org,
                        ..
                    } => {
                        server_addresses.push(SnapshotAddress {
                            role: "HTTP".to_string(),
                            ip: ip.clone(),
                            org: server_org.clone(),
                        });
                    }
                    BackendExtra::Ip { addresses, .. } => {
                        for addr in addresses {
                            server_addresses.push(SnapshotAddress {
                                role: "IP".to_string(),
                                ip: addr.ip.to_string(),
                                org: addr.org.clone(),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        Snapshot {
            shortid,
            domain,
            grade: score.grade.clone(),
            score: score.overall_percentage,
            sections: snap_sections,
            server_addresses,
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
    #[serde(default)]
    pub skips: u32,
    pub findings: Vec<SnapshotFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotFinding {
    pub check_name: String,
    pub verdict: String,
    pub message: String,
    #[serde(default)]
    pub earned: u32,
    #[serde(default)]
    pub possible: u32,
    pub fix_hint: Option<String>,
    pub fix_owner: Option<String>,
    pub guide_url: Option<String>,
}
