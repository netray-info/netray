//! The view of a stored entry: the page parts, built without scoring or any lookup beyond the
//! check catalogue.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use netray_engine::catalogue::catalogue;
use netray_model::diff::{BlockChangeKind, ChangeClass, Diff};
use netray_model::{CheckId, Grade, Status};
use netray_store::Entry;

use crate::components::{CheckRowView, DiffBlockView, DiffEntry, GradeView, RunLineView, Tone};

/// The number of checks per status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusCounts {
    pub pass: usize,
    pub warn: usize,
    pub fail: usize,
    pub not_applicable: usize,
    pub not_tested: usize,
    pub unmeasured: usize,
}

/// Everything the entry page shows.
#[derive(Debug, Clone)]
pub struct EntryPageView {
    pub id: String,
    pub domain: String,
    pub grade: GradeView,
    pub run_line: RunLineView,
    pub diff: Option<DiffBlockView>,
    pub issues: Vec<CheckRowView>,
    pub counts: StatusCounts,
}

/// Format a UTC timestamp as "May 8, 2026 · 17:44 UTC".
pub fn format_utc_timestamp(t: SystemTime) -> String {
    use std::time::UNIX_EPOCH;
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let secs = t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let hour = (secs % 86400) / 3600;
    let minute = (secs % 3600) / 60;
    let (year, month, day) = epoch_days_to_ymd(secs / 86400);
    format!(
        "{} {}, {} \u{00b7} {:02}:{:02} UTC",
        MONTHS[(month - 1) as usize],
        day,
        year,
        hour,
        minute
    )
}

pub(crate) fn epoch_days_to_ymd(mut days: u64) -> (u64, u8, u8) {
    let mut year = 1970u64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let month_days: [u8; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 0u8;
    for (i, &dim) in month_days.iter().enumerate() {
        if days < dim as u64 {
            month = i as u8 + 1;
            break;
        }
        days -= dim as u64;
    }
    (year, month, days as u8 + 1)
}

fn is_leap(y: u64) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

pub(crate) fn grade_letter(grade: Grade) -> &'static str {
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

pub(crate) fn grade_tone(grade: Grade) -> Tone {
    match grade {
        Grade::APlus | Grade::A | Grade::B => Tone::Pass,
        Grade::C | Grade::D => Tone::Warn,
        Grade::F => Tone::Fail,
        Grade::Incomplete => Tone::Untested,
    }
}

pub(crate) fn status_tone(status: Option<Status>) -> Tone {
    match status {
        Some(Status::Pass) => Tone::Pass,
        Some(Status::Warn) => Tone::Warn,
        Some(Status::Fail) => Tone::Fail,
        _ => Tone::Untested,
    }
}

fn issue_row(id: &CheckId, status: Status) -> CheckRowView {
    let tone = status_tone(Some(status));
    match catalogue().get(id) {
        Some(spec) => CheckRowView {
            code: id.to_string(),
            tone,
            title: spec.title.clone(),
            since: None,
            body: spec.finding.clone(),
            fix: Some(spec.fix_hint.clone()),
            evidence_href: None,
            guide_href: Some(spec.guide.clone()),
        },
        None => CheckRowView {
            code: id.to_string(),
            tone,
            title: id.to_string(),
            since: None,
            body: String::new(),
            fix: None,
            evidence_href: None,
            guide_href: None,
        },
    }
}

/// The rows of one status: catalogue order, then unknown IDs in stored order.
fn issues_of(entry: &Entry, status: Status) -> Vec<CheckRowView> {
    let stored: Vec<&CheckId> = entry
        .run
        .checks
        .iter()
        .filter(|c| c.status == status)
        .map(|c| &c.id)
        .collect();
    let known = catalogue()
        .entries()
        .iter()
        .map(|e| &e.id)
        .filter(|id| stored.contains(id));
    let unknown = stored
        .iter()
        .copied()
        .filter(|id| catalogue().get(id).is_none());
    known
        .chain(unknown)
        .map(|id| issue_row(id, status))
        .collect()
}

fn counts_of(entry: &Entry) -> StatusCounts {
    let n = |status| {
        entry
            .run
            .checks
            .iter()
            .filter(|c| c.status == status)
            .count()
    };
    StatusCounts {
        pass: n(Status::Pass),
        warn: n(Status::Warn),
        fail: n(Status::Fail),
        not_applicable: n(Status::NotApplicable),
        not_tested: n(Status::NotTested),
        unmeasured: n(Status::Unmeasured),
    }
}

/// The page view of a stored entry.
pub fn entry_view(entry: &Entry) -> EntryPageView {
    let recorded = UNIX_EPOCH + Duration::from_secs(entry.created_at.max(0) as u64);
    let mut issues = issues_of(entry, Status::Fail);
    issues.extend(issues_of(entry, Status::Warn));
    EntryPageView {
        id: entry.id.clone(),
        domain: entry.domain.clone(),
        grade: GradeView {
            letter: grade_letter(entry.grade).to_string(),
            score: None,
            tone: grade_tone(entry.grade),
        },
        run_line: RunLineView {
            parts: vec![
                format!("recorded {}", format_utc_timestamp(recorded)),
                format!("profile v{}", entry.profile_version),
                format!("engine {}", entry.engine_version),
            ],
        },
        diff: entry.diff.as_ref().map(diff_view),
        issues,
        counts: counts_of(entry),
    }
}

fn class_word(class: &ChangeClass) -> &'static str {
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

/// The diff block of a stored diff.
pub fn diff_view(diff: &Diff) -> DiffBlockView {
    let from = grade_letter(diff.grade.from);
    let to = grade_letter(diff.grade.to);
    let summary = match diff.grade.rescored {
        Some(r) => format!(
            "{from} \u{2192} {} (profile) \u{2192} {to}",
            grade_letter(r)
        ),
        None if from == to => format!("Grade unchanged ({to})"),
        None => format!("{from} \u{2192} {to}"),
    };
    let checks = diff.checks.iter().map(|c| DiffEntry {
        kind: class_word(&c.class).to_string(),
        tone: if c.class == ChangeClass::Fixed {
            Tone::Fix
        } else {
            status_tone(c.to)
        },
        text: catalogue()
            .get(&c.id)
            .map_or_else(|| c.id.to_string(), |s| s.title.clone()),
        protocol: c.id.protocol().as_str().to_string(),
    });
    let evidence = diff.evidence.iter().map(|b| DiffEntry {
        kind: "evidence".to_string(),
        tone: Tone::Untested,
        text: match &b.kind {
            BlockChangeKind::Added => format!("{} added", b.block),
            BlockChangeKind::Removed => format!("{} removed", b.block),
            BlockChangeKind::Changed { items } if items.is_empty() => {
                format!("{} changed", b.block)
            }
            BlockChangeKind::Changed { items } => {
                format!("{}: {} changed", b.block, items.len())
            }
        },
        protocol: b.block.module().to_string(),
    });
    DiffBlockView {
        title: "Changes since the previous entry".to_string(),
        summary,
        entries: checks.chain(evidence).collect(),
        note: diff
            .engine_changed
            .then(|| "The previous entry was recorded by another engine version.".to_string()),
    }
}
