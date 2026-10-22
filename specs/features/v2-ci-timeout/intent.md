# Intent: v2-ci-timeout

## Problem

main's CI `check` job hits its 30-minute timeout and is cancelled with no failing test (run 39217050797).

## Proposed outcome

The `check` job's `timeout-minutes` is 45 and main's CI completes.

## Affected users and systems

`.github/workflows/ci.yml` only.

## Constraints

`specs/rules/workflow-rules.md`.

## Open questions

None.
