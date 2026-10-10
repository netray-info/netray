// Status mapping tests: tlsight's check statuses and error codes map onto the
// shared `netray_model::Status` vocabulary.
//
// Run with: cargo test --test model_status

use netray_model::Status;
use netray_tls::tls::status_of_error_code;
use netray_tls::validate::CheckStatus;

/// Expected mapping for every `CheckStatus` variant. The `match` is exhaustive
/// without a wildcard: adding a variant fails to compile until it is mapped here.
fn expected(status: CheckStatus) -> Status {
    match status {
        CheckStatus::Pass => Status::Pass,
        CheckStatus::Warn => Status::Warn,
        CheckStatus::Fail => Status::Fail,
        CheckStatus::Skip => Status::NotApplicable,
    }
}

#[test]
fn check_status_maps_to_model_status() {
    let all = [
        CheckStatus::Pass,
        CheckStatus::Warn,
        CheckStatus::Fail,
        CheckStatus::Skip,
    ];
    for status in all {
        assert_eq!(
            Status::from(status),
            expected(status),
            "CheckStatus::{status:?} maps to the wrong model status"
        );
    }
}

#[test]
fn error_code_maps_to_model_status() {
    let cases = [
        ("NOT_TESTED_FROM_HERE", Status::NotTested),
        ("HANDSHAKE_FAILED", Status::Unmeasured),
    ];
    for (code, want) in cases {
        assert_eq!(
            status_of_error_code(code),
            want,
            "error code {code} maps to the wrong model status"
        );
    }
}
