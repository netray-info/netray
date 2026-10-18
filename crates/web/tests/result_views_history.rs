//! The history sentence and the issue "since" of the result page (v2-result-page requirement 16).

use netray_model::Status;
use netray_web::pages::result::views::{history_sentence, issue_since};

// Noon UTC on 2026-09-19, 2026-09-24, 2026-10-08, 2026-10-12.
const D0919: i64 = 1_789_819_200;
const D0924: i64 = 1_790_251_200;
const D1008: i64 = 1_791_460_800;
const D1012: i64 = 1_791_806_400;

fn warn_then_pass() -> Vec<(i64, Status)> {
    vec![
        (D0919, Status::Warn),
        (D0924, Status::Warn),
        (D1008, Status::Pass),
        (D1012, Status::Pass),
    ]
}

#[test]
fn history_sentence_collapses_equal_statuses_into_spans() {
    assert_eq!(
        history_sentence(&warn_then_pass()),
        "warn 09-19 to 09-24 · pass since 10-08"
    );
}

#[test]
fn issue_since_is_the_start_of_the_current_span() {
    assert_eq!(issue_since(&warn_then_pass()), Some("10-08".to_string()));
}

#[test]
fn issue_since_is_none_when_the_span_covers_every_point() {
    let equal = vec![
        (D0919, Status::Warn),
        (D0924, Status::Warn),
        (D1008, Status::Warn),
        (D1012, Status::Warn),
    ];
    assert_eq!(issue_since(&equal), None);
}
