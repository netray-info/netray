# Spec: admission-prefetch-race

Status: Draft
Created: 2026-10-18

## Goal

A run's task takes the run out of the in-flight register before it publishes `Recorded`, so a
caller that has read a run's stream to its end and then calls `admit` never joins that finished
run: a prefetch gets `NotStarted`, a normal admit meets the throttle. The loop in `bug.md` passes
every time, and `admission_prefetch_c13` is no longer flaky.

## Non-goals

- No change to when a run is joinable before `Recorded` (sections, `Done`, recording).
- No change to the throttle, slots, budget, recompute limiter or prefetch rules.
- No change to `/api/check`'s late-join and cache fallbacks in `routes.rs`.

## Context and constraints

Rust, `crates/lens/src/admission.rs` (`run_task`, `Deregister`). The gate is `just adlc-verify`;
`cargo test -p lens` must stay green. No new dependency. The test reaches the register's private
lock, so it lives in `admission.rs`'s own `mod tests`; it uses the existing `badge_check_fn` seam
and `crate::routes::tests::test_config_with_rate_limit`.

## Requirements

1. `CARGO_INCREMENTAL=0 cargo test -p lens --lib admission::tests::recorded_follows_deregistration`
   exits 0, the test being the one written out under `## Loop` in `bug.md`: while the test holds the
   register's lock, the run's stream does not yield `Recorded` with the run still registered, and
   after the stream ends a prefetch `admit` for the domain answers `NotStarted`. Today it fails
   with `the stream ended on Recorded while the run was still joinable`.
2. The run is deregistered after `history::record` and `cache::hold` have returned and immediately
   before `Recorded` is published, so a run stays joinable through `Done` and recording exactly as
   before.
3. The `Deregister` drop guard stays the deregistration of a task that ends without publishing
   `Recorded` (panic included), and it still never removes a newer run that took the key.
4. Every existing lens test stays green unchanged (`cargo test -p lens`), and `just adlc-verify`
   passes.

## Phases

## Phase 1 — Deregister before Recorded

**Depends on:** none
**Requirements:** 1, 2, 3, 4

Add the loop's test to `mod tests` in `crates/lens/src/admission.rs`. In `run_task`, bind the
`Deregister` guard and drop it after `cache::hold` and before `tx.send_modify(|s| s.recorded = ...)`;
the guard's `Drop` (with its `same_channel` check) stays as is.

### Test Scenarios

GIVEN a started badge run whose register the test holds locked WHEN the test reads its stream for
500 ms THEN it never sees `Recorded` while the run is registered, and after the lock is released and
the stream ends, a prefetch `admit` answers `NotStarted`.
GIVEN a run in flight WHEN a prefetch `admit` arrives THEN it joins the run (existing
`admission_prefetch_c13`, `c8`, unchanged and green).

## Decision log

- Spec approved by the implementer under the operator's autonomy mandate (implementer under the
  operator's autonomy mandate, 2026-10-18); the operator pre-approved the bug specs of this run.
- Deregister before publishing `Recorded` over making join refuse a run whose stream has ended:
  one moved statement, and the register then means exactly "runs whose stream has not ended";
  a join-side check would leave ended runs registered for other readers (`registered`,
  `join_recording`) (implementer under the operator's autonomy mandate, 2026-10-18).
- A normal request that lands between deregistration and `Recorded` now meets the throttle (and
  in `/api/check` the held cache entry or `before_head`) where it used to join an ended stream;
  that is the same answer it gets after the task returns, so no new state is introduced
  (implementer under the operator's autonomy mandate, 2026-10-18).
- A unit test holding the private register lock over an integration-test loop of the race: the
  loop is red deterministically; a repeated race was red only 1 in 40 file runs
  (implementer under the operator's autonomy mandate, 2026-10-18).
- Refuted: the prefetch reaching `gate` for want of a throttle record (prefetch returns before `gate`).
- Refuted: a newer run under the same key being joined (one run, still registered after `Recorded`).
- Refuted: `history::record` or `cache::hold` publishing `Recorded` early (one write of `recorded`).

## Open decisions

None.

## Out of scope

The `/api/check` late-join logic (`crates/lens/src/routes.rs`, step 4) already covers a run that
deregisters between the head read and `admit`; it is not changed.
