# Spec: changelog 0.23.0

Status: Done
Created: 2026-10-09
Finished: 2026-10-09

## Goal

`CHANGELOG.md`'s `## [Unreleased]` section lists every user-visible change of the 0.23.0 work (security-correctness SDD phases 2 to 5), so `just release 0.23.0` turns it into the release notes without further edits.

## Non-goals

- The `### Security` section: it goes in after the deploy, as for 0.22.2 (security-correctness SC2).
- Any code, config or test change.

## Context and constraints

- `just release` inserts `## [X.Y.Z] - <date>` below `## [Unreleased]` (`justfile`, recipe `release`), so the entries are written under `## [Unreleased]` without a version header.
- Format is Keep a Changelog, as the 0.22.2 entry (`CHANGELOG.md`).
- The landed features and their reports: `specs/features/{lens-goldens,advisories,grade-integrity,email-scoring,backend-correctness,parse-cursor,raw-query-policy}/`.
- R3.7's entry names the mixed-address case (grade-integrity report, amendment for the planning session).
- Neutral wording for the outbound-policy and `@system` changes (security-correctness SC2).

## Requirements

1. Each grade-affecting change has one entry under `### Changed` naming its SDD requirement (R2.x–R5.x).
2. The R3.7 entry says that a check with a refused address among public ones is no longer inspected by spectra and the result is incomplete.
3. Fixes without a grade change go under `### Fixed`.
4. No `### Security` section, and no entry says "security", "SSRF" or names an attack.

## Phase 1 — Unreleased entries

**Depends on:** none
**Requirements:** 1, 2, 3, 4

### Test Scenarios

- GIVEN the changed `CHANGELOG.md` WHEN read THEN `## [Unreleased]` holds `### Changed` and `### Fixed` and no `### Security`.
- GIVEN the `0.22.2` section WHEN diffed THEN it is unchanged.

## Decision log

- Changelog as its own feature, landed before `just release 0.23.0` (operator, 2026-10-09).
- Security notes for R5.8 and SC17 after the deploy, over now: SC2's disclosure order (operator's session instructions).

## Open decisions

None.

## Out of scope

- Tagging and pushing the release (`just release`, operator).
