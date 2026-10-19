//! Pure views of a run for the result page: open issues grouped, the passed checks, the diff
//! block, the history sentence and the layout input. No I/O.

use std::collections::BTreeMap;

use netray_engine::catalogue::{Catalogue, CheckSpec, catalogue};
use netray_engine::profile::{Profile, Profiles};
use netray_engine::scoring::Score;
use netray_engine::{SectionEvent, SectionOutcome};
use netray_model::diff::{BlockChangeKind, ChangeClass, Diff};
use netray_model::evidence::{BlockId, EVIDENCE_SCHEMA_VERSION};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, FixOwner, Grade, Protocol, Severity, Status};
use netray_store::{Entry, Recorded};

use crate::api::{RefusalReason, RunRefusal};
use crate::components::{
    CheckRowView, DiffBlockView, DiffEntry, RunLineView, SegmentState, Tone, TraceRow, TraceView,
    line_tone,
};
use crate::entries::view::{epoch_days_to_ymd, grade_letter, status_tone};
use crate::evidence::href;
use crate::spectrum::{Layout, LineTone, RunInput, SectionState};

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
    pub owner_label: &'static str,
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
        FixOwner::DnsProvider => "Your DNS provider",
        FixOwner::Registrar => "Your registrar",
        FixOwner::CertificateProvider => "Your certificate provider",
        FixOwner::WebServer => "Your web server configuration",
        FixOwner::MailProvider => "Your mail provider",
        FixOwner::Hosting => "Your hosting provider",
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
        Protocol::Email => "Email",
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
            line: line_code(&check.id),
            tone: status_tone(Some(check.status)),
            title: spec.title.clone(),
            since: since.get(&check.id).cloned(),
            body,
            fix: Some(spec.fix_hint.clone()),
            meta: Some(meta_text(spec)),
            evidence_href: Some(evidence_href(check, links)),
            guide_href: Some(spec.guide.clone()),
        },
        owner: owner_word(spec.fix_owner),
        owner_label: owner_label(spec.fix_owner),
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

/// Every warn and fail check of `run` that the catalogue knows, once, grouped. Rows run fail
/// before warn, then by severity, then catalogue order, as the board lists them; groups run by
/// their first row, so the group holding the worst line comes first.
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
    rows.sort_by_key(|(r, severity)| (r.status == "warn", severity_rank(*severity), r.order));

    let mut groups: Vec<IssueGroup> = Vec::new();
    for (row, _) in rows {
        let (key, label) = match grouping {
            Grouping::Owner => (row.owner, row.owner_label),
            Grouping::Severity => (row.severity, row.severity),
            Grouping::Protocol => (
                row.protocol,
                PROTOCOLS
                    .into_iter()
                    .find(|p| p.as_str() == row.protocol)
                    .map_or(row.protocol, protocol_label),
            ),
        };
        match groups.iter_mut().find(|g| g.key == key) {
            Some(group) => group.rows.push(row),
            None => groups.push(IssueGroup {
                key: key.to_string(),
                label: label.to_string(),
                rows: vec![row],
            }),
        }
    }
    groups
}

/// The number of checks the clean bar counts: those that pass or do not apply and that the
/// catalogue still knows.
pub fn clean_count(run: &Run, catalogue: &Catalogue) -> usize {
    run.checks
        .iter()
        .filter(|c| matches!(c.status, Status::Pass | Status::NotApplicable))
        .filter(|c| index_of(catalogue, &c.id).is_some())
        .count()
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
                line: line_code(&check.id),
                tone: status_tone(Some(check.status)),
                title: spec_at.map_or_else(
                    || check.id.to_string(),
                    |i| catalogue.entries()[i].title.clone(),
                ),
                since: None,
                body,
                fix: None,
                meta: spec_at.map(|i| meta_text(&catalogue.entries()[i])),
                evidence_href: None,
                guide_href: None,
            },
        ));
    }
    rows.sort_by_key(|(key, _)| *key);
    rows.into_iter().map(|(_, row)| row).collect()
}

/// The meta line of an item: its protocol and the catalogue's source, "HTTP \u{b7} RFC 6797".
pub fn meta_text(spec: &CheckSpec) -> String {
    format!("{} \u{b7} {}", section_name(spec.protocol), spec.source)
}

