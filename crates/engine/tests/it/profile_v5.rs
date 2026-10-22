//! v2-correctness-followups Phase 3, criteria C2, C3, C10, C11, C12: profile v5 is the current
//! profile and scores exactly as v4 (the version only marks the changed `dns.https_svcb` rule);
//! v3 and v4 stay loadable and locked; a stored v4 run diffed against a v5 run carries the v4
//! run re-scored under v5; the `dns.https_svcb` catalogue texts name every branch of the check.

use netray_engine::catalogue;
use netray_engine::history::{diff, rescore};
use netray_engine::profile::{Profiles, check_lock};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};

fn profile_toml(version: u32) -> String {
    let path = format!("{}/profiles/v{version}.toml", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

#[test]
fn v5_is_current_and_scores_exactly_as_v4() {
    let v5 = Profiles::current();
    assert_eq!(v5.version, 5);
    assert_eq!(v5.name, "v5");
    let v4 = Profiles::get(4).expect("v4 loads");
    assert_eq!(v5.sections, v4.sections);
    assert_eq!(v5.thresholds, v4.thresholds);
}

#[test]
fn v3_v4_v5_load_and_match_their_locks() {
    assert!(Profiles::get(3).is_some(), "v3 loads");
    assert!(Profiles::get(4).is_some(), "v4 loads");
    assert_eq!(Profiles::versions(), vec![3, 4, 5]);
    for v in [3, 4, 5] {
        check_lock(v, &profile_toml(v)).unwrap_or_else(|e| panic!("v{v} lock: {e}"));
    }
}

fn rc(id: &str, status: Status) -> RunCheck {
    RunCheck {
        id: CheckId::parse(id).unwrap(),
        status,
        findings: vec![],
        evidence: vec![],
    }
}

fn dns_run(profile_version: u32, grade: Grade, https: Status) -> Run {
    Run {
        profile_version,
        engine_version: "0.30.0".to_string(),
        evidence_schema_version: 1,
        grade,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![rc("dns.https_svcb", https)],
        blocks: vec![],
    }
}

#[test]
fn a_v4_run_diffed_against_a_v5_run_carries_the_v4_run_rescored_under_v5() {
    let prev = dns_run(4, Grade::B, Status::Fail);
    let cur = dns_run(5, Grade::A, Status::Warn);

    let d = diff(&prev, &cur, &[], Profiles::get(5).expect("v5 loads"));

    assert_eq!(
        d.grade.rescored,
        Some(rescore(&prev, Profiles::get(5).expect("v5 loads")))
    );
}

#[test]
fn the_https_svcb_texts_name_every_branch_of_the_check() {
    let cat = catalogue();
    let e = cat
        .get(&CheckId::parse("dns.https_svcb").unwrap())
        .expect("dns.https_svcb in the catalogue");
    assert_eq!(e.title, "HTTPS record");
    assert_eq!(e.short, "HTTPS RR");
    let finding = e.finding.to_lowercase();
    for needle in ["no https or svcb record", "mix", "several alias", "warning"] {
        assert!(
            finding.contains(needle),
            "finding lacks {needle:?}: {finding}"
        );
    }
    let fix = e.fix_hint.to_lowercase();
    for needle in ["publish", "alias"] {
        assert!(fix.contains(needle), "fix_hint lacks {needle:?}: {fix}");
    }
}
