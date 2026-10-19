---
class: environment-decides-the-outcome
repeat-of: specs/solutions/2026-10-14-shared-temp-dir-raced-only-in-ci.md
---
# A run's stream ended before the run left the in-flight register

**What failed.** `admission_prefetch_c13` failed about 1 in 15 whole-file runs and never alone
(100 runs green): `run_task` published `Recorded` and only then dropped its `Deregister` guard, so
a reader that had consumed the stream could `admit` into the gap and join the finished run
(`crates/lens/src/admission.rs`, `run_task`). Thread scheduling under parallel tests decided the
outcome; a 200-round loop of the race was red only 1 run in 40.
**What worked.** A unit test in `admission.rs`'s own `mod tests` that holds the private register
lock (`state.runs.lock()`) while it reads the stream makes the order of deregistration and
`Recorded` observable, red every run; the fix drops the guard right before the `Recorded` write.
**How to notice next time.** A test that passes alone and fails only inside its file's parallel
run reads shared state one task publishes in two steps; hold the lock between the steps instead
of looping.