/// A check's code: its protocol tag and its 1-based position in the spectrum's line order, two
/// digits ("DNS·04"). `None` for an id the catalogue does not hold.
pub fn line_code(id: &CheckId) -> Option<String> {
    let cat = catalogue();
    let mut position = 0;
    for protocol in PROTOCOLS {
        for spec in cat.for_protocol(protocol).entries() {
            position += 1;
            if &spec.id == id {
                let tag = match protocol {
                    Protocol::Dns => "DNS",
                    Protocol::Tls => "TLS",
                    Protocol::Http => "HTTP",
                    Protocol::Email => "MAIL",
                    Protocol::Ip => "IP",
                };
                return Some(format!("{tag}\u{b7}{position:02}"));
            }
        }
    }
    None
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

/// The run line part and the diff note of a complete run whose record failed.
pub const NOT_RECORDED: &str = "not recorded";

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

/// The diff block of a complete run whose record failed.
pub fn not_recorded_view() -> DiffView {
    empty_view(Some(NOT_RECORDED.to_string()))
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
        protocol: line_code(&c.id).unwrap_or_else(|| c.id.protocol().as_str().to_string()),
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
    let summary = grade_line(&grade);
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

/// The grade steps of a diff as one line: "C → B", with the rescored step "C → D (profile
/// effect) → B (domain effect)".
pub fn grade_line(steps: &[GradeStep]) -> String {
    steps
        .iter()
        .map(|s| match &s.note {
            Some(note) => format!("{} ({note})", s.grade),
            None => s.grade.clone(),
        })
        .collect::<Vec<_>>()
        .join(" \u{2192} ")
}

fn date_of(timestamp: i64) -> (u64, u8, u8) {
    epoch_days_to_ymd(timestamp.div_euclid(86_400).max(0) as u64)
}

/// A protocol's name in a sentence of the page.
pub fn section_name(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Dns => "DNS",
        Protocol::Tls => "TLS",
        Protocol::Http => "HTTP",
        Protocol::Email => "Email",
        Protocol::Ip => "IP",
    }
}

/// The run title of `entry`, the `ordinal`-th of its domain (the first is 1) when that is
/// known: ordinal, time, check count, profile, the duration when known, "cached" and "first run".
pub fn run_title(entry: &Entry, ordinal: Option<usize>, cached: bool) -> RunLineView {
    let checks = entry.run.checks.len();
    let mut parts: Vec<String> = ordinal.map(|n| format!("Run {n}")).into_iter().collect();
    parts.extend([
        format!("{} UTC", utc_minute(entry.created_at)),
        format!("{checks} check{}", if checks == 1 { "" } else { "s" }),
        format!("profile {}", entry.profile_version),
    ]);
    if let Some(ms) = entry.duration_ms {
        parts.push(format!("{:.1} s", ms as f64 / 1000.0));
    }
    if cached {
        parts.push("cached".to_string());
    }
    if ordinal == Some(1) {
        parts.push("first run".to_string());
    }
    RunLineView { parts }
}

/// The hard-fail banner: the failed hard-fail checks and the sections left out of the score.
#[derive(Debug, Clone)]
pub struct HardFailView {
    pub hard_fail: Option<String>,
    pub without: Option<String>,
}

/// The banner of `score` under the letter `shown`, `None` when nothing is left to say. The
/// hard-fail sentence names the letter F, so it stays out when the score's grade is not `shown`.
pub fn hard_fail_view(score: &Score, catalogue: &Catalogue, shown: Grade) -> Option<HardFailView> {
    let hard_fail = (!score.hard_fail_checks.is_empty() && score.grade == shown).then(|| {
        let titles: Vec<String> = score
            .hard_fail_checks
            .iter()
            .map(|id| {
                catalogue
                    .get(id)
                    .map_or_else(|| id.to_string(), |spec| spec.title.clone())
            })
            .collect();
        format!(
            "Hard fail: {}. A failed hard-fail check sets the grade to F whatever the score.",
            titles.join(", ")
        )
    });
    let without = (!score.not_applicable.is_empty()).then(|| {
        let sections: Vec<String> = score
            .not_applicable
            .iter()
            .map(|(protocol, reason)| match reason.as_str() {
                "" => section_name(*protocol).to_string(),
                reason => format!("{} ({reason})", section_name(*protocol)),
            })
            .collect();
        format!("Score computed without: {}", sections.join(", "))
    });
    (hard_fail.is_some() || without.is_some()).then_some(HardFailView { hard_fail, without })
}

/// The UTC date of `timestamp`, `YYYY-MM-DD`.
pub fn run_date(timestamp: i64) -> String {
    let (year, month, day) = date_of(timestamp);
    format!("{year:04}-{month:02}-{day:02}")
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
fn spans<T: Copy + PartialEq>(points: &[(i64, T)]) -> Vec<(T, i64, i64, usize)> {
    let mut out: Vec<(T, i64, i64, usize)> = Vec::new();
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

/// The history sentence of every catalogue line over `points` (oldest first), as (check id,
/// sentence) in catalogue order; a run that lacks a check counts as `not_tested` for it.
pub fn history_sentences(points: &[(i64, &Run)]) -> Vec<(String, String)> {
    catalogue()
        .entries()
        .iter()
        .map(|spec| {
            let line: Vec<(i64, Status)> = points
                .iter()
                .map(|(at, run)| {
                    let status = run
                        .checks
                        .iter()
                        .find(|c| c.id == spec.id)
                        .map_or(Status::NotTested, |c| c.status);
                    (*at, status)
                })
                .collect();
            (spec.id.to_string(), history_sentence(&line))
        })
        .collect()
}

/// What an open check's history says, from its statuses over the shown runs, oldest first
/// (`None` marks a run without the check, which counts as not open) and the number of the first
/// run. `None` when the check is not open in the latest run.
pub fn since_text(statuses: &[Option<Status>], first_run_no: usize) -> Option<String> {
    let now = (*statuses.last()?).filter(|s| is_open(*s))?;
    let start = statuses
        .iter()
        .rposition(|s| *s != Some(now))
        .map_or(0, |i| i + 1);
    let run = first_run_no + start;
    Some(match start.checked_sub(1).map(|i| statuses[i]) {
        None if statuses.len() == 1 => "first run".to_string(),
        None => format!("in all {} runs", statuses.len()),
        Some(Some(Status::Fail)) if now == Status::Warn => {
            format!("down from fail since run {run}")
        }
        Some(Some(Status::Warn)) if now == Status::Fail => {
            format!("up from warn since run {run}")
        }
        Some(_) => format!("lit up in run {run}"),
    })
}

/// The layout input of a stored run; `None` when its profile version is unknown.
pub fn layout_input(run: &Run) -> Option<RunInput<'static>> {
    Some(layout_input_with(run, Profiles::get(run.profile_version)?))
}

/// The layout input of a stored run; an unknown profile version falls back to the current profile.
pub fn layout_input_or_current(run: &Run) -> RunInput<'static> {
    let profile = Profiles::get(run.profile_version).unwrap_or_else(Profiles::current);
    layout_input_with(run, profile)
}

fn layout_input_with(run: &Run, profile: &'static Profile) -> RunInput<'static> {
    RunInput {
        statuses: run
            .checks
            .iter()
            .map(|c| (c.id.clone(), c.status))
            .collect(),
        sections: run
            .sections
            .iter()
            .map(|s| (s.protocol, section_state(&s.outcome)))
            .collect(),
        profile,
    }
}

/// A Unix timestamp as "YYYY-MM-DD HH:MM", UTC.
pub fn utc_minute(timestamp: i64) -> String {
    let (year, month, day) = date_of(timestamp);
    let secs = timestamp.max(0);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        secs % 86_400 / 3_600,
        secs % 3_600 / 60
    )
}

/// A line's mark in the trace: a fix is a pass there, the trace draws no fix.
fn trace_tone(tone: LineTone) -> Tone {
    match tone {
        LineTone::Fixed => Tone::Pass,
        tone => line_tone(tone),
    }
}

/// One run of the history behind the trace: when it was recorded, its grade and its place in
/// the stored history when that is known.
pub struct TraceInput {
    pub created_at: i64,
    pub grade: Grade,
    pub no: Option<usize>,
}

/// The trace of a layout: one row per run, newest first, `inputs` the history of each run in
/// the layout's run order; `table_href` the history table's address when the domain has one.
pub fn trace_view(layout: &Layout, inputs: &[TraceInput], table_href: Option<String>) -> TraceView {
    let shown: Vec<&TraceInput> = inputs.iter().take(layout.runs.len()).collect();
    let rows: Vec<TraceRow> = layout
        .runs
        .iter()
        .zip(&shown)
        .enumerate()
        .map(|(k, (run, input))| TraceRow {
            no: input.no,
            date: run_date(input.created_at),
            grade: grade_letter(input.grade).to_string(),
            cells: run.tones.iter().map(|t| trace_tone(*t)).collect(),
            current: k == 0,
        })
        .collect();
    let edge =
        |input: Option<&&TraceInput>| input.map_or_else(String::new, |i| month_day(i.created_at));
    let count = rows.len();
    TraceView {
        newest: edge(shown.first()),
        oldest: edge(shown.last()),
        caption: format!(
            "Each line carries its trace: {count} {}, newest on top.",
            if count == 1 { "run" } else { "runs" }
        ),
        hint: "hover a run to replay the spectrum \u{b7} tap to pin".to_string(),
        table_href,
        rows,
    }
}

/// The run so far: the sections that arrived, in arrival order; not complete, grade incomplete.
pub fn live_run(sections: &[SectionEvent]) -> Run {
    let mut run = Run {
        profile_version: Profiles::current().version,
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::Incomplete,
        complete: false,
        sections: Vec::new(),
        checks: Vec::new(),
        blocks: Vec::new(),
    };
    for event in sections {
        let (outcome, reason) = match &event.outcome {
            SectionOutcome::Measured { checks, evidence } => {
                run.checks.extend(checks.iter().map(|c| RunCheck {
                    id: c.id.clone(),
                    status: c.status,
                    findings: c.findings.clone(),
                    evidence: c.evidence.clone(),
                }));
                run.blocks.extend(evidence.iter().cloned());
                (SectionKind::Measured, None)
            }
            SectionOutcome::NotApplicable { reason } => {
                (SectionKind::NotApplicable, Some(reason.clone()))
            }
            SectionOutcome::Incomplete { reason } => {
                (SectionKind::Incomplete, Some(reason.clone()))
            }
            SectionOutcome::TimedOut => (SectionKind::TimedOut, None),
        };
        run.sections.push(RunSection {
            protocol: event.protocol,
            outcome,
            reason,
        });
    }
    run
}

fn section_state(kind: &SectionKind) -> SectionState {
    match kind {
        SectionKind::Measured => SectionState::Measured,
        SectionKind::NotApplicable => SectionState::NotApplicable,
        SectionKind::Incomplete | SectionKind::TimedOut => SectionState::Unmeasured,
    }
}

/// The layout input of a run in progress: the sections that finished come from `live`, every
/// other protocol keeps what `stored` (the latest stored run) showed. A finished section that
/// is incomplete or timed out is unmeasured.
pub fn live_input(stored: Option<&Run>, live: &Run) -> RunInput<'static> {
    let finished = |p: Protocol| live.sections.iter().any(|s| s.protocol == p);
    let mut statuses = BTreeMap::new();
    let mut sections = BTreeMap::new();
    if let Some(stored) = stored {
        for c in stored.checks.iter().filter(|c| !finished(c.id.protocol())) {
            statuses.insert(c.id.clone(), c.status);
        }
        for s in stored.sections.iter().filter(|s| !finished(s.protocol)) {
            sections.insert(s.protocol, section_state(&s.outcome));
        }
    }
    for c in &live.checks {
        statuses.insert(c.id.clone(), c.status);
    }
    for s in &live.sections {
        sections.insert(s.protocol, section_state(&s.outcome));
    }
    RunInput {
        statuses,
        sections,
        profile: Profiles::current(),
    }
}

