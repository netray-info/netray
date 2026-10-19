#!/usr/bin/env bash
# v2-ci-speed R3: check may run test-rust in place of adlc-verify's test-rust-offline only while
# test-rust is a superset (data branch: cargo test --workspace, no --exclude; no-data branch:
# just test-rust-offline). Every other adlc-verify dependency must still be reached from check.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1
source tests/repo/lib/verbs.sh || { echo "FAIL: tests/repo/lib/verbs.sh missing"; exit 1; }

fx=tests/repo/fixtures/verbs
fail=0
out=$(check_reaches_verify "$fx/clean.just" 2>&1) || { echo "FAIL: clean fixture refused: $out"; fail=1; }
out=$(check_reaches_verify "$fx/drops-fmt.just" 2>&1) && { echo "FAIL: a check without fmt-check passed"; fail=1; }
grep -q 'fmt-check' <<<"$out" || { echo "FAIL: refusal does not name fmt-check: $out"; fail=1; }
check_reaches_verify "$fx/no-delegate.just" >/dev/null 2>&1 && { echo "FAIL: test-rust without the test-rust-offline delegation passed"; fail=1; }
check_reaches_verify "$fx/swapped.just" >/dev/null 2>&1 && { echo "FAIL: test-rust with swapped branches passed"; fail=1; }
check_reaches_verify "$fx/excludes.just" >/dev/null 2>&1 && { echo "FAIL: test-rust excluding a crate passed"; fail=1; }
out=$(check_reaches_verify justfile 2>&1) || { echo "FAIL: the root justfile: $out"; fail=1; }
[ $fail -eq 0 ] && echo ok
exit $fail
