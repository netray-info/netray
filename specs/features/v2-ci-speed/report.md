# Report: v2-ci-speed

## Phase 1 — Compile once

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: just check runs every adlc-verify dependency except test-rust-offline, then deny and test-rust; adlc-verify unchanged | green | tests/repo/test_verbs.sh (rules 3, 4b) |
| C2 | R2: test-rust without data runs just test-rust-offline; with data cargo test --workspace, no --exclude | green | tests/repo/test_verbs_superset.sh |
| C3 | R3: test_verbs accepts test-rust for test-rust-offline only while R2 holds; any other missing dependency fails | green | tests/repo/test_verbs_superset.sh |
| C4 | GIVEN the new justfile WHEN just -n check runs THEN no --exclude cargo test, and cargo test --workspace appears | green | tests/repo/test_verbs.sh (rule 4, 4b) |
| C5 | GIVEN the new justfile WHEN just --show adlc-verify runs THEN its line is unchanged | green | tests/repo/test_verbs.sh (rule 4b: adlc-verify still runs test-rust-offline); git diff shows the line untouched |
| C6 | GIVEN a check recipe that drops fmt-check WHEN test_verbs runs THEN it fails naming fmt-check | green | tests/repo/test_verbs_superset.sh (fixtures/verbs/drops-fmt) |
| C7 | GIVEN test-rust whose no-data branch does not call test-rust-offline WHEN checked THEN it fails | green | tests/repo/test_verbs_superset.sh (fixtures/verbs/no-delegate) |
| C8 | GIVEN test-rust whose data branch excludes a crate WHEN checked THEN it fails | green | tests/repo/test_verbs_superset.sh (fixtures/verbs/excludes) |

Tests written directly (shell checks over fixture justfiles), no test-writer agents: each criterion
is a dry-run property of the justfile.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 justfile + tests/repo/lib/verbs.sh (made directly) | 0 | n/a | 0 | 0 |

### Behavioural verification

`just -n check | grep -E 'cargo test|just '` → `cargo test --workspace` and, in the no-data branch,
`just test-rust-offline`; no `--exclude` line. The CI run of the PR measures the effect (Phase 6).

## Phase 2 — netray-ip build script reruns only on git changes

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R4: a second cargo build -p netray with no change reports netray-ip fresh; only existing git paths named; without git none and both variables unknown | green | tests/repo/test_build_fresh.sh |
| C2 | GIVEN a built tree WHEN cargo build -p netray --message-format=json runs again THEN the netray-ip artifact is fresh | green | tests/repo/test_build_fresh.sh (3) |
| C3 | GIVEN the build script outside any git checkout WHEN it runs THEN no git rerun path and both variables unknown | green | tests/repo/test_build_fresh.sh (2, standalone rustc build of the script in a temp dir) |
| C4 | GIVEN the build script WHEN read THEN no relative .git rerun path | green | tests/repo/test_build_fresh.sh (1) |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 crates/ip/build.rs (made directly) | 0 | n/a | 0 | 0 |

### Behavioural verification

