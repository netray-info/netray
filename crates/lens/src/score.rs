//! lens's view of the engine's scoring: section results scored by `netray_engine::score` and
//! mapped back onto lens's section names, grade labels and bare check names; the per-check texts
//! come from `netray_engine::catalogue()`.

use std::collections::HashMap;

use netray_engine::catalogue::CheckSpec;
use netray_engine::profile::Profile;
use netray_engine::{SectionEvent, SectionOutcome};
use netray_model::{CheckId, FixOwner, Grade, Protocol};

use crate::check::SectionError;
use crate::modules::BackendResult;

pub use netray_engine::scoring::SectionScore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckVerdict {
    Pass,
    Warn,
    Fail,
    NotFound,
    /// Excluded from both earned and possible totals.
    Skip,
    /// Could not be checked; makes its section incomplete.
    Unmeasured,
}

/// Maps onto the shared model vocabulary; `NotFound` is `Fail`, `Skip` is `NotApplicable`,
/// `Unmeasured` is `Unmeasured`.
impl From<CheckVerdict> for netray_model::Status {
    fn from(verdict: CheckVerdict) -> Self {
        match verdict {
            CheckVerdict::Pass => netray_model::Status::Pass,
            CheckVerdict::Warn => netray_model::Status::Warn,
            CheckVerdict::Fail => netray_model::Status::Fail,
            CheckVerdict::NotFound => netray_model::Status::Fail,
            CheckVerdict::Skip => netray_model::Status::NotApplicable,
            CheckVerdict::Unmeasured => netray_model::Status::Unmeasured,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CheckResult {
    pub name: String,
    pub verdict: CheckVerdict,
    /// Diagnostic messages explaining a non-passing verdict (warn/fail/not-found), or the reason
    /// of a not-applicable check.
    pub messages: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct OverallScore {
    /// Per-section scores of the finished sections. Missing key = section errored, not
    /// applicable, scored with `possible == 0`, or absent from profile.
    pub sections: HashMap<String, SectionScore>,
    /// Weighted average of the finished sections. Informational when the result is incomplete.
    pub overall_percentage: f64,
    /// A letter grade, or `"incomplete"` when `complete` is false.
    pub grade: String,
    /// False when any section is Errored, Scored with `possible == 0`, or has a weighted check
    /// that is Unmeasured. NotApplicable sections are excluded without making the result
    /// incomplete. A failed hard-fail check still gives grade `F` when incomplete.
    pub complete: bool,
    pub hard_fail_triggered: bool,
    /// Which specific checks triggered a hard fail, by bare name.
    pub hard_fail_checks: Vec<String>,
    /// Section name → reason. Always present (may be empty). Populated only for NotApplicable.
    pub not_applicable: HashMap<String, String>,
}

const PROTOCOLS: [Protocol; 5] = [
    Protocol::Dns,
    Protocol::Tls,
    Protocol::Http,
    Protocol::Email,
    Protocol::Ip,
];

fn protocol_for(section: &str) -> Option<Protocol> {
    PROTOCOLS.into_iter().find(|p| p.as_str() == section)
}

pub fn grade_label(grade: Grade) -> &'static str {
    match grade {
        Grade::APlus => "A+",
        Grade::A => "A",
        Grade::B => "B",
        Grade::C => "C",
        Grade::D => "D",
        Grade::F => "F",
        Grade::Incomplete => "incomplete",
    }
}

/// Parse a grade label into the shared model grade.
pub fn parse_grade(s: &str) -> Option<Grade> {
    match s {
        "A+" => Some(Grade::APlus),
        "A" => Some(Grade::A),
        "B" => Some(Grade::B),
        "C" => Some(Grade::C),
        "D" => Some(Grade::D),
        "F" => Some(Grade::F),
        "incomplete" => Some(Grade::Incomplete),
        _ => None,
    }
}

/// The highest grade of `profile` whose threshold is at most `percentage`; `"F"` below all.
pub fn grade_for(profile: &Profile, percentage: f64) -> String {
    profile
        .thresholds
        .iter()
        .find(|(_, t)| percentage >= *t as f64)
        .map_or("F", |(g, _)| grade_label(*g))
        .to_string()
}

/// The weights of one section of `profile`, by bare check name; empty for an unknown section.
pub fn section_weights(profile: &Profile, section: &str) -> HashMap<String, u32> {
    protocol_for(section)
        .and_then(|p| profile.sections.get(&p))
        .map(|s| {
            s.checks
                .iter()
                .map(|(id, w)| (id.name().to_string(), *w))
                .collect()
        })
        .unwrap_or_default()
}

/// The catalogue entry of a bare check name (bare names are unique across the catalogue).
pub fn spec_by_name(name: &str) -> Option<&'static CheckSpec> {
    netray_engine::catalogue()
        .entries()
        .iter()
        .find(|s| s.id.name() == name)
}

pub fn guide_url(spec: &CheckSpec) -> String {
    format!("https://netray.info{}", spec.guide)
}

pub fn fix_owner_label(owner: FixOwner) -> &'static str {
    match owner {
        FixOwner::DnsProvider => "your DNS provider",
        FixOwner::Registrar => "your registrar",
        FixOwner::CertificateProvider => "your certificate provider",
        FixOwner::WebServer => "your web server configuration",
        FixOwner::MailProvider => "your mail provider",
        FixOwner::Hosting => "your hosting provider",
    }
}

fn event(section: &str, result: &Result<BackendResult, SectionError>) -> Option<SectionEvent> {
    let protocol = protocol_for(section)?;
    let outcome = match result {
        Ok(r) => SectionOutcome::Measured {
            checks: r
                .checks
                .iter()
                .filter_map(|c| {
                    Some(netray_model::CheckResult {
                        id: CheckId::parse(&format!("{section}.{}", c.name)).ok()?,
                        status: c.verdict.clone().into(),
                        findings: vec![],
                        evidence: vec![],
                    })
                })
                .collect(),
            presentation: serde_json::Value::Null,
            evidence: vec![],
        },
        Err(SectionError::NotApplicable { reason }) => SectionOutcome::NotApplicable {
            reason: reason.clone(),
        },
        Err(e) => SectionOutcome::Incomplete {
            reason: format!("{e:?}"),
        },
    };
    Some(SectionEvent { protocol, outcome })
}

/// Scores lens's section results with `profile` through `netray_engine::score`.
pub fn score(
    profile: &Profile,
    sections: &HashMap<String, Result<BackendResult, SectionError>>,
) -> OverallScore {
    let events: Vec<SectionEvent> = sections
        .iter()
        .filter_map(|(name, result)| event(name, result))
        .collect();
    let s = netray_engine::score(profile, &events);
    let hard_fail_checks: Vec<String> = s
        .hard_fail_checks
        .iter()
        .map(|id| id.name().to_string())
        .collect();
    OverallScore {
        sections: s
            .sections
            .into_iter()
            .map(|(p, sc)| (p.as_str().to_string(), sc))
            .collect(),
        overall_percentage: s.overall_percentage,
        grade: grade_label(s.grade).to_string(),
        complete: s.complete,
        hard_fail_triggered: !hard_fail_checks.is_empty(),
        hard_fail_checks,
        not_applicable: s
            .not_applicable
            .into_iter()
            .map(|(p, r)| (p.as_str().to_string(), r))
            .collect(),
    }
}
