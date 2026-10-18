# Spec: CI check job fits the runner disk

Status: Ready for Implementation
Created: 2026-10-18

## Goal

The `check` job of `.github/workflows/ci.yml` concludes `success` on `ubuntu-24.04-arm` with the full
`just check`, including `test-rust`'s GeoIP data path, because the job frees preinstalled runner
disk before the build and links test binaries with line tables only instead of full debuginfo.

## Non-goals

- No change to the justfile's recipes or to which tests run: nothing is excluded or ignored to save disk.
- No change to `release.yml`; the release builds in release profile inside Docker.
- No deduplication of the two feature-resolved target trees (`adlc-verify` then `test-rust`); that
  would change what the gate contains.

## Context and constraints

`specs/rules/workflow-rules.md` governs: one job (R-J1), besides setup steps only `just adlc-setup`
and `just check` (R-J2), every action pinned (R-A1). Freeing disk is a setup step. The rust-cache key
includes `CARGO_*` variables, so the first run after the change starts from a cold cache.
`tests/repo/test_workflows.sh` checks the workflow's shape; a workflow change is proven only by its
first CI run (CLAUDE.md).

## Requirements

1. The loop command `gh run list --workflow ci.yml --commit "$(git rev-parse HEAD)" --json conclusion --jq '.[0].conclusion'`
   prints `success` for the fix branch's head and, after merge, for the merge commit on `main`; the
   job log contains no `No space left on device`.
2. The `check` job sets `CARGO_PROFILE_DEV_DEBUG: line-tables-only` in its `env`.
3. A step before `just adlc-setup` removes `/usr/share/dotnet`, `/opt/ghc` and `/usr/local/lib/android`
   and prints `df -h /` before and after; it does not fail when a path is absent.
4. The justfile is unchanged: `test-rust` still takes its GeoIP branch in CI (`cargo test --workspace`
   with `crates/ip/data/GeoLite2-City.mmdb` present), and the ifconfig-rs integration tests report
   their results in the job log.

## Phase 1 — Bound the check job's disk use

**Depends on:** none
**Requirements:** 1, 2, 3, 4

`.github/workflows/ci.yml`: job-level `env` with `CARGO_PROFILE_DEV_DEBUG`, and a "Free runner disk"
step after checkout. `tests/repo/test_workflows.sh`: assertions for the env value and for the step
coming before `just adlc-setup` (under `ADLC-Test-Change`). Requirement 1 and 4 are verified on the
PR's CI run, the seam the symptom shows at; no local test reaches the hosted runner's disk.

### Test Scenarios

GIVEN `ci.yml` without `CARGO_PROFILE_DEV_DEBUG: line-tables-only` on the check job WHEN `tests/repo/test_workflows.sh` runs THEN it fails naming the missing debuginfo bound.
GIVEN `ci.yml` without a step removing the preinstalled toolchains before `just adlc-setup` WHEN `tests/repo/test_workflows.sh` runs THEN it fails naming the missing disk step.
GIVEN the fix branch pushed WHEN the PR's `CI` run completes THEN the `check` job is `success`, its log shows `df -h /` twice and the netray-ip integration test results from `cargo test --workspace`.

## Decision log

- Refuted: incremental artefacts fill the disk; `CARGO_INCREMENTAL=0` is already exported by dtolnay/rust-toolchain, so setting it again changes nothing.
- Refuted: the restored cache alone fills the disk; the green run before PR #27 restored the same ~1212 MB.
- `line-tables-only` over `0`: keeps file and line in backtraces of failing tests at a small share of full debuginfo's size (implementer under the operator's autonomy mandate, 2026-10-18).
- Freeing disk and bounding debuginfo together over either alone: the margin was crossed by PR #27's growth and will be again; two independent levers keep headroom (implementer under the operator's autonomy mandate, 2026-10-18).
- Keep both target trees over merging `adlc-verify`'s offline suite into `test-rust`: the gate's content stays as the justfile defines it (R-J2).
- Spec approved as drafted (implementer under the operator's autonomy mandate, 2026-10-18).

## Open decisions

None.

## Out of scope

The ifconfig-rs tests marked `#[ignore = "requires data/GeoLite2-City.mmdb …"]` run only with
`--include-ignored` (`just test-ifconfig-data`), not in `just check`; whether CI should run them is a
separate change.
