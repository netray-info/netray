#!/usr/bin/env bash
# The deterministic supply-chain checks (cargo-deny bans, licenses, sources) run
# in `just check` against one root deny.toml, and CI installs cargo-deny for it.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

git ls-files --error-unmatch deny.toml >/dev/null 2>&1 || fail "no root deny.toml tracked"

nested=$(git ls-files | grep -E '^(crates|packages)/.+/deny\.toml$' | tr '\n' ' ')
[ -z "$nested" ] || fail "per-crate deny.toml still tracked: $nested"

dry=$(just -n check 2>&1) || fail "just -n check failed"
echo "$dry" | grep -q 'cargo deny' || fail "just check does not run cargo deny"
echo "$dry" | grep -qE 'check bans licenses sources' || fail "just check does not run 'cargo deny check bans licenses sources'"

[ -f .github/workflows/ci.yml ] || fail ".github/workflows/ci.yml missing"
grep -q 'cargo-deny' .github/workflows/ci.yml 2>/dev/null || fail "ci.yml does not install cargo-deny"

[ "$fails" -eq 0 ] || exit 1
echo "PASS: cargo-deny bans/licenses/sources in just check"
