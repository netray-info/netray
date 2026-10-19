//! Result page views: the layout input of a stored run (V2 spec v2-result-page, Phase 1,
//! requirement 15, the run mapping).
use netray_engine::catalogue::catalogue;
use netray_engine::profile::Profiles;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{Grade, Protocol, Status};
use netray_web::pages::result::views::layout_input;
use netray_web::spectrum::{SectionState, layout};

fn stored_run(profile_version: u32) -> Run {
    let statuses = [
        Status::Pass,
        Status::Warn,
        Status::Fail,
        Status::NotApplicable,
    ];
    let checks = catalogue()
        .entries()
        .iter()
        .enumerate()
        .map(|(i, e)| RunCheck {
            id: e.id.clone(),
            status: statuses[i % statuses.len()],
            findings: Vec::new(),
            evidence: Vec::new(),
        })
        .collect();
    let section = |protocol, outcome| RunSection {
        protocol,
        outcome,
        reason: None,
    };
    Run {
        profile_version,
        engine_version: "0.0.0".into(),
        evidence_schema_version: 1,
        grade: Grade::B,
        complete: false,
        sections: vec![
            section(Protocol::Dns, SectionKind::Measured),
            section(Protocol::Tls, SectionKind::NotApplicable),
            section(Protocol::Http, SectionKind::Incomplete),
            section(Protocol::Email, SectionKind::TimedOut),
        ],
        checks,
        blocks: Vec::new(),
    }
}

#[test]
fn c18_layout_input_carries_statuses_sections_and_profile() {
    let run = stored_run(3);
    let input = layout_input(&run).expect("profile 3 is known");

    assert!(std::ptr::eq(
        input.profile,
        Profiles::get(3).expect("profile 3")
    ));

    assert_eq!(input.statuses.len(), run.checks.len());
    for c in &run.checks {
        assert_eq!(input.statuses[&c.id], c.status, "{}", c.id);
    }

    assert_eq!(input.sections.len(), 4);
    assert_eq!(input.sections[&Protocol::Dns], SectionState::Measured);
    assert_eq!(input.sections[&Protocol::Tls], SectionState::NotApplicable);
    assert_eq!(input.sections[&Protocol::Http], SectionState::Unmeasured);
    assert_eq!(input.sections[&Protocol::Email], SectionState::Unmeasured);
}

#[test]
fn c6_layout_input_unknown_profile_is_none_and_known_feeds_layout() {
    assert!(layout_input(&stored_run(999)).is_none());

    let input = layout_input(&stored_run(3)).expect("profile 3 is known");
    let l = layout(&input, &[], catalogue(), 1104).expect("layout");
    assert_eq!(l.lines.len(), 54);
    assert_eq!(l.runs.len(), 1);
    assert_eq!(
        l.runs[0].sections[&Protocol::Tls],
        SectionState::NotApplicable
    );
}
