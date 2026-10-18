//! Contract tests for the passed-checks list (spec v2-result-page, requirement 13; C5, C12).
use std::collections::BTreeMap;

use netray_engine::catalogue::catalogue;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_web::pages::result::views::{Grouping, Links, issues, passed_checks};

const NA_REASON: &str = "the domain has no mail exchanger";

fn id_at(i: usize) -> CheckId {
    catalogue().entries()[i].id.clone()
}

fn rc(i: usize, status: Status) -> RunCheck {
    RunCheck {
        id: id_at(i),
        status,
        findings: vec![],
        evidence: vec![],
    }
}

/// Inserted in an order that is not the catalogue's; the not_applicable check's
/// protocol section carries the reason.
fn mixed_run() -> Run {
    let na_protocol = id_at(8).protocol();
    let sections = [
        Protocol::Dns,
        Protocol::Tls,
        Protocol::Http,
        Protocol::Email,
        Protocol::Ip,
    ]
    .into_iter()
    .map(|protocol| {
        if protocol == na_protocol {
            RunSection {
                protocol,
                outcome: SectionKind::NotApplicable,
                reason: Some(NA_REASON.into()),
            }
        } else {
            RunSection {
                protocol,
                outcome: SectionKind::Measured,
                reason: None,
            }
        }
    })
    .collect();
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections,
        checks: vec![
            rc(10, Status::Unmeasured),
            rc(7, Status::Pass),
            rc(1, Status::Warn),
            rc(9, Status::NotTested),
            rc(0, Status::Fail),
            rc(8, Status::NotApplicable),
            rc(2, Status::Pass),
        ],
        blocks: vec![],
    }
}

#[test]
fn passed_checks_lists_pass_then_not_applicable_then_not_tested_then_unmeasured() {
    let run = mixed_run();
    let cat = catalogue();
    let rows = passed_checks(&run, cat);

    let codes: Vec<&str> = rows.iter().map(|r| r.code.as_str()).collect();
    let expected: Vec<String> = [2, 7, 8, 9, 10]
        .iter()
        .map(|&i| id_at(i).to_string())
        .collect();
    assert_eq!(
        codes,
        expected.iter().map(String::as_str).collect::<Vec<_>>()
    );

    assert_eq!(
        rows[2].body, NA_REASON,
        "not_applicable body is the section reason"
    );
    assert_eq!(rows[4].body, "could not check", "unmeasured body");
    for (row, i) in rows.iter().zip([2, 7, 8, 9, 10]) {
        assert_eq!(row.title, cat.entries()[i].title);
    }
}

#[test]
fn every_check_appears_once_across_issues_and_passed_checks() {
    let run = mixed_run();
    let cat = catalogue();
    let groups = issues(&run, cat, Links::Live, Grouping::Owner, &BTreeMap::new());

    let mut seen: Vec<String> = groups
        .iter()
        .flat_map(|g| g.rows.iter().map(|r| r.view.code.clone()))
        .collect();
    let issue_codes = seen.clone();
    seen.extend(passed_checks(&run, cat).into_iter().map(|r| r.code));
    seen.sort();

    let mut all: Vec<String> = run.checks.iter().map(|c| c.id.to_string()).collect();
    all.sort();
    assert_eq!(seen, all, "each check exactly once");

    let mut issue_sorted = issue_codes;
    issue_sorted.sort();
    let mut want = vec![id_at(0).to_string(), id_at(1).to_string()];
    want.sort();
    assert_eq!(issue_sorted, want, "only the warn and the fail are issues");
}
