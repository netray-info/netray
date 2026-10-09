# Spec: security notes 0.23.0

Status: Ready for Implementation
Created: 2026-10-09

## Goal

`CHANGELOG.md`'s `## [0.23.0]` section carries a `### Security` subsection describing the two issues 0.23.0 fixed, published after the deploy as 0.22.2's note was (security-correctness SC2).

## Non-goals

- Changing prism's `allow_system_resolvers` default (`crates/mhost-prism/src/config.rs:198`, still `true`).
- Any code, config or test change.

## Context and constraints

- 0.23.0 is deployed and prod acceptance is green (security-correctness SDD, "Phases 2–5 result", planning repo).
- R5.8 (SDD): prism's NS checks, `authcompare` and the DNSSEC chain walk sent raw DNS queries to addresses from the checked domain's NS and glue data with no target policy; the walk returned the records; missing glue was resolved through the system resolver, which in a container answers service names with container addresses. Fixed by `raw-query-policy` (PR #6).
- SC17 (SDD): `@system` queries the container's `/etc/resolv.conf`, Docker's embedded resolver, which answers service names with container addresses. Production sets `allow_system_resolvers = false`; the UI and `/api/parse` offer `@system` only when allowed (R5.9, `backend-correctness`). The default stays `true` (`config.rs:198`).
- Format follows the 0.22.2 `### Security` entry: what was affected, what it could do, what changed, who is affected, what to do.

## Requirements

1. `## [0.23.0]` gains `### Security` after `### Fixed`, with one entry for the raw DNS queries (R5.8) and one for `@system` (SC17).
2. The R5.8 entry names the three query paths and the glue resolution, says a refused address is never queried, and says every earlier release is affected.
3. The SC17 entry says that the fix for a containerised deployment is `[dns] allow_system_resolvers = false`, and that the default stays `true`.
4. No other section of `CHANGELOG.md` changes.

## Phase 1 — Security section

**Depends on:** none
**Requirements:** 1, 2, 3, 4

### Test Scenarios

- GIVEN the changed `CHANGELOG.md` WHEN read THEN `## [0.23.0]` holds `### Changed`, `### Fixed`, `### Security` in that order.
- GIVEN `git diff main -- CHANGELOG.md` WHEN read THEN it only adds lines inside `## [0.23.0]`.

## Decision log

- Notes after the deploy, as for 0.22.2 (operator's session instructions; SC2).
- Advise `allow_system_resolvers = false` over flipping the default in this feature: a default change is a code change and belongs in its own feature.

## Open decisions

None.

## Out of scope

- The security-correctness SDD's finish (planning repo).
