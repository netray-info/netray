# Spec: CI check job in under ten minutes

Status: Ready for Implementation
Created: 2026-10-19

## Goal

The `check` job of `.github/workflows/ci.yml` finishes a warm-cache run of `just check` in about
eight to nine minutes instead of about 28, with the same tests run: the workspace and its test
binaries compile once, `netray-ip`'s build script stops rerunning on every cargo call, the
`ok_handlers` binary loads GeoLite2 once, linking goes through mold, and the web and lens
integration tests link as one binary per crate. The PR's CI run records the per-step timings
before and after.

## Non-goals

- No test is dropped, skipped or weakened; moved tests keep their assertions byte for byte.
- `adlc-verify` (the contract) keeps its recipe line and body; only `check` changes.
- No change to `release.yml` or the Dockerfile's build.
- No change to which crates' unit tests run. With GeoIP data, `check` runs them under the unified
  workspace features only; the gate's narrower feature sets stay in `adlc-verify` (decision log).

## Context and constraints

- Baseline: run 39227670650 (`main`, warm `rust-cache` hit, ~1.1 GB) took 27m30s; `just check`
  ran 09:08:30–09:34:53: `test-repo` 4m10s (each `netray_bin` call rebuilt netray-ip, netray-web,
  lens and netray, ~27 s apiece), `cargo test --workspace --exclude netray-ip` compile 7m48s,
  `test-rust`'s `cargo test --workspace` compile 8m32s, ok_handlers 127.5 s of runtime
  (scratchpad logs, analysis 2026-10-19).
- `justfile`: `check: adlc-verify deny test-rust`; `adlc-verify` runs `test-rust-offline`
  (`--exclude netray-ip`), `test-rust` runs `cargo test --workspace` when
  `crates/ip/data/GeoLite2-City.mmdb` exists. Excluding netray-ip changes the resolved features of
  45 dependencies, so the two invocations share no workspace artefacts.
- `tests/repo/test_verbs.sh` rule 3 holds that every `adlc-verify` dependency is reached from
  `check`.
- `crates/ip/build.rs` emits `cargo::rerun-if-changed=.git/HEAD` and `.git/refs/heads`, resolved
  against `crates/ip/`, where neither exists; a missing path makes cargo rerun the script on every
  call. Docker builds have no `.git` (`.dockerignore`).
- `crates/ip/tests/ok_handlers.rs` calls `netray_ip::build_app` (GeoLite2 load) in every one of its
  115 tests; the dev config's per-IP limit is burst 10, 60/min, and every test connects from
  127.0.0.1.
- Runner: `ubuntu-24.04-arm` (`ci.yml`, workflow-rules §2). Workflow-rules R-A1: every action
  pinned to a full SHA with a version comment; R-J2: only setup steps besides `just adlc-setup` and
  `just check`. `rui314/setup-mold` v1 is `339e96b4ff1f3bd865856ba4cbbe8818a7acb038`; it takes
  `mold-version` and by default links `/usr/bin/ld` to mold, so no RUSTFLAGS change is needed.
- Integration test files: web 153, lens 78 (plus `tests/common/mod.rs`), dns 28, tls 22, email 20,
  engine 20, http 17, ip 12, common 11, model 10, store 6. lens's `admission_config.rs` and
  `history_config.rs` mutate `LENS_*` environment variables, which lens's config loader reads as
  keys: in a shared process they would leak into concurrently running tests.
- adlc's orphan check counts a Rust test file as run when it is a cargo test target's `src_path`
  (`adlc/internal/collect/collect.go`); module files under a target directory are not.
- `tests/repo/test_workflows.sh` pins `CARGO_PROFILE_DEV_DEBUG: line-tables-only` and the
  "Free runner disk" step, both added because two debug test-binary trees outgrew the runner disk.
- Protected tests that move or change need `ADLC-Test-Change: <path> -- <reason>` trailers
  (AGENTS.md). Other features land on `main` in parallel and add test files.

## Requirements

1. `just check` compiles the workspace tests once: it runs every `adlc-verify` dependency except
   `test-rust-offline`, then `deny` and `test-rust`; `adlc-verify` is unchanged.
2. `test-rust` without GeoIP data runs `just test-rust-offline`; with data it runs
   `cargo test --workspace` with no `--exclude`.
3. `tests/repo/test_verbs.sh` accepts `test-rust` in place of `test-rust-offline` in `check` only
   while requirement 2 holds; any other `adlc-verify` dependency missing from `check` still fails.
4. A second `cargo build -p netray` with no source change reports `netray-ip` fresh: its build
   script names only paths that exist (the real git dir's `HEAD`, `refs/heads` and, when present,
   `packed-refs`), and without a git checkout it names none of them and still sets
   `GIT_SHORT_SHA` and `BUILD_DATE` to `unknown`.
