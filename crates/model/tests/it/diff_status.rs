//! Status transition table of the run diff (v2-history, R4).
use netray_model::diff::{ChangeClass, diff};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};

fn run_with(status: Status) -> Run {
    Run {
        profile_version: 1,
        engine_version: "0.26.0".to_string(),
        evidence_schema_version: 1,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("tls.chain_trusted").expect("valid id"),
            status,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

#[test]
fn status_pairs_classify_per_r4_table() {
    use ChangeClass::*;
    use Status::{
        Fail as F, NotApplicable as NA, NotTested as NT, Pass as P, Unmeasured as UM, Warn as W,
    };
    let rows: [(Status, Status, ChangeClass); 30] = [
        (P, W, New),
        (P, F, New),
        (P, NA, NoLongerApplicable),
        (P, NT, IntoNotTested),
        (P, UM, IntoNotMeasured),
        (W, P, Fixed),
        (W, F, Worse),
        (W, NA, NoLongerApplicable),
        (W, NT, IntoNotTested),
        (W, UM, IntoNotMeasured),
        (F, P, Fixed),
        (F, W, Improved),
        (F, NA, NoLongerApplicable),
        (F, NT, IntoNotTested),
        (F, UM, IntoNotMeasured),
        (NA, P, NowApplicable),
        (NA, W, NowApplicable),
        (NA, F, NowApplicable),
        (NA, NT, IntoNotTested),
        (NA, UM, IntoNotMeasured),
        (NT, P, OutOfNotTested),
        (NT, W, OutOfNotTested),
        (NT, F, OutOfNotTested),
        (NT, NA, OutOfNotTested),
        (NT, UM, IntoNotMeasured),
        (UM, P, OutOfNotMeasured),
        (UM, W, OutOfNotMeasured),
        (UM, F, OutOfNotMeasured),
        (UM, NA, OutOfNotMeasured),
        (UM, NT, IntoNotTested),
    ];
    for (from, to, class) in rows {
        let d = diff(&run_with(from.clone()), &run_with(to.clone()), &[], None);
        assert_eq!(d.checks.len(), 1, "{from:?} -> {to:?} must yield one row");
        let c = &d.checks[0];
        assert_eq!(c.id, CheckId::parse("tls.chain_trusted").unwrap());
        assert_eq!(c.class, class, "{from:?} -> {to:?}");
        assert_eq!(c.from, Some(from));
        assert_eq!(c.to, Some(to));
    }
}

#[test]
fn equal_pairs_yield_no_row_and_untested_never_fixed_or_worse() {
    let all = [
        Status::Pass,
        Status::Warn,
        Status::Fail,
        Status::NotApplicable,
        Status::NotTested,
        Status::Unmeasured,
    ];
    for from in &all {
        for to in &all {
            let d = diff(&run_with(from.clone()), &run_with(to.clone()), &[], None);
            if from == to {
                assert!(d.checks.is_empty(), "{from:?} == {to:?} must yield no row");
                continue;
            }
            assert_eq!(d.checks.len(), 1, "{from:?} -> {to:?}");
            let untested = |s: &Status| matches!(s, Status::NotTested | Status::Unmeasured);
            if untested(from) || untested(to) {
                let class = &d.checks[0].class;
                assert!(
                    *class != ChangeClass::Fixed && *class != ChangeClass::Worse,
                    "{from:?} -> {to:?} must not be {class:?}"
                );
            }
        }
    }
}
