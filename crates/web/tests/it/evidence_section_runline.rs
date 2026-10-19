//! Run line of an evidence section (spec v2-inspectors, requirement 5; C3, C10).
use netray_engine::catalogue::catalogue;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{Grade, Protocol, Status};
use netray_web::components::run_line;
use netray_web::evidence::{entry_run_line, incomplete_run_line, render_section, section};

/// 2026-10-08 14:32:47 UTC: 20_734 days since the epoch, plus 14:32:47.
const CREATED_AT: i64 = 20_734 * 86_400 + 14 * 3_600 + 32 * 60 + 47;

fn run() -> Run {
    let id = catalogue()
        .entries()
        .iter()
        .find(|e| e.protocol == Protocol::Tls)
        .expect("a tls check in the catalogue")
        .id
        .clone();
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id,
            status: Status::Pass,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

#[test]
fn stored_entry_run_line_reads_run_time_and_entry_id() {
    let line = entry_run_line(CREATED_AT, "Ab3dE5fG");
    assert_eq!(
        line.parts,
        vec![
            "run of 2026-10-08 14:32 UTC".to_string(),
            "entry Ab3dE5fG".to_string()
        ]
    );

    let html = run_line(&line).unwrap();
    assert!(html.contains("run of 2026-10-08 14:32 UTC"), "{html}");
    assert!(html.contains("entry Ab3dE5fG"), "{html}");

    let view = section(&run(), Protocol::Tls, line);
    let html = render_section(&view).unwrap();
    assert!(html.contains("run of 2026-10-08 14:32 UTC"), "{html}");
    assert!(html.contains("entry Ab3dE5fG"), "{html}");
}

#[test]
fn live_incomplete_run_line_says_not_recorded() {
    let line = incomplete_run_line();
    assert_eq!(line.parts, vec!["incomplete, not recorded".to_string()]);

    let view = section(&run(), Protocol::Tls, line);
    let html = render_section(&view).unwrap();
    assert!(html.contains("incomplete, not recorded"), "{html}");
}
