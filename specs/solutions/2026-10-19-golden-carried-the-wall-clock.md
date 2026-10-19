---
class: environment-decides-the-outcome
repeat-of: specs/solutions/2026-10-18-run-ended-before-it-left-the-register.md
---
# A Rust-written golden for the islands passed all day and failed after midnight

**What failed.** `crates/web/tests/result_islands_fixtures.rs` wrote `islands/test/fixtures/result-run.sse` from a live run; the live run's trace row is dated `now()` (`utc_minute`), so the golden held the minute it was written and `adlc verify` refused the tree at 00:4x: "result-run.sse differs from the fixture".
**What worked.** The generator normalises the wall-clock minute before comparing (`20(2[6-9]|[3-9][0-9])-..-.. ..:..` → `NOW`, all fixed fixture times lie in 2025) and the fixture was regenerated (60acebd0); line sentences of a live figure use stored entries only, so no `MM-DD` of today enters a golden.
**How to notice next time.** A golden or fixture test that passes on one run and fails on the next with no code change, the diff showing a date or minute.
**Check to build.** A golden generator test that writes the fixture twice with a faked clock one day apart and asserts identical bytes (in each `crates/web/tests/result_islands_*_fixture.rs`).
