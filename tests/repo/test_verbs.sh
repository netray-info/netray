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

# 3. adlc-verify is a subset of check
deps() { just --show "$1" 2>/dev/null | grep -m1 -E "^$1[^:=]*:" | sed -E 's/^[^:]*://' | tr ' ' '\n' | grep -v '^$'; }
check_deps=$(deps check) || true
[ -n "$(just --show check 2>/dev/null)" ] || fail "recipe 'check' does not exist"
verify_deps=$(deps adlc-verify) || true
if ! grep -qx 'adlc-verify' <<<"$check_deps"; then
    for d in $verify_deps; do
        grep -qx -- "$d" <<<"$check_deps" || fail "adlc-verify dependency '$d' is not reached from check"
    done
fi

# 4. check runs the Rust and frontend tests
dry=$(just -n check 2>&1) || fail "just -n check failed: $dry"
grep -E 'cargo test' <<<"$dry" | grep -q -- '--workspace' || fail "check does not run 'cargo test --workspace'"
grep -E 'npm (run )?test' <<<"$dry" | grep -Eq -- '(--workspaces|-ws)( |$)' || fail "check does not run npm tests with --workspaces"

# 5. no per-crate/package justfile or Makefile
stray=$(git ls-files crates packages | grep -Ei '(^|/)(justfile|makefile)$' || true)
[ -z "$stray" ] || fail "per-crate build files still tracked: $(echo "$stray" | tr '\n' ' ')"

echo "ok"
