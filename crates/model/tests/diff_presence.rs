//! Contract tests for check presence in the run diff (spec v2-history, Phase 1, requirements 5 and 6).
use netray_model::diff::{ChangeClass, Diff, diff};
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};

fn cid(s: &str) -> CheckId {
    CheckId::parse(s).expect("valid check id")
}

fn section(protocol: Protocol, outcome: SectionKind) -> RunSection {
    RunSection {
        protocol,
        outcome,
        reason: None,
    }
}

fn run(
    engine: &str,
    complete: bool,
    sections: Vec<RunSection>,
    checks: &[(&str, Status)],
    blocks: Vec<Block>,
) -> Run {
    Run {
        profile_version: 1,
        engine_version: engine.into(),
        evidence_schema_version: 1,
        grade: if complete {
            Grade::A
        } else {
            Grade::Incomplete
        },
        complete,
        sections,
        checks: checks
            .iter()
            .map(|(i, s)| RunCheck {
                id: cid(i),
                status: s.clone(),
            })
            .collect(),
        blocks,
    }
}

fn block(name: &str) -> Block {
    Block {
        id: BlockId::parse(name).unwrap(),
        title: "t".into(),
        shape: Shape::Fields(vec![("k".to_string(), Value::Text("v".into()))]),
    }
}

fn class_of(d: &Diff, id: &str) -> Vec<ChangeClass> {
    d.checks
        .iter()
        .filter(|c| c.id == cid(id))
        .map(|c| c.class.clone())
        .collect()
}

#[test]
fn check_only_in_prev_is_removed_and_engine_changed_tracks_versions() {
    let secs = || vec![section(Protocol::Dns, SectionKind::Measured)];
    let prev = run(
        "0.26.0",
        true,
        secs(),
        &[("dns.a", Status::Pass), ("dns.old", Status::Pass)],
        vec![],
    );

    let same = run("0.26.0", true, secs(), &[("dns.a", Status::Pass)], vec![]);
    let d = diff(&prev, &same, &[], None);
    assert_eq!(class_of(&d, "dns.old"), vec![ChangeClass::Removed]);
    assert!(!d.engine_changed);

    let newer = run("0.27.0", true, secs(), &[("dns.a", Status::Pass)], vec![]);
    let d = diff(&prev, &newer, &[], None);
    assert_eq!(class_of(&d, "dns.old"), vec![ChangeClass::RemovedByRelease]);
    assert!(d.engine_changed);
}

#[test]
fn check_only_in_current_is_added_or_added_by_release() {
    let secs = || vec![section(Protocol::Dns, SectionKind::Measured)];
    let prev = run("0.26.0", true, secs(), &[("dns.a", Status::Pass)], vec![]);
    let cur_checks = [("dns.a", Status::Pass), ("dns.new", Status::Pass)];

    let same = run("0.26.0", true, secs(), &cur_checks, vec![]);
    let d = diff(&prev, &same, &[], None);
    assert_eq!(class_of(&d, "dns.new"), vec![ChangeClass::Added]);
    assert!(!d.engine_changed);

    let newer = run("0.27.0", true, secs(), &cur_checks, vec![]);
    let d = diff(&prev, &newer, &[], None);
    assert_eq!(class_of(&d, "dns.new"), vec![ChangeClass::AddedByRelease]);
    assert!(d.engine_changed);
}

#[test]
fn not_applicable_section_becoming_measured_is_now_applicable_not_added_by_release() {
    let prev = run(
        "0.26.0",
        true,
        vec![section(Protocol::Tls, SectionKind::NotApplicable)],
        &[
            ("tls.not_expired", Status::NotApplicable),
            ("tls.chain_trusted", Status::NotApplicable),
        ],
        vec![],
    );
    let cur = run(
        "0.27.0",
        true,
        vec![section(Protocol::Tls, SectionKind::Measured)],
        &[
            ("tls.not_expired", Status::Pass),
            ("tls.chain_trusted", Status::Fail),
        ],
        vec![],
    );
    let d = diff(&prev, &cur, &[], None);
    assert!(d.engine_changed);
    assert_eq!(d.checks.len(), 2);
    for id in ["tls.not_expired", "tls.chain_trusted"] {
        assert_eq!(class_of(&d, id), vec![ChangeClass::NowApplicable], "{id}");
    }
}

#[test]
fn section_going_incomplete_marks_checks_into_not_measured_and_keeps_evidence_quiet() {
    let prev = run(
        "0.26.0",
        true,
        vec![section(Protocol::Email, SectionKind::Measured)],
        &[("email.spf", Status::Pass), ("email.dmarc", Status::Warn)],
        vec![block("email.spf"), block("email.dmarc")],
    );
    let cur = run(
        "0.26.0",
        false,
        vec![section(Protocol::Email, SectionKind::Incomplete)],
        &[
            ("email.spf", Status::Unmeasured),
            ("email.dmarc", Status::Unmeasured),
        ],
        vec![],
    );
    let d = diff(&prev, &cur, &[], None);
    for id in ["email.spf", "email.dmarc"] {
        assert_eq!(class_of(&d, id), vec![ChangeClass::IntoNotMeasured], "{id}");
    }
    assert!(
        d.evidence.is_empty(),
        "no email block may be reported Removed: {:?}",
        d.evidence
    );
}
