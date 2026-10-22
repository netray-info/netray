//! One domain run end to end: the registry's modules over a section plan under a hard deadline,
//! each section's outcome adapted onto a section result, the result scored with a profile, and
//! the `Run` the history stores built from it.
//!
//! Copied from lens (`check.rs`, `modules.rs`, `score.rs`, `history.rs`) without its V1 parts
//! (spec v2-cutover, Phase 6): the stored `Run` must stay byte-identical to the one lens records,
//! or every domain stores a changed entry after the cutover. Keep the mappings as they are:
//! `NotTested` → `Skip` → `NotApplicable`, the evidence filter on the run's blocks, the IP
//! section's `NoDnsResults`, `{e:?}` as the reason of an errored section, the facts blocks no
//! section carries on the DNS section's evidence, sections sorted by name.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use netray_model::evidence::{Block, BlockId, EVIDENCE_SCHEMA_VERSION};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use tokio::sync::mpsc;

use crate::profile::Profile;
use crate::scoring::SectionScore;
use crate::{Domain, Registry, RunContext, RunOptions, RunReport, SectionEvent, SectionOutcome};

/// Hard deadline for a whole run. The section windows must sum below it.
pub const HARD_DEADLINE: Duration = Duration::from_secs(20);

/// The protocols in plan order.
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

/// The section plan of `registry`: every registered protocol whose `window` is `Some`, in the
/// order dns, tls, http, email, ip.
pub fn section_plan(
    registry: &Registry,
    window: impl Fn(Protocol) -> Option<Duration>,
) -> Vec<(Protocol, Duration)> {
    PROTOCOLS
        .into_iter()
        .filter(|p| registry.module(*p).is_some())
        .filter_map(|p| Some((p, window(p)?)))
        .collect()
}

// ---------------------------------------------------------------------------
// Section results
// ---------------------------------------------------------------------------

pub struct CheckInput {
    pub domain: String,
    pub dkim_selectors: Option<Vec<String>>,
}

/// Error that can occur for a single section. Its `Debug` form is the stored reason of an
/// errored section: do not rename a variant.
#[derive(Debug, Clone)]
pub enum SectionError {
    BackendError(String),
    Timeout,
    NoDnsResults,
    NotApplicable { reason: String },
}

#[derive(Clone, Debug)]
pub struct BackendResult {
    pub checks: Vec<CheckResult>,
    /// The section's blocks; the DNS section's also the facts blocks no section carries.
    pub evidence: Vec<Block>,
}

