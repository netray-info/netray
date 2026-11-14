# Spec: test-doc-followups

Status: Ready for Implementation
Created: 2026-11-14

## Goal

Every repository check self-test that the audit-advisories-2026-11 and release-toolchain-pin reviews
found unable to fail fails when the code it names is removed, `tests/repo/test_toolchain_pin.sh`
takes a root directory argument that its self-test uses instead of copying the script, and
`crates/common/CLAUDE.md` names the check that guards the pinned toolchain; `just test-repo` stays
green on the real tree.

## Non-goals

- No change to what `test_toolchain_pin.sh` refuses on the real tree: it keeps grepping R-A2 and `crates/common/CLAUDE.md` and keeps its exact components check.
- No change to `check_floors` or `check_npm_floors` themselves, nor to their floors.
- No edit of `tests/repo/test_advisories.sh` outside its self-test block (feature/sqlx-0-9-bump owns the ignore list and its expiry check).
- No release, tag or version bump.

## Context and constraints

- `tests/repo/test_advisories.sh:235-241`: the C9 self-test passes `lock-no-rustix.lock` to `check_floors` and fails only when it exits 0 or `grep -q 'rustix'` misses the output; the failure line `FAIL: $lock has no $name package` (`:144`) prints the fixture path, which contains `rustix`, so the grep half cannot fail.
- `tests/repo/test_advisories.sh:151-175`: `check_npm_floors` floors are `happy-dom:21.0.4 vitest:4.1.12 @vitest/mocker:4.1.12` (`:174`); the below-floor line is `FAIL: <file>: <name> <v> is older than <floor>` (`:169`). Its self-tests (`:243-254`) cover a floors-met fixture and a missing-package fixture, no below-floor fixture.
- `tests/repo/test_toolchain_pin.sh:7`: the root is `cd "$(dirname "$0")/../.."`; no argument or variable overrides it, although release-toolchain-pin R5 (`specs/features/release-toolchain-pin/spec.md:70`) asked for a root directory override.
- `tests/repo/test_toolchain_pin_selftest.sh:21-41`: `make_repo` copies the check into each temp repo and `run_check` runs the copy from inside it.
- `tests/repo/test_toolchain_pin_selftest.sh:78-82`: `env_other_stage` puts `RUN cargo build` in the builder body and `ENV RUSTUP_TOOLCHAIN=stable` in the runtime stage, and asserts only a non-zero exit; the check stops its ENV scan at the first `cargo` line (`test_toolchain_pin.sh:43-52`), so the stage-boundary code is never reached.
- `tests/repo/test_toolchain_pin_selftest.sh:63-65`: the C9 cases `channel_stable` and `channel_short` assert only that the output contains `rust-toolchain.toml`, which the builder-tag, CI and named-version messages print too (`test_toolchain_pin.sh:35-80`); the channel refusal reads `rust-toolchain.toml: channel '<raw>' is not an exact X.Y.Z version` (`:22`).
- `crates/common/CLAUDE.md:7-8` names `1.99.1` and `rust-toolchain.toml`, not `tests/repo/test_toolchain_pin.sh`; release-toolchain-pin R6 (`spec.md:71-72`) asked it to name the check.
- `test_toolchain_pin.sh`, `test_toolchain_pin_selftest.sh`, `test_advisories.sh` and `tests/repo/fixtures/advisories/` are protected (added under `ADLC-Baseline:`, AGENTS.md:11-15): every change carries one `ADLC-Test-Change: <path> -- <reason>` per file.
- `just test-repo` (justfile:24-32) runs every `tests/repo/test_*.sh`, self-tests included; `just adlc-verify` runs it (justfile:10).

## Requirements

1. `tests/repo/test_toolchain_pin.sh` takes an optional first argument, the repository root it checks; without it the root stays the directory two levels above the script. A root argument that is empty or names no directory exits 1 with a message naming it and checks nothing.
2. `tests/repo/test_toolchain_pin_selftest.sh` runs the repository's own `tests/repo/test_toolchain_pin.sh` with each temp repo as the root argument, and no longer copies the script into the temp repos; its comments that describe the copy change with it.
3. The selftest's `env_other_stage` case fails when the check's builder-stage boundary exit (the awk `if (in_b) exit` on the next `FROM`, `test_toolchain_pin.sh:46`) is removed: its builder stage holds no `cargo` line and no `ENV`, the runtime stage holds `ENV RUSTUP_TOOLCHAIN=stable`, and the case asserts `builder stage has no ENV RUSTUP_TOOLCHAIN=stable`.
4. The selftest's `channel_stable` and `channel_short` cases assert their own channel refusal, `channel 'stable' is not an exact X.Y.Z version` and `channel '1.99' is not an exact X.Y.Z version`.
5. The C9 self-test of `tests/repo/test_advisories.sh` asserts `has no rustix package`, not the bare word `rustix`.
6. `tests/repo/test_advisories.sh` has a self-test with a new fixture `tests/repo/fixtures/advisories/lock-npm-happy-dom-below.json` holding happy-dom 20.14.5 and the other floors met, which requires `check_npm_floors` to exit non-zero and print `happy-dom 20.14.5 is older than 21.0.4`.
7. `crates/common/CLAUDE.md` names `tests/repo/test_toolchain_pin.sh`, in the toolchain bullet that names `1.99.1`, as the check that keeps `rust-toolchain.toml`, the builder tag and `ci.yml` in step.

