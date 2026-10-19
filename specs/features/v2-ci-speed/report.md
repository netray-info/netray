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
