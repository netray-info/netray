//! Pure views of a run for the result page: open issues grouped, the passed checks, the diff
//! block, the history sentence and the layout input. No I/O.

use std::collections::BTreeMap;

use netray_engine::catalogue::{Catalogue, CheckSpec};
use netray_engine::profile::Profiles;
use netray_model::diff::{BlockChangeKind, ChangeClass, Diff};
use netray_model::evidence::BlockId;
use netray_model::run::{Run, RunCheck, SectionKind};
use netray_model::{CheckId, FixOwner, Protocol, Severity, Status};
use netray_store::{Entry, Recorded};

use crate::components::{CheckRowView, DiffBlockView, DiffEntry, Tone};
use crate::entries::view::{epoch_days_to_ymd, grade_letter, status_tone};
use crate::evidence::href;
use crate::spectrum::{RunInput, SectionState};

/// Where an issue's evidence link points.
pub enum Links<'a> {
    /// A stored entry: `/r/<id>/<protocol>[#ev-<block>]`.
    Entry(&'a str),
    /// A run that was not stored: `#ev-<block id>`, or `#evidence-<protocol>` without references.
    Live,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grouping {
    Owner,
    Severity,
    Protocol,
}

/// One open issue: the component view plus its grouping attributes.
#[derive(Debug, Clone)]
pub struct IssueRow {
    pub view: CheckRowView,
    pub owner: &'static str,
    pub severity: &'static str,
    pub protocol: &'static str,
    pub status: &'static str,
    pub order: usize,
}

#[derive(Debug, Clone)]
pub struct IssueGroup {
    pub key: String,
    pub label: String,
    pub rows: Vec<IssueRow>,
}

const OWNERS: [FixOwner; 6] = [
    FixOwner::DnsProvider,
    FixOwner::Registrar,
    FixOwner::CertificateProvider,
    FixOwner::WebServer,
    FixOwner::MailProvider,
    FixOwner::Hosting,
];

const SEVERITIES: [Severity; 4] = [
    Severity::Critical,
    Severity::High,
    Severity::Medium,
    Severity::Low,
];

const PROTOCOLS: [Protocol; 5] = [
    Protocol::Dns,
    Protocol::Tls,
    Protocol::Http,
    Protocol::Email,
    Protocol::Ip,
];

fn owner_word(owner: FixOwner) -> &'static str {
    match owner {
        FixOwner::DnsProvider => "dns_provider",
        FixOwner::Registrar => "registrar",
        FixOwner::CertificateProvider => "certificate_provider",
        FixOwner::WebServer => "web_server",
        FixOwner::MailProvider => "mail_provider",
        FixOwner::Hosting => "hosting",
    }
}

fn owner_label(owner: FixOwner) -> &'static str {
    match owner {
        FixOwner::DnsProvider => "your DNS provider",
        FixOwner::Registrar => "your registrar",
        FixOwner::CertificateProvider => "your certificate provider",
        FixOwner::WebServer => "your web server configuration",
        FixOwner::MailProvider => "your mail provider",
        FixOwner::Hosting => "your hosting provider",
    }
}

fn severity_word(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical => "critical",
        Severity::High => "high",
        Severity::Medium => "medium",
        Severity::Low => "low",
    }
}

fn severity_rank(severity: Severity) -> usize {
    SEVERITIES
        .iter()
        .position(|s| *s == severity)
        .unwrap_or(SEVERITIES.len())
}

fn protocol_label(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Dns => "DNS",
        Protocol::Tls => "TLS",
        Protocol::Http => "HTTP",
        Protocol::Email => "email",
        Protocol::Ip => "IP",
    }
}

fn owner_protocol(bid: &BlockId) -> Option<Protocol> {
    PROTOCOLS.into_iter().find(|p| p.as_str() == bid.module())
}

/// The evidence href of one check: the first referenced block, else the protocol's section.
pub fn evidence_href(check: &RunCheck, links: &Links<'_>) -> String {
    let first = check.evidence.first();
    match links {
        Links::Entry(id) => match first {
            Some(block) => href(
                id,
                owner_protocol(block).unwrap_or_else(|| check.id.protocol()),
                Some(block),
            ),
            None => href(id, check.id.protocol(), None),
        },
        Links::Live => match first {
            Some(block) => format!("#ev-{block}"),
            None => format!("#evidence-{}", check.id.protocol().as_str()),
        },
    }
}

