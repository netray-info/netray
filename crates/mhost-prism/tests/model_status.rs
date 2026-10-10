// Mapping tests: mhost lint results map onto the shared netray-model status vocabulary.
//
// The orphan rule forbids `From<CheckResult> for Status`, so prism exposes
// `prism::lint_status`. The exhaustive match below (no wildcard) makes a new
// `CheckResult` variant a compile error here until its mapping is decided.
//
// Run with: cargo test --test model_status

use mhost::lints::CheckResult;
use netray_model::Status;
use prism::lint_status;

/// Expected status per variant. No wildcard arm: a new variant must fail to compile.
fn expected(r: &CheckResult) -> Status {
    match r {
        CheckResult::NotFound() => Status::Fail,
        CheckResult::Ok(_) => Status::Pass,
        CheckResult::Warning(_) => Status::Warn,
        CheckResult::Failed(_) => Status::Fail,
    }
}

#[test]
fn lint_status_maps_every_check_result_variant() {
    let rows = vec![
        (CheckResult::NotFound(), Status::Fail),
        (CheckResult::Ok("fine".into()), Status::Pass),
        (CheckResult::Warning("careful".into()), Status::Warn),
        (CheckResult::Failed("broken".into()), Status::Fail),
    ];

    for (input, want) in rows {
        assert_eq!(
            expected(&input),
            want,
            "table disagrees with exhaustive match for {input:?}"
        );
        assert_eq!(lint_status(&input), want, "wrong status for {input:?}");
    }
}
