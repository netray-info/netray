// spectra's quality `CheckStatus` maps onto the shared `netray_model::Status`.
//
// `expected` matches every `CheckStatus` variant without a wildcard, so adding a variant
// breaks this file's compilation until its mapping is decided here.

use netray_http::quality::types::CheckStatus;
use netray_model::Status;

fn expected(status: &CheckStatus) -> Status {
    match status {
        CheckStatus::Pass => Status::Pass,
        CheckStatus::Skip => Status::NotApplicable,
        CheckStatus::Warn => Status::Warn,
        CheckStatus::Fail => Status::Fail,
    }
}

#[test]
fn check_status_maps_to_model_status() {
    let all = [
        CheckStatus::Pass,
        CheckStatus::Skip,
        CheckStatus::Warn,
        CheckStatus::Fail,
    ];
    for status in all {
        let want = expected(&status);
        let got: Status = status.clone().into();
        assert_eq!(got, want, "mapping for {status:?}");
    }
}