fn is_open(status: Status) -> bool {
    matches!(status, Status::Warn | Status::Fail)
}

fn index_of(catalogue: &Catalogue, id: &CheckId) -> Option<usize> {
    catalogue.entries().iter().position(|s| &s.id == id)
}

fn issue_row(
    check: &RunCheck,
    spec: &CheckSpec,
    order: usize,
    links: &Links<'_>,
    since: &BTreeMap<CheckId, String>,
) -> IssueRow {
    let body = if check.findings.is_empty() {
        spec.finding.clone()
    } else {
        check.findings.join("; ")
    };
    IssueRow {
        view: CheckRowView {
            code: check.id.to_string(),
            tone: status_tone(Some(check.status)),
            title: spec.title.clone(),
            since: since.get(&check.id).cloned(),
            body,
            fix: Some(spec.fix_hint.clone()),
            evidence_href: Some(evidence_href(check, links)),
            guide_href: Some(spec.guide.clone()),
        },
        owner: owner_word(spec.fix_owner),
        severity: severity_word(spec.severity),
        protocol: spec.protocol.as_str(),
        status: if check.status == Status::Fail {
            "fail"
        } else {
            "warn"
        },
        order,
    }
}

/// Every warn and fail check of `run` that the catalogue knows, once, grouped. Groups run in
/// a fixed order and empty ones are left out; inside a group rows run by severity, fail before
/// warn, then catalogue order.
pub fn issues(
    run: &Run,
    catalogue: &Catalogue,
    links: Links<'_>,
    grouping: Grouping,
    since: &BTreeMap<CheckId, String>,
) -> Vec<IssueGroup> {
    let mut rows: Vec<(IssueRow, Severity)> = run
        .checks
        .iter()
        .filter(|c| is_open(c.status))
        .filter_map(|c| {
            let order = index_of(catalogue, &c.id)?;
            let spec = &catalogue.entries()[order];
            Some((issue_row(c, spec, order, &links, since), spec.severity))
        })
        .collect();
    rows.sort_by_key(|(r, severity)| (severity_rank(*severity), r.status == "warn", r.order));

    let keyed: Vec<(String, String)> = match grouping {
        Grouping::Owner => OWNERS
            .iter()
            .map(|o| (owner_word(*o).to_string(), owner_label(*o).to_string()))
            .collect(),
        Grouping::Severity => SEVERITIES
            .iter()
            .map(|s| (severity_word(*s).to_string(), severity_word(*s).to_string()))
            .collect(),
        Grouping::Protocol => PROTOCOLS
            .iter()
            .map(|p| (p.as_str().to_string(), protocol_label(*p).to_string()))
            .collect(),
    };
    let key_of = |row: &IssueRow| match grouping {
        Grouping::Owner => row.owner,
        Grouping::Severity => row.severity,
        Grouping::Protocol => row.protocol,
    };
    keyed
        .into_iter()
        .filter_map(|(key, label)| {
            let group: Vec<IssueRow> = rows
                .iter()
                .filter(|(r, _)| key_of(r) == key)
                .map(|(r, _)| r.clone())
                .collect();
            (!group.is_empty()).then_some(IssueGroup {
                key,
                label,
                rows: group,
            })
        })
        .collect()
}

/// Every check that is not an open issue: pass, not applicable, not tested, unmeasured, each in
/// catalogue order; a warn or fail the catalogue no longer knows comes last.
pub fn passed_checks(run: &Run, catalogue: &Catalogue) -> Vec<CheckRowView> {
    let mut rows: Vec<((usize, usize), CheckRowView)> = Vec::new();
    for (pos, check) in run.checks.iter().enumerate() {
        let spec_at = index_of(catalogue, &check.id);
        let rank = match (check.status, spec_at) {
            (Status::Warn | Status::Fail, Some(_)) => continue,
            (Status::Pass, Some(_)) => 0,
            (Status::NotApplicable, Some(_)) => 1,
            (Status::NotTested, Some(_)) => 2,
            (Status::Unmeasured, Some(_)) => 3,
            (_, None) => 4,
        };
        let body = match check.status {
            Status::NotApplicable => run
                .sections
                .iter()
                .find(|s| s.protocol == check.id.protocol())
                .and_then(|s| s.reason.clone())
                .unwrap_or_default(),
            Status::Unmeasured => "could not check".to_string(),
            Status::NotTested => String::new(),
            _ => check.findings.join("; "),
        };
        rows.push((
            (rank, spec_at.unwrap_or(pos)),
            CheckRowView {
                code: check.id.to_string(),
                tone: status_tone(Some(check.status)),
                title: spec_at.map_or_else(
                    || check.id.to_string(),
                    |i| catalogue.entries()[i].title.clone(),
                ),
                since: None,
                body,
                fix: None,
                evidence_href: None,
                guide_href: None,
            },
        ));
    }
    rows.sort_by_key(|(key, _)| *key);
    rows.into_iter().map(|(_, row)| row).collect()
}

