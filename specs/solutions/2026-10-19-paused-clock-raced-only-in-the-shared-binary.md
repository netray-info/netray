---
class: environment-decides-the-outcome
repeat-of: specs/solutions/2026-10-14-shared-temp-dir-raced-only-in-ci.md
---
# A paused-clock test that was green in its own binary failed once 385 tests shared its process

**What failed.** After v2-ci-speed merged lens's integration tests into one binary, CI run
39282555459 failed `result_run_admission::result_run_c15_a_run_20s_ago_is_refused_throttle_with_the_run_line`
(200 instead of 429). The test paused tokio's clock before a run that writes to a `HistoryStore`;
under load the runtime idled on the store's blocking thread, the paused clock auto-advanced through
the modules' deadlines, the run came out incomplete and got the shorter throttle window.
Reproduced locally 1 in 3 with `RUST_TEST_THREADS=64` and 14 `yes` processes.
**What worked.** Pause after `h.state.runs.drain().await`, then `advance(20 s)`
(crates/lens/tests/it/result_run_admission.rs); 0 failures in 9 loaded runs.
**How to notice next time.** A test using `pause()`/`start_paused` beside real I/O passes alone and
fails only in a busy process: rerun it under CPU load before trusting it.
**Check to build.** In `crates/*/tests/**`, `tokio::time::pause()` or `start_paused` in a test function that opens a `HistoryStore` before the pause: a tests/repo check (sg pattern).
