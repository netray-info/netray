---
class: check-passes-without-observing
repeat-of: specs/solutions/2026-10-09-reason-string-satisfied-keyword-tests.md
---
# A beacon check that never finished looked like a complete result to lens

**What failed.** beacon sent category events only for tasks that finished; a panicked or missing task became `skip_result` only inside the summary verdict map (`crates/beacon/src/checks/mod.rs`). lens saw `skip` and no event, could not tell "did not run" from "nothing to report", and scored the result as complete. The spec's rule keyed on a `skipped` sub-check that never reached the wire.
**What worked.** beacon now sends the `skip_result` event before the summary (test with a resolver that panics on DKIM lookups); lens also treats a summary verdict without a category event as not run → Errored (`crates/lens/src/backends/email.rs`, `missing_category_event_errors_the_section`).
**How to notice next time.** A consumer rule that keys on a marker the producer only builds internally — check the marker is on the wire.