/// The class label of a change.
pub fn change_label(class: &ChangeClass) -> &'static str {
    match class {
        ChangeClass::Fixed => "fixed",
        ChangeClass::Improved => "improved",
        ChangeClass::New => "new",
        ChangeClass::Worse => "worse",
        ChangeClass::NowApplicable => "now applicable",
        ChangeClass::NoLongerApplicable => "no longer applicable",
        ChangeClass::IntoNotTested => "into not tested",
        ChangeClass::OutOfNotTested => "out of not tested",
        ChangeClass::IntoNotMeasured => "into not measured",
        ChangeClass::OutOfNotMeasured => "out of not measured",
        ChangeClass::Added => "added",
        ChangeClass::Removed => "removed",
        ChangeClass::AddedByRelease => "added by release",
        ChangeClass::RemovedByRelease => "removed by release",
    }
}

/// One step of the grade line.
#[derive(Debug, Clone, PartialEq)]
pub struct GradeStep {
    pub grade: String,
    pub note: Option<String>,
}

/// The diff block plus its grade line.
#[derive(Debug, Clone)]
pub struct DiffView {
    pub block: DiffBlockView,
    pub grade: Vec<GradeStep>,
}

/// The note of an entry that has no previous state to compare with.
pub const FIRST_STATE: &str = "first state recorded \u{2014} re-check after your change";

const DIFF_TITLE: &str = "Changes since the previous entry";

fn empty_view(note: Option<String>) -> DiffView {
    DiffView {
        block: DiffBlockView {
            title: DIFF_TITLE.to_string(),
            summary: String::new(),
            entries: Vec::new(),
            note,
        },
        grade: Vec::new(),
    }
}

/// The diff block of a stored entry: the rows of its diff, or the first-state note.
pub fn diff_view(diff: Option<&Diff>, catalogue: &Catalogue) -> DiffView {
    let Some(diff) = diff else {
        return empty_view(Some(FIRST_STATE.to_string()));
    };
    let checks = diff.checks.iter().map(|c| DiffEntry {
        kind: change_label(&c.class).to_string(),
        tone: if c.class == ChangeClass::Fixed {
            Tone::Fix
        } else {
            status_tone(c.to)
        },
        text: catalogue
            .get(&c.id)
            .map_or_else(|| c.id.to_string(), |s| s.title.clone()),
        protocol: c.id.protocol().as_str().to_string(),
    });
    let blocks = diff.evidence.iter().map(|b| {
        let (kind, text) = match &b.kind {
            BlockChangeKind::Added => ("added", b.block.to_string()),
            BlockChangeKind::Removed => ("removed", b.block.to_string()),
            BlockChangeKind::Changed { items } => (
                "changed",
                match items.len() {
                    0 => b.block.to_string(),
                    1 => format!("{} (1 item)", b.block),
                    n => format!("{} ({n} items)", b.block),
                },
            ),
        };
        DiffEntry {
            kind: kind.to_string(),
            tone: Tone::Untested,
            text,
            protocol: b.block.module().to_string(),
        }
    });

    let step = |grade, note: Option<&str>| GradeStep {
        grade: grade_letter(grade).to_string(),
        note: note.map(str::to_string),
    };
    let grade = match diff.grade.rescored {
        Some(r) => vec![
            step(diff.grade.from, None),
            step(r, Some("profile effect")),
            step(diff.grade.to, Some("domain effect")),
        ],
        None => vec![step(diff.grade.from, None), step(diff.grade.to, None)],
    };
    let summary = grade
        .iter()
        .map(|s| s.grade.as_str())
        .collect::<Vec<_>>()
        .join(" \u{2192} ");
    DiffView {
        block: DiffBlockView {
            title: DIFF_TITLE.to_string(),
            summary,
            entries: checks.chain(blocks).collect(),
            note: diff
                .engine_changed
                .then(|| "engine changed between these runs".to_string()),
        },
        grade,
    }
}

