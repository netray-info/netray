//! V1's scoring rules over the engine's section events: Pass earns the weight, Warn half, Fail
//! nothing; excluded statuses count neither way; hard-fail checks force `F`.

use std::collections::BTreeMap;

use netray_model::{CheckId, CheckResult, Grade, Protocol, Status};

use crate::profile::Profile;
use crate::{SectionEvent, SectionOutcome};

/// One scored section.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionScore {
    pub earned: f64,
    pub possible: u32,
    /// 0.0–100.0.
    pub percentage: f64,
}

/// The score of one run.
#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    /// The scored sections; missing are sections not run, not applicable, incomplete, timed out
    /// or with nothing weighted to score.
    pub sections: BTreeMap<Protocol, SectionScore>,
    /// Weighted mean of the scored sections; informational when the score is incomplete.
    pub overall_percentage: f64,
    pub grade: Grade,
    /// False when a section is incomplete, timed out or measured with possible 0.
    pub complete: bool,
    pub hard_fail_checks: Vec<CheckId>,
    /// Not-applicable sections and their reasons.
    pub not_applicable: BTreeMap<Protocol, String>,
}

/// `None` when nothing weighted was scored: such a section carries no signal.
fn score_section(weights: &BTreeMap<CheckId, u32>, checks: &[CheckResult]) -> Option<SectionScore> {
    let mut earned = 0.0;
    let mut possible = 0;
    for check in checks {
        let Some(&weight) = weights.get(&check.id) else {
            continue;
        };
        match check.status {
            Status::Pass => {
                earned += weight as f64;
                possible += weight;
            }
            Status::Warn => {
                earned += weight as f64 / 2.0;
                possible += weight;
            }
            Status::Fail => possible += weight,
            Status::NotApplicable | Status::NotTested | Status::Unmeasured => {}
        }
    }
    if possible == 0 {
        return None;
    }
    Some(SectionScore {
        earned,
        possible,
        percentage: earned / possible as f64 * 100.0,
    })
}

fn lookup_grade(thresholds: &[(Grade, u32)], percentage: f64) -> Grade {
    thresholds
        .iter()
        .find(|(_, t)| percentage >= *t as f64)
        .map_or(Grade::F, |(g, _)| *g)
}

/// Scores the finished sections against `profile`. A profile section without an event is
/// ignored; no scored section at all is `Incomplete`.
pub fn score(profile: &Profile, sections: &[SectionEvent]) -> Score {
    let mut scored = BTreeMap::new();
    let mut not_applicable = BTreeMap::new();
    let mut hard_fail_checks = Vec::new();
    let mut weighted_sum = 0.0;
    let mut total_weight = 0;
    let mut complete = true;

    for (protocol, weights) in &profile.sections {
        let Some(event) = sections.iter().find(|e| e.protocol == *protocol) else {
            continue;
        };
        match &event.outcome {
            SectionOutcome::NotApplicable { reason } => {
                not_applicable.insert(*protocol, reason.clone());
            }
            SectionOutcome::Incomplete { .. } | SectionOutcome::TimedOut => complete = false,
            SectionOutcome::Measured { checks, .. } => {
                for hf in &weights.hard_fail {
                    if checks
                        .iter()
                        .any(|c| c.id == *hf && c.status == Status::Fail)
                    {
                        hard_fail_checks.push(hf.clone());
                    }
                }
                match score_section(&weights.checks, checks) {
                    Some(s) => {
                        weighted_sum += s.percentage * weights.weight as f64;
                        total_weight += weights.weight;
                        scored.insert(*protocol, s);
                    }
                    None => complete = false,
                }
            }
        }
    }

    if total_weight == 0 {
        return Score {
            sections: scored,
            overall_percentage: 0.0,
            grade: Grade::Incomplete,
            complete: false,
            hard_fail_checks: vec![],
            not_applicable,
        };
    }

    let overall_percentage = weighted_sum / total_weight as f64;
    let grade = if !complete {
        Grade::Incomplete
    } else if !hard_fail_checks.is_empty() {
        Grade::F
    } else {
        lookup_grade(&profile.thresholds, overall_percentage)
    };

    Score {
        sections: scored,
        overall_percentage,
        grade,
        complete,
        hard_fail_checks,
        not_applicable,
    }
}