/// Output of a full domain run.
pub struct CheckOutput {
    pub domain: String,
    pub sections: HashMap<String, Result<BackendResult, SectionError>>,
    pub score: OverallScore,
    pub duration_ms: u64,
}

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
impl From<CheckVerdict> for Status {
    fn from(verdict: CheckVerdict) -> Self {
        match verdict {
            CheckVerdict::Pass => Status::Pass,
            CheckVerdict::Warn => Status::Warn,
            CheckVerdict::Fail => Status::Fail,
            CheckVerdict::NotFound => Status::Fail,
            CheckVerdict::Skip => Status::NotApplicable,
            CheckVerdict::Unmeasured => Status::Unmeasured,
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
    /// The blocks of the run the check judged by.
    pub evidence: Vec<BlockId>,
}

pub fn section_name(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Dns => "dns",
        Protocol::Tls => "tls",
        Protocol::Http => "http",
        Protocol::Email => "email",
        Protocol::Ip => "ip",
    }
}

pub fn verdict(status: Status) -> CheckVerdict {
    match status {
        Status::Pass => CheckVerdict::Pass,
        Status::Warn => CheckVerdict::Warn,
        Status::Fail => CheckVerdict::Fail,
        Status::NotApplicable | Status::NotTested => CheckVerdict::Skip,
        Status::Unmeasured => CheckVerdict::Unmeasured,
    }
}

/// Maps the `protocol` section's outcome of an engine run onto its section result. The IP
/// section of a run whose resolve stage answered no address is `NoDnsResults`.
pub fn adapt(
    protocol: Protocol,
    domain: &str,
    outcome: SectionOutcome,
    report: &RunReport,
) -> Result<BackendResult, SectionError> {
    let section = section_name(protocol);
    if protocol == Protocol::Ip
        && report.resolve_error.is_none()
        && report.facts.a.is_empty()
        && report.facts.aaaa.is_empty()
        && !report.facts.no_address_definite
    {
        return Err(SectionError::NoDnsResults);
    }
    match outcome {
        SectionOutcome::Measured { checks, evidence } => {
            let run: HashSet<&BlockId> = report.blocks().into_iter().map(|b| &b.id).collect();
            let checks: Vec<CheckResult> = checks
                .into_iter()
                .map(|c| CheckResult {
                    name: c.id.name().to_string(),
                    verdict: verdict(c.status),
                    messages: c.findings,
                    evidence: c
                        .evidence
                        .into_iter()
                        .filter(|id| run.contains(id))
                        .collect(),
                })
                .collect();
            Ok(BackendResult { checks, evidence })
        }
        SectionOutcome::Incomplete { reason } => {
            tracing::warn!(service = section, url = %domain, error = %reason, "backend call failed");
            Err(SectionError::BackendError(reason))
        }
        SectionOutcome::TimedOut => {
            tracing::warn!(service = section, url = %domain, error = "timeout", "backend call failed");
            Err(SectionError::Timeout)
        }
        SectionOutcome::NotApplicable { reason } => Err(SectionError::NotApplicable { reason }),
    }
}

// ---------------------------------------------------------------------------
// Orchestration
// ---------------------------------------------------------------------------

/// What a run task needs of one domain run.
pub struct DomainRun {
    pub output: CheckOutput,
    /// The facts blocks no section carries when the DNS section, which publishes them, did not
    /// answer; an answering DNS section holds them itself.
    pub facts: Vec<Block>,
    /// The run as the history stores it.
    pub run: Run,
}

/// [`run_check_streaming`], then [`build_run`] over its output.
pub async fn run_domain(
    registry: &Registry,
    plan: &[(Protocol, Duration)],
    resolve_budget: Duration,
    profile: &Profile,
    input: CheckInput,
    hard_deadline: Duration,
    on_event: impl FnMut(&SectionEvent) + Send,
) -> DomainRun {
    let (output, facts) = run_check_streaming(
        registry,
        plan,
        resolve_budget,
        profile,
        input,
        hard_deadline,
        on_event,
    )
    .await;
    let run = build_run(profile, &output, &facts);
    DomainRun { output, facts, run }
}

/// Runs the `plan` sections of `registry` under `hard_deadline`, the resolve stage under
/// `resolve_budget`, calling `on_event` for each section as it finishes. Sections that finished
/// before the deadline are kept; the unfinished ones become `Err(SectionError::Timeout)`. Also
/// returns the facts blocks no section carries when the DNS section did not answer.
pub async fn run_check_streaming(
    registry: &Registry,
    plan: &[(Protocol, Duration)],
    resolve_budget: Duration,
    profile: &Profile,
    input: CheckInput,
    hard_deadline: Duration,
    mut on_event: impl FnMut(&SectionEvent) + Send,
) -> (CheckOutput, Vec<Block>) {
    let start = Instant::now();
    let domain = input.domain.clone();
    let base = RunContext {
        deadline: start + hard_deadline,
        domain: Domain::new(domain.as_str()),
        options: RunOptions {
            dkim_selectors: input.dkim_selectors.clone(),
        },
    };

    let (tx, mut rx) = mpsc::channel(plan.len().max(1));
    let mut events = Vec::new();
    let (report, ()) = futures::join!(
        crate::run(registry, base, plan, resolve_budget, tx),
        async {
            while let Some(event) = rx.recv().await {
                on_event(&event);
                events.push(event);
            }
        }
    );

    if let Some(e) = &report.resolve_error {
        tracing::warn!(error = %e, "address resolution failed");
    }

    let mut sections: HashMap<String, Result<BackendResult, SectionError>> = HashMap::new();
    for event in events {
        if let Some(&(protocol, _)) = plan.iter().find(|(p, _)| *p == event.protocol) {
            sections.insert(
                section_name(protocol).to_string(),
                adapt(protocol, &domain, event.outcome, &report),
            );
        }
    }
    for &(protocol, _) in plan {
        sections
            .entry(section_name(protocol).to_string())
            .or_insert(Err(SectionError::Timeout));
    }

    let carried: HashSet<_> = sections
        .values()
        .flatten()
        .flat_map(|r| r.evidence.iter().map(|b| b.id.clone()))
        .collect();
    let mut facts_evidence: Vec<Block> = report
        .blocks()
        .into_iter()
        .filter(|b| !carried.contains(&b.id))
        .cloned()
        .collect();
    if let Some(Ok(dns)) = sections.get_mut("dns") {
        dns.evidence.append(&mut facts_evidence);
    }

    // Warn about profile sections with no registered module.
    for protocol in profile.sections.keys() {
        let name = protocol.as_str();
        if !sections.contains_key(name) {
            tracing::warn!(section = %name, "profile section has no registered backend — skipped");
        }
    }

    let score = score(profile, &sections);

    let output = CheckOutput {
        domain,
        sections,
        score,
        duration_ms: start.elapsed().as_millis() as u64,
    };
    (output, facts_evidence)
}

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

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

/// The section result `result` of `section` as the event the engine's scoring takes.
pub fn event(section: &str, result: &Result<BackendResult, SectionError>) -> Option<SectionEvent> {
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

/// Scores section results with `profile` through [`crate::scoring::score`].
pub fn score(
    profile: &Profile,
    sections: &HashMap<String, Result<BackendResult, SectionError>>,
) -> OverallScore {
    let events: Vec<SectionEvent> = sections
        .iter()
        .filter_map(|(name, result)| event(name, result))
        .collect();
    let s = crate::scoring::score(profile, &events);
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

// ---------------------------------------------------------------------------
// The stored run
// ---------------------------------------------------------------------------

/// Every registered module's volatile paths, in plan order.
pub fn volatile(registry: &Registry) -> Vec<&'static str> {
    PROTOCOLS
        .into_iter()
        .filter_map(|p| registry.module(p))
        .flat_map(|m| m.volatile().iter().map(|path| path.0))
        .collect()
}

fn catalogue_checks(protocol: Protocol, status: Status) -> impl Iterator<Item = RunCheck> {
    crate::catalogue()
        .entries()
        .iter()
        .filter(move |e| e.protocol == protocol)
        .map(move |e| RunCheck {
            id: e.id.clone(),
            status,
            findings: vec![],
            evidence: vec![],
        })
}

/// The scored run: one section per scored section, its checks, every answering section's
/// evidence plus `facts`, and the grade and completeness of `output.score`.
pub fn build_run(profile: &Profile, output: &CheckOutput, facts: &[Block]) -> Run {
    let mut names: Vec<&String> = output.sections.keys().collect();
    names.sort();

    let mut sections = Vec::new();
    let mut checks = Vec::new();
    let mut blocks = Vec::new();
    for name in names {
        let Some(protocol) = protocol_for(name) else {
            continue;
        };
        let (outcome, reason) = match &output.sections[name] {
            Ok(result) => {
                checks.extend(result.checks.iter().filter_map(|c| {
                    Some(RunCheck {
                        id: CheckId::parse(&format!("{name}.{}", c.name)).ok()?,
                        status: c.verdict.clone().into(),
                        findings: c.messages.clone(),
                        evidence: c.evidence.clone(),
                    })
                }));
                blocks.extend(result.evidence.iter().cloned());
                crate::measured_kind_and_reason(
                    result
                        .checks
                        .iter()
                        .map(|c| (Status::from(c.verdict.clone()), c.messages.as_slice())),
                )
            }
            Err(SectionError::NotApplicable { reason }) => {
                checks.extend(catalogue_checks(protocol, Status::NotApplicable));
                (SectionKind::NotApplicable, Some(reason.clone()))
            }
            Err(e) => {
                checks.extend(catalogue_checks(protocol, Status::Unmeasured));
                (SectionKind::Incomplete, Some(format!("{e:?}")))
            }
        };
        sections.push(RunSection {
            protocol,
            outcome,
            reason,
        });
    }
    blocks.extend(facts.iter().cloned());

    Run {
        profile_version: profile.version,
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: parse_grade(&output.score.grade).unwrap_or(Grade::Incomplete),
        complete: output.score.complete,
        sections,
        checks,
        blocks,
    }
}
