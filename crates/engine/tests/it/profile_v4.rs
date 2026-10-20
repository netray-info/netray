//! v2-tls-versions-coop Phase 3, criteria C6, C20, C21, C22: the three new catalogue entries carry
//! every text, owner and severity of their weight; a stored v3 run diffed against a v4 run carries
//! the v3 run re-scored under v4.

use netray_engine::catalogue;
use netray_engine::history::{diff, rescore};
use netray_engine::profile::Profiles;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, FixOwner, Grade, Protocol, Severity, Status};

#[test]
fn the_new_catalogue_entries_carry_every_text_owner_and_severity() {
    let cat = catalogue();
    for (id, severity, guide) in [
        ("tls.legacy_versions", Severity::High, "/guide/tls-protocol"),
        ("tls.weak_suites", Severity::Medium, "/guide/tls-protocol"),
        ("http.coop", Severity::Low, "/guide/security-headers"),
    ] {
        let e = cat
            .get(&CheckId::parse(id).unwrap())
            .unwrap_or_else(|| panic!("{id} in the catalogue"));
        for (name, v) in [
            ("title", &e.title),
            ("finding", &e.finding),
            ("impact", &e.impact),
            ("fix_hint", &e.fix_hint),
            ("source", &e.source),
            ("guide", &e.guide),
        ] {
            assert!(!v.trim().is_empty(), "{id}: {name} is empty");
        }
        assert_eq!(e.fix_owner, FixOwner::WebServer, "{id}: owner");
        assert_eq!(e.severity, severity, "{id}: severity");
        assert_eq!(e.guide, guide, "{id}: guide");
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

fn tls_run(profile_version: u32, grade: Grade, checks: Vec<RunCheck>) -> Run {
    Run {
        profile_version,
        engine_version: "0.30.0".to_string(),
        evidence_schema_version: 1,
        grade,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks,
        blocks: vec![],
    }
}

#[test]
fn a_v3_run_diffed_against_a_v4_run_carries_the_v3_run_rescored_under_v4() {
    let prev = tls_run(
        3,
        Grade::A,
        vec![
            rc("tls.tls_reachable", Status::Pass),
            rc("tls.tls_version", Status::Pass),
        ],
    );
    let cur = tls_run(
        4,
        Grade::C,
        vec![
            rc("tls.tls_reachable", Status::Pass),
            rc("tls.tls_version", Status::Pass),
            rc("tls.legacy_versions", Status::Fail),
            rc("tls.weak_suites", Status::Fail),
        ],
    );

    let d = diff(&prev, &cur, &[], Profiles::current());

    assert_eq!(
        d.grade.rescored,
        Some(rescore(&prev, Profiles::get(4).unwrap()))
    );
    assert_eq!(d.grade.from, Grade::A);
    assert_eq!(d.grade.to, Grade::C);
}
