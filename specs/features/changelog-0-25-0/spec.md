# Spec: changelog 0.25.0

Status: Draft
Created: 2026-10-11

## Goal

`CHANGELOG.md`'s `## [Unreleased]` states what 0.25.0 changes for a user and a self-hoster. That means the grade shift, the new check names in lens's output, the fixed checks, the configuration rules and the known defects. With that in place, `just release 0.25.0` publishes complete release notes.

## Non-goals

- Code, config or catalogue changes.
- The release itself (`just release`, the operator's push, argus-oci's deploy).

## Context and constraints

- `[Unreleased]` is empty at a757312 (`CHANGELOG.md:9`).
- The scope of the notes:
  - The release notes state the grade changes and the new check names (v2-catalogue R4, `specs/features/v2-catalogue/spec.md`).
  - The three defects found by the R4 review are named in the 0.25.0 release notes (planning SDD `specs/sdd/v2.md` §3.9, 92d92b3).
- Measured grade shift (R4 review, v2.md §3.9): netray.info A 94.1 → A+ 97.7. Three anonymised domains moved B 86.6 → A 91.0, C 72.6 → C 75.5 and C 73.0 → C 73.8. Most of the shift comes from the split header roll-ups.
- lens keeps its JSON, SSE and snapshot shapes, and each section's check list holds the v3 names. A snapshot's `earned` is a decimal, and 0.24.0 snapshots still render (v2-catalogue R3(f)).
- What v2-check-fixes changed:
  - `http.cors`, `tls.caa_compliant` (critical flag, `issuewild`), `tls.strong_signature`, `tls.chain_trusted` (no name), `tls.chain_complete`.
  - `tls.hostname_match` is a hard fail.
  - The tlsight API gains `validation.ends_at_anchor`, and `weakest_signature` excludes roots.
  - The trust store reloads for inspections (`specs/features/v2-check-fixes/spec.md` R1–R8).
- lens refuses `resolve_timeout_ms` ≥ `[backends.tls] timeout_ms` (`crates/lens/src/config.rs:530`).
- `[scoring] profile_path` refuses a check that is not a catalogue ID (`crates/lens/src/state.rs:185`).
- `[modules.dns.backends.ip]` is accepted and ignored (v2.md §3.9 as built).
- Wording follows the 0.24.0 entry: bold lead, one paragraph per item, `### Changed` / `### Fixed` / `### Known issues`.

## Requirements

1. A lead paragraph under `## [Unreleased]` says that grades change, giving the four measured shifts (netray.info by name, the others anonymised) and their main cause.
2. `### Changed` names the new check names lens reports, for API and snapshot readers:
   - the five HTTP header checks and three hygiene checks that replace `security_headers` and `hygiene`;
   - the ten email checks that replace the four buckets;
   - `dns.infrastructure` removed;
   - all 21 TLS checks emitted in every run, `not_applicable` with a reason where they do not apply;
   - a warn earning exactly half its weight;
   - the decimal `earned`;
   - old snapshots still rendering;
   - `tls.hostname_match` as a fourth hard fail.
3. `### Changed` names the configuration rules: `resolve_timeout_ms` must stay below `[backends.tls] timeout_ms`; `[scoring] profile_path` refuses non-catalogue checks; `[modules.dns.backends.ip]` is accepted and has no effect.
4. `### Fixed` names each check fix of v2-check-fixes in one line, plus:
   - the tlsight API changes (`chain_trusted` without the name, so IP targets get a trust verdict; `ends_at_anchor`; `weakest_signature` without roots);
   - the trust-store reload.
5. `### Known issues` names the three defects, each with what a user sees: `email.mx` N/A for a working MX, the `https_redirect` timeout counting as fail, and a refused DNSBL zone counting as N/A.

## Phase 1 — Release notes

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 5

### Test Scenarios

- GIVEN `CHANGELOG.md` WHEN `[Unreleased]` is read THEN it holds a lead paragraph with the four shifts, `### Changed` with the check names and config rules, `### Fixed` with the check fixes, and `### Known issues` with the three defects.
- GIVEN the notes WHEN grepped for real domains THEN only `netray.info` appears.

## Decision log

- A changelog feature before `just release 0.25.0`, as for 0.24.0 (`specs/features/changelog-0-24-0`), over editing on the release commit: landing goes through the feature path (operator rule).
- Domains other than netray.info stay anonymised in the notes (repository rule: no real domains besides netray.info).

## Open decisions

None.

## Out of scope

- The SDD's record of the release: planning repo `specs/sdd/v2.md`.