`cargo build -q -p netray --bin netray && cargo build -p netray --bin netray` → the second call
prints only `Finished`, no `Compiling netray-ip` (output below the phase commit's run).

## Phase 3 — ok_handlers builds its app once

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R5: ok_handlers builds the dev-config app once per binary, fresh listener per test; filtered_headers keeps its own build; no assertion changes | green | tests/repo/test_ip_app_once.sh; crates/ip/tests/ok_handlers.rs |
| C2 | GIVEN ok_handlers.rs WHEN read THEN every build_app call with the unchanged dev config sits inside the OnceCell initialiser | green | tests/repo/test_ip_app_once.sh |
| C3 | GIVEN the GeoIP data WHEN cargo test -p netray-ip --test ok_handlers runs THEN all its tests pass as before | green | crates/ip/tests/ok_handlers.rs (113 passed, 2 ignored, as before) |

The spec's first scenario said "exactly once"; `filtered_headers_excluded_from_response` builds
with its own config, so requirement 5 and the scenario were amended (decision log).

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 crates/ip/tests/ok_handlers.rs (made directly) | 0 | n/a | 0 | 0 |

### Behavioural verification

With the `ifconfig-rs-data` image's files in `crates/ip/data/` (local, gitignored):
`cargo test -p netray-ip --test ok_handlers` → before `113 passed; 0 failed; 2 ignored ... finished in 20.24s`,
after `113 passed; 0 failed; 2 ignored ... finished in 11.44s` (macOS, while another cargo build ran;
CI's baseline was 127.54 s).

## Phase 4 — mold links in CI

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R6: the check job installs mold via rui314/setup-mold pinned by SHA with an explicit mold-version, before just adlc-setup; test_workflows requires it | green | tests/repo/test_workflows.sh |
| C2 | GIVEN ci.yml with the mold step after just adlc-setup WHEN test_workflows runs THEN it fails | green | tests/repo/test_workflows.sh (order rule) |
| C3 | GIVEN ci.yml with the step pinned to @v1 WHEN test_workflows runs THEN it fails | green | tests/repo/test_workflows.sh (C10 pin rule, existing) |
| C4 | GIVEN the new ci.yml WHEN test_workflows runs THEN it passes | green | tests/repo/test_workflows.sh |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 .github/workflows/ci.yml, workflow-rules §2 (made directly) | 0 | n/a | 0 | 0 |

### Behavioural verification

skipped locally: no hosted runner here; the PR's CI run shows the setup-mold step and the link
times (Phase 6).

## Phase 5 — One integration-test binary per crate

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R7: web and lens integration tests build as one target it; lens's env-mutating files stay separate; other mechanical crates too; a repo check reads cargo metadata; adlc orphans not worsened | green | tests/repo/test_test_binaries.sh |
| C2 | R8: CLAUDE.md states the tests/it convention; commands naming moved targets updated | green | CLAUDE.md, specs/rules/frontend-rules.md, moved files' --test comments (docs commit) |
| C3 | GIVEN cargo metadata WHEN web and lens are read THEN each has target it and lens only its listed exceptions besides | green | tests/repo/test_test_binaries.sh |
| C4 | GIVEN a new file crates/web/tests/x.rs WHEN the repo check runs THEN it fails naming the file | green | tests/repo/test_test_binaries.sh (any non-it target without required-features or a listed reason fails) |
| C5 | GIVEN the merged tree WHEN cargo test --workspace runs THEN the passing count equals main's | green | local run: 2980 passed, 0 failed, 15 ignored (with GeoIP data); CI comparison under Phase 6 |
| C6 | GIVEN the merged tree WHEN adlc verify and adlc gate run THEN orphans is verified | test-unwritable | orphans was already refuted on main (frontend vitest files, tests/repo shells, lens tests/common/mod.rs: 132 files); the contract now prints ADLC-RAN for every tests/it module file, so the move adds no orphan and removes lens's common/mod.rs |

Merged: web 153, lens 76 (+ common), engine 20, model 10, store 6, dns 17, tls 20, email 7, http 15
(324 files). Kept as their own targets: `required-features = ["testing"]` files (dns 11, email 13,
tls 2, http 2), lens's `admission_config` and `history_config` (set `LENS_*`). Not merged: ip
(harness = false benches, `admin` needs its own process for the global recorder, data-dependent
tests) and common (mostly `required-features = ["fetch"]`). The move ran through a script
(`git mv`, include paths one level deeper, `mod common;` to `use crate::common;`, `--test X` to
`--test it X`), then `cargo fmt`. Comments in `crates/web/islands/test/*.ts` that cite moved paths
were left untouched (lane A owns that tree).

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 move script + justfile ADLC-RAN + docs paths (made directly) | 0 | n/a | 0 | 0 |

### Behavioural verification

`cargo test --workspace` with the data image's files in `crates/ip/data/`: every target green,
2980 passed, 0 failed, 15 ignored; `Running tests/it/main.rs` once per merged crate.
