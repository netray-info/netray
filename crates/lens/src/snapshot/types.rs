use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::store::MAX_FINDINGS_PER_SECTION;
use crate::check::SectionError;
use crate::modules::{BackendResult, HttpServer, ip_addresses};
use crate::score::{CheckVerdict, OverallScore, section_weights};
use netray_engine::profile::Profile;

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
        profile: &Profile,
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
                            earned: 0.0,
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
                    let weights = section_weights(profile, &sr.name);
                    let findings: Vec<SnapshotFinding> = checks
                        .iter()
                        .map(|c| {
                            let weight = weights.get(&c.name).copied().unwrap_or(0);
                            let (earned, possible) = match c.verdict {
                                CheckVerdict::Pass => (weight as f64, weight),
                                CheckVerdict::Warn => (weight as f64 / 2.0, weight),
                                CheckVerdict::Fail | CheckVerdict::NotFound => (0.0, weight),
                                CheckVerdict::Skip | CheckVerdict::Unmeasured => (0.0, 0),
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

        // Server addresses from the evidence: the HTTP server and the IP section's addresses.
        let mut server_addresses: Vec<SnapshotAddress> = Vec::new();
        for sr in sections {
            if let Ok(br) = &sr.backend_result {
                match sr.name.as_str() {
                    "http" => {
                        let server = HttpServer::from_evidence(&br.evidence);
                        if let Some(ip) = server.address {
                            server_addresses.push(SnapshotAddress {
                                role: "HTTP".to_string(),
                                ip,
                                org: server.org,
                            });
                        }
                    }
                    "ip" => {
                        for addr in ip_addresses(&br.evidence) {
                            server_addresses.push(SnapshotAddress {
                                role: "IP".to_string(),
                                ip: addr.ip.to_string(),
                                org: addr.org,
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
        CheckVerdict::Unmeasured => "unmeasured",
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
    pub earned: f64,
    #[serde(default)]
    pub possible: u32,
    pub fix_hint: Option<String>,
    pub fix_owner: Option<String>,
    pub guide_url: Option<String>,
}
