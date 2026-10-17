//! History over stored runs: scoring a run under a profile and diffing two runs.

use netray_model::diff::Diff;
use netray_model::run::{Run, SectionKind};
use netray_model::{CheckResult, Grade};

use crate::profile::Profile;
use crate::scoring::score;
use crate::{SectionEvent, SectionOutcome};

/// The grade of `run` under `profile`.
pub fn rescore(run: &Run, profile: &Profile) -> Grade {
    score_run(run, profile).grade
}

/// The score of `run` under `profile`. The sections and check statuses are rebuilt as the
/// engine produced them, without findings or evidence, which scoring does not read.
pub fn score_run(run: &Run, profile: &Profile) -> crate::scoring::Score {
    let events: Vec<SectionEvent> = run
        .sections
        .iter()
        .map(|section| {
            let outcome = match section.outcome {
                SectionKind::Measured => SectionOutcome::Measured {
                    checks: run
                        .checks
                        .iter()
                        .filter(|c| c.id.protocol() == section.protocol)
                        .map(|c| CheckResult {
                            id: c.id.clone(),
                            status: c.status,
                            findings: vec![],
                            evidence: vec![],
                        })
                        .collect(),
                    evidence: vec![],
                },
                SectionKind::NotApplicable => SectionOutcome::NotApplicable {
                    reason: section.reason.clone().unwrap_or_default(),
                },
                SectionKind::Incomplete => SectionOutcome::Incomplete {
                    reason: section.reason.clone().unwrap_or_default(),
                },
                SectionKind::TimedOut => SectionOutcome::TimedOut,
            };
            SectionEvent {
                protocol: section.protocol,
                outcome,
            }
        })
        .collect();
    score(profile, &events)
}

/// Compares `cur` with its predecessor `prev`. When the profile versions differ, `prev` is
/// also scored under `cur_profile` and the grade change carries that grade as `rescored`.
pub fn diff(prev: &Run, cur: &Run, volatile: &[&str], cur_profile: &Profile) -> Diff {
    netray_model::diff::diff(
        prev,
        cur,
        volatile,
        (prev.profile_version != cur.profile_version).then(|| rescore(prev, cur_profile)),
    )
}
