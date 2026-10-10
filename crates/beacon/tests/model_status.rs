//! `Verdict` -> `netray_model::Status` mapping (V2 shared model).
//!
//! The `match` over `Verdict` has no wildcard arm: adding a variant breaks compilation here
//! until its mapping is decided and asserted.

use beacon::quality::Verdict;
use netray_model::Status;

fn expected(verdict: Verdict) -> Status {
    match verdict {
        Verdict::Skip => Status::NotApplicable,
        Verdict::Info => Status::Pass,
        Verdict::Pass => Status::Pass,
        Verdict::Warn => Status::Warn,
        Verdict::Fail => Status::Fail,
    }
}

#[test]
fn verdict_maps_to_model_status() {
    let all = [
        Verdict::Skip,
        Verdict::Info,
        Verdict::Pass,
        Verdict::Warn,
        Verdict::Fail,
    ];
    for verdict in all {
        assert_eq!(Status::from(verdict), expected(verdict), "{verdict:?}");
    }
}
