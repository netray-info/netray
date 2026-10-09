/// Mapping of lens scoring types onto the shared `netray-model` vocabulary.
use lens::scoring::engine::{CheckVerdict, parse_grade};
use netray_model::{Grade, Status};

// C3 + C11: exhaustive over CheckVerdict (no wildcard), so a new variant breaks the build.
fn expected(v: &CheckVerdict) -> Status {
    match v {
        CheckVerdict::Pass => Status::Pass,
        CheckVerdict::Warn => Status::Warn,
        CheckVerdict::Fail => Status::Fail,
        CheckVerdict::NotFound => Status::Fail,
        CheckVerdict::Skip => Status::NotApplicable,
    }
}

#[test]
fn check_verdict_maps_to_status() {
    let all = [
        CheckVerdict::Pass,
        CheckVerdict::Warn,
        CheckVerdict::Fail,
        CheckVerdict::NotFound,
        CheckVerdict::Skip,
    ];
    for v in all {
        let want = expected(&v);
        let got = Status::from(v.clone());
        assert_eq!(got, want, "verdict {v:?}");
    }
}

#[test]
fn parse_grade_maps_known_grades() {
    let cases = [
        ("A+", Some(Grade::APlus)),
        ("A", Some(Grade::A)),
        ("B", Some(Grade::B)),
        ("C", Some(Grade::C)),
        ("D", Some(Grade::D)),
        ("F", Some(Grade::F)),
        ("incomplete", Some(Grade::Incomplete)),
        ("E", None),
    ];
    for (input, want) in cases {
        assert_eq!(parse_grade(input), want, "input {input:?}");
    }
}
