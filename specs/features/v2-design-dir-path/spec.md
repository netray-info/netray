# Spec: v2-design-dir-path

Status: Done
Created: 2026-10-20
Finished: 2026-10-20

## Goal

`just design-check` finds the private design canvas at its new home by default. The canvas (HTML
export, boards, `manifest.json`) moved from the netray-ops checkout
(`netray-ops/specs/netray/design/canvas`) to the private netray.info checkout
(`netray.info/specs/design/canvas`, private). The justfile default of `NETRAY_DESIGN_DIR` and
`specs/rules/design-rules.md` name the new location; a repository check holds them together.

## Non-goals

- No change to the design check's method, manifest format or tolerances.
- No edit to finished features' documents under `specs/features/*/` that name the old path: they
  are history (the manifest commits they cite live in netray-ops).
- No change to `.claude/settings.local.json` (machine-local permissions).

## Context and constraints

- The justfile resolves the default beside the main checkout
  (`$main_checkout/../netray-ops/specs/netray/design/canvas`), so a worktree finds it too
  (v2-design-gate report, AMENDMENT on the worktree default).
- `specs/rules/design-rules.md` lines 5 and 21 name the netray-ops checkout and the old default.
- `tests/repo/test_design_rules.sh` checks the rules file's method terms; no test pins the default
  path today.

## Requirements

1. The `design-check` recipe defaults `NETRAY_DESIGN_DIR` to
   `$main_checkout/../netray.info/specs/design/canvas`; an explicit `NETRAY_DESIGN_DIR` still wins.
2. `specs/rules/design-rules.md` names the netray.info checkout under `specs/design/canvas/` as
   the canvas location and `../netray.info/specs/design/canvas` as the default.
3. Neither the justfile nor `specs/rules/design-rules.md` names `netray-ops` or
   `specs/netray/design` any more.

## Phase 1 — Default design directory in netray.info

Requirements 1–3. The new `tests/repo/test_design_dir.sh` holds the checks; the justfile and the rules
file change.

### Test Scenarios

GIVEN the justfile WHEN the `design-check` recipe's `NETRAY_DESIGN_DIR` default is read THEN it is
`$main_checkout/../netray.info/specs/design/canvas`, kept behind the `${NETRAY_DESIGN_DIR:-...}`
expansion.

GIVEN `specs/rules/design-rules.md` WHEN its location text is read THEN it names
`../netray.info/specs/design/canvas` and the netray.info checkout.

GIVEN the justfile and `specs/rules/design-rules.md` WHEN they are searched for `netray-ops` or
`specs/netray/design` THEN neither holds a match.

GIVEN the main checkout beside `netray.info` WHEN `just design-check` runs without arguments or
`NETRAY_DESIGN_DIR` THEN it reads `netray.info/specs/design/canvas/manifest.json` (behavioural
check, recorded in report.md).

## Decision log

- 2026-10-20: Spec drafted and approved by the implementer under the operator's mandate for
  features 0–7 (operator, 2026-10-19).
- 2026-10-20: Finished features' documents keep the old path (history, not instructions);
  implementer under the operator's autonomy mandate, 2026-10-19.
- 2026-10-20: Distil at finish: no entries written; the one lesson (a repo check scoped to a recipe body misses its header comment) was repaired in phase and fits no rule or check beyond this test. Implementer under the operator's autonomy mandate, 2026-10-19.

## Open decisions

None.

## Out of scope

- Moving or editing the canvas itself (done in netray.info).