5. `crates/ip/tests/ok_handlers.rs` builds the dev-config app once per test binary and serves it
   on a fresh listener per test; a test that needs its own config (`filtered_headers`) keeps its own
   build; no assertion changes.
6. The `check` job installs mold through `rui314/setup-mold` pinned by SHA with an explicit
   `mold-version`, before `just adlc-setup`; `tests/repo/test_workflows.sh` requires it.
7. The integration tests of `netray-web` and `lens` each build as one test target `it`
   (`tests/it/main.rs` declaring one module per former file); lens's environment-mutating files
   stay separate targets. Every other crate whose tests merge mechanically does the same. A repo
   check reads `cargo metadata` and fails when a merged crate has a test target other than `it`
   and its listed exceptions; adlc's orphan check stays green.
8. `CLAUDE.md` states that new integration tests go into `tests/it/` and gives the measured CI
   duration; commands naming a moved test target (`--test lens_golden`, `--test contract_golden`
   of a merged crate) are updated.
9. After the measured run, `timeout-minutes` drops to about twice the measured `check` duration,
   and the debuginfo bound and "Free runner disk" step are dropped only if the measured run shows
   the single build fits the disk without them.

## Phase 1 — Compile once

**Depends on:** none
**Requirements:** 1, 2, 3

`justfile`: `check` lists the `adlc-verify` dependencies with `test-rust` in place of
`test-rust-offline`; `test-rust`'s no-data branch calls `just test-rust-offline`.
`tests/repo/test_verbs.sh` rule 3 gains the superset clause.

### Test Scenarios

GIVEN the new justfile WHEN `just -n check` runs THEN `cargo test --workspace --exclude netray-ip` does not appear and `cargo test --workspace` does.
GIVEN the new justfile WHEN `just --show adlc-verify` runs THEN its line is unchanged from main.
GIVEN a check recipe that drops `fmt-check` WHEN `test_verbs.sh` runs THEN it fails naming `fmt-check`.
GIVEN a check recipe with `test-rust` whose no-data branch does not call `test-rust-offline` WHEN `test_verbs.sh` runs THEN it fails.
GIVEN a check recipe with `test-rust` whose data branch excludes a crate WHEN `test_verbs.sh` runs THEN it fails.

## Phase 2 — netray-ip build script reruns only on git changes

**Depends on:** none
**Requirements:** 4

`crates/ip/build.rs` asks git for its paths (`git rev-parse --path-format=absolute --git-path`)
and emits only those that exist. A repo check builds `netray` twice and reads cargo's JSON
messages.

### Test Scenarios

GIVEN a built tree WHEN `cargo build -p netray --message-format=json` runs again THEN the `netray-ip` compiler-artifact is fresh.
GIVEN a copy of crates/ip outside any git checkout WHEN its build script logic runs THEN it emits no rerun path under `.git` and sets both variables to `unknown`.
GIVEN the build script WHEN read THEN it emits no relative `.git/` rerun path.

## Phase 3 — ok_handlers builds its app once

**Depends on:** none
**Requirements:** 5

