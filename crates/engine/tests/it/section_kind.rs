//! The label of a section its module reported measured (v2-correctness requirement 10).

use netray_engine::measured_kind_and_reason;
use netray_model::Status;
use netray_model::run::SectionKind;

fn label(checks: &[(Status, Vec<String>)]) -> (SectionKind, Option<String>) {
    measured_kind_and_reason(checks.iter().map(|(s, f)| (*s, f.as_slice())))
}

#[test]
fn every_check_unmeasured_is_incomplete_with_the_first_finding() {
    let checks = vec![
        (
            Status::Unmeasured,
            vec!["HTTPS probe: timed out after 10s".to_string()],
        ),
        (
            Status::Unmeasured,
            vec!["CORS probe: timed out".to_string()],
        ),
    ];
    assert_eq!(
        label(&checks),
        (
            SectionKind::Incomplete,
            Some("every check unmeasured: HTTPS probe: timed out after 10s".to_string())
        )
    );
}

#[test]
fn any_measured_check_or_no_check_stays_measured() {
    let mixed = vec![
        (Status::Pass, vec![]),
        (Status::Unmeasured, vec!["timed out".to_string()]),
    ];
    assert_eq!(label(&mixed), (SectionKind::Measured, None));
    assert_eq!(label(&[]), (SectionKind::Measured, None));
}

#[test]
fn a_first_check_without_findings_gives_the_bare_reason() {
    let checks = vec![(Status::Unmeasured, vec![])];
    assert_eq!(
        label(&checks),
        (
            SectionKind::Incomplete,
            Some("every check unmeasured".to_string())
        )
    );
}
