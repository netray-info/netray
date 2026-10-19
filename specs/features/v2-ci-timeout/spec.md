# Spec: CI check job timeout fits the suite

Status: Done
Created: 2026-10-19
Finished: 2026-10-19

## Goal

The `check` job of `.github/workflows/ci.yml` runs with `timeout-minutes: 45`, so a cold-cache run
of the full `just check` completes instead of being cancelled at 30 minutes.

## Non-goals

- No change to the justfile, to which tests run, or to the job's steps.
- No change to `release.yml`.
- No build-time optimisation of the suite.

## Context and constraints

- Run 39217050797 on `main` at 3ba0f1a9 (PR #31's merge) was cancelled twice; attempt 2's `check`
  job ran 08:07:16–08:37:33 UTC, exactly into its 30-minute limit, with no failing test.
- Earlier green runs took 24–26 min (39161277261: 26m24s); CLAUDE.md records a cold-cache run of
  25m51s of the 30-minute timeout. PR #31 added tests on top of that.
- `specs/rules/workflow-rules.md` governs the workflow's shape; it fixes no timeout value.
  `tests/repo/test_workflows.sh` does not pin `timeout-minutes`.
- A workflow change is verified by its first CI run (CLAUDE.md).

## Requirements

1. The `check` job in `.github/workflows/ci.yml` sets `timeout-minutes: 45`; nothing else in the
   workflow changes.

## Phase 1 — Raise the check job's timeout

**Depends on:** none
**Requirements:** 1

One-line change in `.github/workflows/ci.yml`. The behaviour (a run completing past 30 minutes) is
verified by the PR's CI run and the merge commit's run on `main`; no local test reaches the hosted
runner.

### Test Scenarios

GIVEN `ci.yml` WHEN read THEN the `check` job's `timeout-minutes` is 45.
GIVEN `ci.yml` before and after the change WHEN diffed THEN the only changed line is `timeout-minutes`.

## Decision log

- 45 over 40 or 60: about 1.7x the observed cold run, room for suite growth without letting a hung
  job hold a runner for an hour (operator, 2026-10-19).
- No repo check pinning the value: the timeout is a tuning knob, not a shape rule (implementer under
  the operator's autonomy mandate, 2026-10-19).
- Spec approved as drafted, independent reading skipped for a one-line change (implementer under the
  operator's autonomy mandate, 2026-10-19).
- Finish: CLAUDE.md's CI-duration bullet updated to the 45-minute limit (review minor); no check to build, no solutions note: a tuning value, not a mechanical lesson (implementer under the operator's autonomy mandate, 2026-10-19).

## Open decisions

None.

## Out of scope

Shortening the suite (the double test-binary build of `adlc-verify` plus `test-rust`) lives in the
root `justfile` and would change what the gate contains.