## Phases

## Phase 1 — Check self-tests that can fail

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 5, 6, 7

The toolchain check takes a root argument and its self-test drives the real script against temp
repos; the four weak self-test assertions are tied to their own messages; a below-floor npm fixture
joins the advisories self-test; the crate doc names the check. `bash tests/repo/test_toolchain_pin.sh <root>`
is the boundary: its exit code and output against a fixture root.

### Test Scenarios

GIVEN a temp repo whose `rust-toolchain.toml` channel is `stable` WHEN `bash tests/repo/test_toolchain_pin.sh <temp repo>` runs from the real tree THEN it exits 1 and prints `channel 'stable' is not an exact X.Y.Z version`.
GIVEN no argument WHEN `bash tests/repo/test_toolchain_pin.sh` runs on the real tree THEN it exits 0 (the real tree still agrees).
GIVEN a root argument that names no directory, or an empty one, WHEN the check runs THEN it exits 1 and names the root, and checks nothing.
GIVEN the selftest WHEN it runs THEN no temp repo holds a copy of `test_toolchain_pin.sh`.
GIVEN the check with its builder-stage boundary handling removed WHEN the selftest runs THEN `env_other_stage` fails.
GIVEN the check with its channel refusal removed WHEN the selftest runs THEN `channel_stable` and `channel_short` fail.
GIVEN `check_floors` with its missing-package refusal replaced by a message that does not name the package WHEN `test_advisories.sh` runs THEN the C9 self-test fails.
GIVEN `lock-npm-happy-dom-below.json` with happy-dom 20.14.5 WHEN `check_npm_floors` runs on it THEN it exits 1 and prints `happy-dom 20.14.5 is older than 21.0.4`.
GIVEN the real `package-lock.json` WHEN `test_advisories.sh` runs THEN it passes (the new self-test does not touch the real check).
GIVEN `crates/common/CLAUDE.md` WHEN grepped for `tests/repo/test_toolchain_pin.sh` THEN it matches.

## Decision log

- audit-advisories-2026-11/review-3 dropped: resolver-made edge moves, no version change (operator, 2026-11-14).
- Root override as an optional first argument over an environment variable: a `NETRAY_`-style variable would need a `TOOL_VARS` entry and is easy to leak into `serve` (agent default, confirmed by operator, 2026-11-14).
- The selftest stops copying the check over keeping the copy and passing the argument too: the copy is what hid the missing override (agent default, confirmed by operator, 2026-11-14).
- release-toolchain-pin/review-3 dropped over narrowing the check: the R-A2 and `crates/common/CLAUDE.md` greps keep R6's text from drifting; this supersedes the release-toolchain-pin decision line "no check greps them" (agent default, confirmed by operator, 2026-11-14).
- release-toolchain-pin/review-4 dropped over loosening the check: R-A2 states `rustfmt` and `clippy`, the exact components line pins them (agent default, confirmed by operator, 2026-11-14).
- F2 as a doc change only over a new check that `crates/common/CLAUDE.md` names the script: the existing greps already pin the version there; the name is text for readers (agent default, confirmed by operator, 2026-11-14).
- An empty or missing-directory root argument is an error over falling back to the real tree: a failed expansion in a caller must never check the real tree silently (agent default, confirmed by operator, 2026-11-14).
- `env_other_stage` and C9 needles are the full message lines, per case, over a shared fragment: each case then fails only on its own refusal (agent default, confirmed by operator, 2026-11-14).
- Out of scope names the sqlx-0-9-bump block by its branch, not by a spec path no ref holds yet (reading fix; agent default, confirmed by operator, 2026-11-14).
- One phase over one phase per check script: all seven are test or doc edits of the same size, one review covers them (agent default, confirmed by operator, 2026-11-14).

## Open decisions

None.

## Out of scope

- Removing the RUSTSEC-2026-0459 ignore and its expiry check: the parallel work block on branch `feature/sqlx-0-9-bump` (its spec is not yet committed).
- The other open deferred rows of the repository: `.adlc/measurements/deferred.jsonl`.
