//! Requirement 7: `history::rescore` scores a stored run under a profile, and `history::diff`
//! fills the grade split (`rescored`) exactly when the profile versions differ.

use netray_engine::history::{diff, rescore};
use netray_engine::profile::{Profile, Profiles};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};

fn rc(id: &str, status: Status) -> RunCheck {
    RunCheck {
        id: CheckId::parse(id).unwrap(),
        status,
        findings: vec![],
        evidence: vec![],
    }
}

fn section(protocol: Protocol, outcome: SectionKind, reason: Option<&str>) -> RunSection {
    RunSection {
        protocol,
        outcome,
        reason: reason.map(str::to_string),
    }
}

fn run(
    profile_version: u32,
    grade: Grade,
    sections: Vec<RunSection>,
    checks: Vec<RunCheck>,
) -> Run {
    Run {
        profile_version,
        engine_version: "0.26.0".to_string(),
        evidence_schema_version: 1,
        grade,
        complete: grade != Grade::Incomplete,
        sections,
        checks,
        blocks: vec![],
    }
}

/// dns: dnssec (weight 5) passes, caa (weight 5) fails: 50 % under v3, which is a D.
fn dns_half(profile_version: u32, grade: Grade) -> Run {
    run(
        profile_version,
        grade,
        vec![section(Protocol::Dns, SectionKind::Measured, None)],
        vec![rc("dns.dnssec", Status::Pass), rc("dns.caa", Status::Fail)],
    )
}

fn dns_all_pass(profile_version: u32) -> Run {
    run(
        profile_version,
        Grade::APlus,
        vec![section(Protocol::Dns, SectionKind::Measured, None)],
        vec![rc("dns.dnssec", Status::Pass), rc("dns.caa", Status::Pass)],
    )
}

/// v3 cloned as version 4 with `dns.caa` reweighted from 5 to 1: the same checks give 5/6, a B.
fn profile_b() -> Profile {
    let mut b = Profiles::current().clone();
    b.version = 4;
    let dns = b.sections.get_mut(&Protocol::Dns).unwrap();
    dns.checks.insert(CheckId::parse("dns.caa").unwrap(), 1);
    b
}

#[test]
fn profile_change_rescores_the_predecessor_under_the_current_profile() {
    let b = profile_b();
    let prev = dns_half(3, Grade::D);
    let cur = dns_all_pass(4);

    let d = diff(&prev, &cur, &[], &b);

    assert_eq!(d.grade.from, Grade::D);
    assert_eq!(d.grade.rescored, Some(Grade::B));
    assert_eq!(d.grade.rescored, Some(rescore(&prev, &b)));
    assert_ne!(d.grade.rescored, Some(prev.grade));
    assert_eq!(d.grade.to, Grade::APlus);
}

#[test]
fn same_profile_version_leaves_rescored_empty() {
    let prev = dns_half(3, Grade::D);
    let cur = dns_all_pass(3);

    let d = diff(&prev, &cur, &[], Profiles::current());

    assert_eq!(d.grade.rescored, None);
    assert_eq!(d.grade.from, Grade::D);
    assert_eq!(d.grade.to, Grade::APlus);
}

#[test]
fn rescore_under_the_run_own_profile_equals_the_stored_grade() {
    let v3 = Profiles::current();

    let measured = dns_half(3, Grade::D);
    assert_eq!(rescore(&measured, v3), measured.grade);

    // A not-applicable section counts neither way.
    let mut na = dns_all_pass(3);
    na.sections.push(section(
        Protocol::Email,
        SectionKind::NotApplicable,
        Some("no mail exchanger"),
    ));
    assert_eq!(rescore(&na, v3), na.grade);
    assert_eq!(na.grade, Grade::APlus);

    // An incomplete section makes the grade incomplete.
    let mut incomplete = dns_all_pass(3);
    incomplete.grade = Grade::Incomplete;
    incomplete.sections.push(section(
        Protocol::Http,
        SectionKind::Incomplete,
        Some("connect refused"),
    ));
    assert_eq!(rescore(&incomplete, v3), incomplete.grade);

    // A measured hard fail outranks the incomplete section.
    let hard_fail = run(
        3,
        Grade::F,
        vec![
            section(Protocol::Tls, SectionKind::Measured, None),
            section(
                Protocol::Http,
                SectionKind::Incomplete,
                Some("connect refused"),
            ),
        ],
        vec![rc("tls.chain_trusted", Status::Fail)],
    );
    assert_eq!(rescore(&hard_fail, v3), hard_fail.grade);
}