/// The segments of the progress ray in `PROTOCOLS` order: pending until the section arrived,
/// then done, or incomplete for an incomplete or timed-out one.
pub fn segment_states(run: &Run) -> [SegmentState; 5] {
    PROTOCOLS.map(|p| {
        match run
            .sections
            .iter()
            .find(|s| s.protocol == p)
            .map(|s| &s.outcome)
        {
            None => SegmentState::Pending,
            Some(SectionKind::Measured | SectionKind::NotApplicable) => SegmentState::Done,
            Some(SectionKind::Incomplete | SectionKind::TimedOut) => SegmentState::Incomplete,
        }
    })
}

/// The refusal sentence of a refused run: the lead text, and the seconds a countdown follows it
/// with.
pub fn refusal_line(refusal: &RunRefusal) -> (String, u64) {
    let lead = match (refusal.reason, refusal.last_run_ago_s) {
        (RefusalReason::Throttle, Some(ago)) if refusal.last_incomplete => {
            format!("a run {ago} s ago was incomplete; a new run is possible in ")
        }
        (RefusalReason::Throttle, Some(ago)) => {
            format!("checked {ago} s ago; a new run is possible in ")
        }
        (RefusalReason::Throttle, None) => "a new run is possible in ".to_string(),
        (RefusalReason::Budget | RefusalReason::IpLimit, _) => {
            "too many checks from your address; try again in ".to_string()
        }
        (RefusalReason::GlobalLimit | RefusalReason::Slots, _) => {
            "netray is busy; try again in ".to_string()
        }
    };
    (lead, refusal.retry_after_s)
}
