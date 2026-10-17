---
class: check-passes-without-observing
repeat-of: specs/solutions/2026-10-15-render-tests-saw-classes-not-drawing.md
---
# A "no regression file" test passed whatever the proptest config said

**What failed.** `crates/web/tests/spectrum_layout.rs` asserted that no `proptest-regressions` directory exists after the property test ran. A passing run writes no file under any config, and proptest 1.11 writes `tests/<name>.proptest-regressions` (not a directory) when it finds no `lib.rs` above the test, so deleting `failure_persistence: None` left the test green (Phase 1 review, pass 1).
**What worked.** The config moved into `fn config()`, and the test asserts `config().failure_persistence.is_none()` and `cases == 256` directly, plus no `*.proptest-regressions` file in `tests/` (commit 37225ad).
**How to notice next time.** A test that asserts the absence of an artefact the code under test only writes on failure observes nothing on a green run.
**Check to build.** A `tests/repo` check that every `proptest!` block in the workspace takes its config from a function a plain test asserts `failure_persistence.is_none()` on.