A `tokio::sync::OnceCell` holds the built app; the shared config raises the per-IP burst and rate so
115 requests from 127.0.0.1 are not limited (no test in the file asserts on rate limiting; the
limiter's own tests live in `rate_limit.rs`). A repo check holds the single `build_app` call.

### Test Scenarios

GIVEN ok_handlers.rs WHEN read THEN every `build_app` call with the unchanged dev config sits inside the `OnceCell` initialiser.
GIVEN the GeoIP data WHEN `cargo test -p netray-ip --test ok_handlers` runs THEN all its tests pass as before.

## Phase 4 — mold links in CI

**Depends on:** none
**Requirements:** 6

`ci.yml` gains a `rui314/setup-mold@339e96b4ff1f3bd865856ba4cbbe8818a7acb038 # v1` step with
`mold-version: 2.42.1` before `just adlc-setup`; `test_workflows.sh` requires it.

### Test Scenarios

GIVEN ci.yml with the mold step after `just adlc-setup` WHEN `test_workflows.sh` runs THEN it fails.
GIVEN ci.yml with the step pinned to `@v1` WHEN `test_workflows.sh` runs THEN it fails.
GIVEN the new ci.yml WHEN `test_workflows.sh` runs THEN it passes.

## Phase 5 — One integration-test binary per crate

**Depends on:** Phase 1
**Requirements:** 7, 8

`git mv crates/<c>/tests/<f>.rs crates/<c>/tests/it/<f>.rs`, a generated `tests/it/main.rs`, shared
helpers moved under `tests/it/`, `[[test]]` entries of merged files removed or pointed at `it`.
`tests/repo/test_test_binaries.sh` holds the layout. CLAUDE.md records the convention.

### Test Scenarios

GIVEN `cargo metadata` WHEN the web and lens packages are read THEN each has the test target `it` and, for lens, only the listed environment-mutating targets besides it.
GIVEN a new file crates/web/tests/x.rs WHEN the repo check runs THEN it fails naming the file.
GIVEN the merged tree WHEN `cargo test --workspace` runs THEN the number of passing tests equals main's.
GIVEN the merged tree WHEN `adlc verify` and `adlc gate` run THEN orphans is verified.

## Phase 6 — Tune the job to the measured run

**Depends on:** Phases 1–5
**Requirements:** 9

From the PR's CI run: lower `timeout-minutes`, drop the disk workarounds if the run shows headroom,
update `test_workflows.sh` and CLAUDE.md's CI-duration bullet with the measured numbers.

### Test Scenarios

GIVEN the measured check duration D WHEN ci.yml is read THEN `timeout-minutes` is about 2·D, rounded up to a multiple of five.
GIVEN a CI run without the disk workarounds WHEN it completes THEN `df` after `just check` shows free space left, else the workarounds stay.

## Decision log

- All five speed-ups approved (operator, 2026-10-19).
- Spec drafted and approved by the implementer under the operator's mandate for features 0–7 (operator, 2026-10-19).
- `check` lists the static verbs itself over restructuring `adlc-verify` into sub-recipes: the contract recipe's body is what adlc reads for its cargo selection, so it stays literal (implementer under the operator's autonomy mandate, 2026-10-19).
- mold through `rui314/setup-mold` over an apt package or RUSTFLAGS: the action replaces the system `ld`, so the rust-cache key (hashing `CARGO*`/`RUST*` env) is untouched; mold 2.42.1 over 3.0.0, which is only 14 days old (implementer under the operator's autonomy mandate, 2026-10-19).
- ok_handlers raises the shared app's per-IP quota over one app per test: the file asserts no rate-limit behaviour, and a shared limiter keyed on 127.0.0.1 would refuse after ten requests (implementer under the operator's autonomy mandate, 2026-10-19).
- lens's `admission_config.rs` and `history_config.rs` stay their own binaries: they set `LENS_*` variables that lens's loader reads as keys, which would leak into concurrent tests in one process (implementer under the operator's autonomy mandate, 2026-10-19).

- ok_handlers' `filtered_headers_excluded_from_response` keeps its own `build_app`: it sets `filtered_headers` on its config, which the shared app must not carry; the repo check admits a call preceded by a config assignment (implementer under the operator's autonomy mandate, 2026-10-19).

- `check` with data drops the gate's `--exclude netray-ip` / `-p netray-ip` feature sets (review, DEFERRED): accepted, because `adlc verify` runs them and every push needs its receipt; CI checks the unified set the binary ships with (implementer under the operator's autonomy mandate, 2026-10-19).
- netray-ip's build script watches HEAD, `logs/HEAD` and the branch's loose ref, not all of `refs/heads` or `packed-refs` (review AMENDMENT): a commit in another worktree rebuilt netray-ip and could turn `test_build_fresh.sh` red mid-run (implementer under the operator's autonomy mandate, 2026-10-19).
- ok_handlers' shared app runs with the `?ip=` cache off (review BLOCKER): with one app per test it never hit; shared, one test's PTR answer reached `ip_param_dns_false_skips_dns`. The production path serving a cached hostname for `dns=false` (`crates/ip/src/routes.rs`) is left for its own fix (implementer under the operator's autonomy mandate, 2026-10-19).

- lens's `result_run_c15` pauses tokio's clock after the run, not before: in the shared binary under CI load the paused clock ran the run into its deadlines (CI run 39282555459); a timing fix in the test, assertions unchanged (implementer under the operator's autonomy mandate, 2026-10-19).
- `timeout-minutes: 20` over 15 (twice the measured 5m34s, rounded up): the measured run had a warm cache; a cold one rebuilds every dependency (implementer under the operator's autonomy mandate, 2026-10-19).
- "Free runner disk" dropped, `CARGO_PROFILE_DEV_DEBUG: line-tables-only` kept: the runner had 108G free before freeing; the debuginfo bound speeds linking and is part of the rust-cache key (implementer under the operator's autonomy mandate, 2026-10-19).

## Open decisions

None.

## Out of scope

- Caching beyond `Swatinem/rust-cache` (sccache, a self-hosted runner): workflow-rules §2 setup steps.
- Splitting the job (R-J1 forbids it).
