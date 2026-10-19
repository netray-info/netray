#!/usr/bin/env bash
# C4 + C11: the root justfile exposes the verbs; crates and packages keep no justfile/Makefile.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fail() { echo "FAIL: $1"; exit 1; }

# 1. verbs are listed as whole words
summary=$(just --summary 2>&1) || fail "just --summary failed: $summary"
for verb in check adlc-verify build image acceptance release; do
    grep -qw -- "$verb" <<<"$(tr ' ' '\n' <<<"$summary")" || fail "verb '$verb' missing from just --summary: $summary"
done

# 2. release takes a version argument
out=$(just -n release 9.9.9 2>&1) || fail "just -n release 9.9.9 failed: $out"
grep -q '9\.9\.9' <<<"$out" || fail "dry-run of release 9.9.9 does not mention 9.9.9: $out"
if just -n release >/dev/null 2>&1; then fail "just -n release without a version should fail"; fi

# 3. every adlc-verify dependency is reached from check (test-rust may stand in for test-rust-offline
#    while it is a superset: tests/repo/lib/verbs.sh)
[ -n "$(just --show check 2>/dev/null)" ] || fail "recipe 'check' does not exist"
source tests/repo/lib/verbs.sh || fail "tests/repo/lib/verbs.sh missing"
out=$(check_reaches_verify justfile 2>&1) || fail "$out"
verify_deps=$(recipe_deps justfile adlc-verify) || true

# 4. check runs the Rust and frontend tests
dry=$(just -n check 2>&1) || fail "just -n check failed: $dry"
grep -E 'cargo test' <<<"$dry" | grep -q -- '--workspace' || fail "check does not run 'cargo test --workspace'"
grep -E 'npm (run )?test' <<<"$dry" | grep -Eq -- '(--workspaces|-ws)( |$)' || fail "check does not run npm tests with --workspaces"

# 4b. check compiles the workspace tests once (v2-ci-speed R1): no --exclude run beside --workspace,
#     while the contract keeps its offline run.
if grep -E 'cargo test' <<<"$dry" | grep -q -- '--exclude'; then
    fail "check runs an excluding cargo test beside cargo test --workspace (two feature sets, two builds): $(grep -E 'cargo test' <<<"$dry" | tr '\n' ';')"
fi
grep -qx 'test-rust-offline' <<<"$verify_deps" || fail "adlc-verify no longer runs test-rust-offline"

# 5. no per-crate/package justfile or Makefile
stray=$(git ls-files crates packages | grep -Ei '(^|/)(justfile|makefile)$' || true)
[ -z "$stray" ] || fail "per-crate build files still tracked: $(echo "$stray" | tr '\n' ' ')"

echo "ok"
