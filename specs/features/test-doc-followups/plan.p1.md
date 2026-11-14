## Phase 1 — Check self-tests that can fail

## Groups
G1: C1, C2, C3, C4 · G2: C7

C5 and C6 are already_implemented (`tests/repo/test_advisories.sh` self-tests and fixture `tests/repo/fixtures/advisories/lock-npm-happy-dom-below.json` are in 05681d24): no change is planned for them.

## Plan
### G1
1. `tests/repo/test_toolchain_pin.sh` — change — replace the fixed root at line 7 with an optional first argument: when `$#` is at least 1, an empty `$1` or one that is not a directory prints a `FAIL:` line naming the root (e.g. `FAIL: root '<arg>' is not a directory`) and exits 1 before any check runs, otherwise `cd "$1"`; with no argument keep `cd "$(dirname "$0")/../.."`. Add the argument to the header comment (lines 2-5). Nothing else in the file changes (non-goal: same refusals on the real tree). Commit with `ADLC-Test-Change: tests/repo/test_toolchain_pin.sh -- R1: optional repository root argument` (protected file) — hangs on the root `cd`, `tests/repo/test_toolchain_pin.sh:7`.
2. `tests/repo/test_toolchain_pin_selftest.sh` — no change — already passes the temp repo as argument (`run_check`, :41), copies no script (`make_repo`, :22-35; copy check :102-103), asserts the full channel refusals (:64-65) and the builder-stage needle with a cargo-free builder body (`env_other_stage`, :80-86), and the missing/empty root cases (:94-99). C2, C3, C4 go green once step 1 lands; they hang on `run_check`, :41 and the stage-boundary exit `if (in_b) exit`, `tests/repo/test_toolchain_pin.sh:46`.
3. Verify: `bash tests/repo/test_toolchain_pin_selftest.sh` prints PASS; `bash tests/repo/test_toolchain_pin.sh` with no argument exits 0 on the real tree (`just test-repo`, justfile:24-32, calls it without arguments). Mutation spot checks from the scenarios: drop `if (in_b) exit;` at :46 and `env_other_stage` fails; drop the `fail` at :22 and `channel_stable`/`channel_short` fail; revert both before committing.

### G2
1. `crates/common/CLAUDE.md` — change — in the toolchain bullet (lines 7-8, the one naming `1.99.1`), name `tests/repo/test_toolchain_pin.sh` as the check that keeps `rust-toolchain.toml`, the builder tag and `ci.yml` in step; keep `1.99.1` and avoid the text `the toolchain is \`stable\`` (both grepped by `tests/repo/test_toolchain_pin.sh:76-78`) — hangs on the existing bullet, `crates/common/CLAUDE.md:7`.
2. Verify: `grep -F tests/repo/test_toolchain_pin.sh crates/common/CLAUDE.md` matches and `bash tests/repo/test_toolchain_pin.sh` still exits 0.
