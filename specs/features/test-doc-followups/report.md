# Report: test-doc-followups

## Phase 1 — Check self-tests that can fail

### Criteria

Scenarios are listed with the requirement they test.

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: `test_toolchain_pin.sh` takes an optional root argument; without it the root is two levels above the script; an empty or missing-directory root exits 1 naming it (scenarios: stable channel in a temp root exits 1; no argument on the real tree exits 0; bad root exits 1) | green | `tests/repo/test_toolchain_pin_selftest.sh` |
| C2 | R2: the selftest runs the repository's own check with each temp repo as root argument and copies no script (scenario: no temp repo holds a copy) | green | `tests/repo/test_toolchain_pin_selftest.sh` |
| C3 | R3: `env_other_stage` has no `cargo` and no `ENV` in the builder stage, ENV in the runtime stage, and asserts `builder stage has no ENV RUSTUP_TOOLCHAIN=stable` (scenario: fails with the stage-boundary exit removed) | green | `tests/repo/test_toolchain_pin_selftest.sh` |
| C4 | R4: `channel_stable` and `channel_short` assert their own channel refusal (scenario: fail with the channel refusal removed) | green | `tests/repo/test_toolchain_pin_selftest.sh` |
| C5 | R5: the advisories C9 self-test asserts `has no rustix package` (scenario: fails when the refusal no longer names the package) | already_implemented | `tests/repo/test_advisories.sh` |
| C6 | R6: fixture `lock-npm-happy-dom-below.json` (happy-dom 20.14.5) makes `check_npm_floors` exit non-zero printing `happy-dom 20.14.5 is older than 21.0.4`; the real lockfile still passes | already_implemented | `tests/repo/test_advisories.sh` |
| C7 | R7: `crates/common/CLAUDE.md` names `tests/repo/test_toolchain_pin.sh` in the toolchain bullet | test-unwritable | none: a doc line; the check greps the version, not the name (decision 5) |

C5 and C6 pass on the current checks: the self-tests are the deliverable and were verified to fail
under a mutated check (C5: missing-package message without the name; C3/C4 likewise for the
toolchain check). C1 to C4 are red as one file: `FAIL: channel_stable: expected failure`, because
the check ignores its root argument and checks the real tree.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 (C1-C4) | 1 | sonnet | 22219 | 11 |
| G2 (C7) | 1 | sonnet | 17289 | 8 |

### Reader

none: review reads this range

### Behavioural verification

```
$ bash tests/repo/test_toolchain_pin.sh            -> PASS: toolchain pinned to 1.99.1 ... (rc=0)
$ bash tests/repo/test_toolchain_pin.sh /nonexistent -> FAIL: root '/nonexistent' is not a directory (rc=1)
$ bash tests/repo/test_toolchain_pin.sh ""          -> FAIL: root '' is not a directory (rc=1)
$ bash tests/repo/test_toolchain_pin_selftest.sh   -> PASS: toolchain_pin check behaves (rc=0)
mutation: awk `if (in_b) exit` removed -> FAIL: env_other_stage: expected failure for ENV in another stage only
```

### Checks to build

- A `tests/repo/*_selftest.sh` never copies the `tests/repo/test_*.sh` it exercises into its fixture repos; it runs the real script with a root argument (a copy hid the missing root override of release-toolchain-pin R5). Pattern: `cp` of a `tests/repo/test_*.sh` path in a selftest; check would live in `tests/repo/test_selftests_run_real_script.sh`.
