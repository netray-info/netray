//! Enrichment failures must not score as healthy (spec grade-integrity; criteria C5, C6).
//!
//! A failed or stalled IP lookup makes the IP section Errored
//! (`Err(SectionError::BackendError | Timeout)`), which is what makes the overall result
//! incomplete. In-process the lookup failure is the module's `Incomplete` outcome and a stall
//! is a module that does not finish within lens's section timeout; both are asserted through
//! `ModuleSection`, with small local modules in place of the 500 and the sleeping stub.
//!
//! Not kept at lens level: C5/C6 "one of two lookups fails" (a golden module answers every
//! address alike, so one of two cannot fail here); `netray_ip`'s `translate` test
//! (`translate_failed_lookup_is_incomplete`) owns that rule.

mod common;

use std::time::Duration;

use common::{ip_golden, ip_incomplete, run_ip, slow};
use lens::check::SectionError;

const PUBLIC: [&str; 2] = ["1.1.1.1", "8.8.8.8"];

fn is_errored<T>(r: &Result<T, SectionError>) -> bool {
    matches!(
        r,
        Err(SectionError::BackendError(_) | SectionError::Timeout)
    )
}

#[tokio::test]
async fn c5_a_failed_enrichment_errors_the_ip_section() {
    let run = run_ip(ip_incomplete(), Duration::from_secs(5), &PUBLIC).await;
    assert!(is_errored(&run), "IP section must be Errored");
}

#[tokio::test]
async fn c6_a_stalled_enrichment_errors_the_ip_section() {
    let stalled = slow(ip_golden("ifconfig-json.json"), None);
    let run = run_ip(stalled, Duration::from_millis(300), &PUBLIC).await;
    assert!(
        matches!(run, Err(SectionError::Timeout)),
        "IP section must be Errored by timeout"
    );
}

#[tokio::test]
async fn a_healthy_enrichment_does_not_error_the_ip_section() {
    let run = run_ip(
        ip_golden("ifconfig-json.json"),
        Duration::from_secs(5),
        &PUBLIC,
    )
    .await;
    assert!(run.is_ok(), "the golden module answers every address");
}