fn date_of(timestamp: i64) -> (u64, u8, u8) {
    epoch_days_to_ymd(timestamp.div_euclid(86_400).max(0) as u64)
}

fn month_day(timestamp: i64) -> String {
    let (_, month, day) = date_of(timestamp);
    format!("{month:02}-{day:02}")
}

fn join_note(head: String, own: Option<String>) -> String {
    match own {
        Some(own) => format!("{head}; {own}"),
        None => head,
    }
}

/// The diff block after a fresh run, by what the store did with it.
pub fn recorded_diff_view(
    recorded: &Recorded,
    latest: Option<&Entry>,
    catalogue: &Catalogue,
) -> DiffView {
    match recorded {
        Recorded::Changed { diff, .. } => diff_view(Some(diff), catalogue),
        Recorded::Identical { .. } => {
            let mut view = diff_view(latest.and_then(|e| e.diff.as_ref()), catalogue);
            view.block.note = Some(match latest {
                Some(e) => {
                    let (year, month, day) = date_of(e.created_at);
                    let head = format!("unchanged since {year:04}-{month:02}-{day:02}");
                    join_note(head, view.block.note.take())
                }
                None => "unchanged".to_string(),
            });
            view
        }
        Recorded::First { .. } => diff_view(None, catalogue),
        Recorded::NotStored { diff: Some(diff) } => {
            let mut view = diff_view(Some(diff), catalogue);
            view.block.note = Some(join_note(
                "incomplete, not recorded".to_string(),
                view.block.note.take(),
            ));
            view
        }
        Recorded::NotStored { diff: None } => {
            empty_view(Some("not recorded: this run is incomplete".to_string()))
        }
    }
}

fn status_word(status: Status) -> &'static str {
    match status {
        Status::Pass => "pass",
        Status::Warn => "warn",
        Status::Fail => "fail",
        Status::NotApplicable => "not applicable",
        Status::NotTested => "not tested",
        Status::Unmeasured => "not measured",
    }
}

/// The spans of equal consecutive statuses: (status, first timestamp, last timestamp, points).
fn spans(points: &[(i64, Status)]) -> Vec<(Status, i64, i64, usize)> {
    let mut out: Vec<(Status, i64, i64, usize)> = Vec::new();
    for (at, status) in points {
        match out.last_mut() {
            Some(span) if span.0 == *status => {
                span.2 = *at;
                span.3 += 1;
            }
            _ => out.push((*status, *at, *at, 1)),
        }
    }
    out
}

/// One line's history over (created_at, status) points, oldest first.
pub fn history_sentence(points: &[(i64, Status)]) -> String {
    let spans = spans(points);
    let last = spans.len().saturating_sub(1);
    spans
        .iter()
        .enumerate()
        .map(|(i, (status, start, end, n))| {
            let word = status_word(*status);
            if i == last {
                format!("{word} since {}", month_day(*start))
            } else if *n == 1 || month_day(*start) == month_day(*end) {
                format!("{word} {}", month_day(*start))
            } else {
                format!("{word} {} to {}", month_day(*start), month_day(*end))
            }
        })
        .collect::<Vec<_>>()
        .join(" \u{00b7} ")
}

/// The start of the current span, unless it covers every point.
pub fn issue_since(points: &[(i64, Status)]) -> Option<String> {
    let spans = spans(points);
    match spans.as_slice() {
        [] | [_] => None,
        [.., (_, start, _, _)] => Some(month_day(*start)),
    }
}

/// The layout input of a stored run; `None` when its profile version is unknown.
pub fn layout_input(run: &Run) -> Option<RunInput<'static>> {
    let profile = Profiles::get(run.profile_version)?;
    Some(RunInput {
        statuses: run
            .checks
            .iter()
            .map(|c| (c.id.clone(), c.status))
            .collect(),
        sections: run
            .sections
            .iter()
            .map(|s| {
                let state = match s.outcome {
                    SectionKind::Measured => SectionState::Measured,
                    SectionKind::NotApplicable => SectionState::NotApplicable,
                    SectionKind::Incomplete | SectionKind::TimedOut => SectionState::Unmeasured,
                };
                (s.protocol, state)
            })
            .collect(),
        profile,
    })
}
