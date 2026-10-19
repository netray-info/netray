#!/usr/bin/env bash
# specs/features/v2-result-page, R27: `just acceptance-local` appends the optional LENS_EXTRA
# (`KEY=VALUE ...`) to lens's environment and to no other service's, and the Playwright config
# declares the project `result-page` with testDir ./result-page. Textual shape checks on the recipe.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

command -v just >/dev/null 2>&1 || { echo "FAIL: just not found"; exit 1; }
body=$(just --show acceptance-local 2>&1) || { echo "FAIL: just --show acceptance-local failed: $body"; exit 1; }

lens_row=$(printf '%s\n' "$body" | grep -E '^ *"lens\|' | head -n 1)
[ -n "$lens_row" ] || fail "recipe has no lens row"
case "$lens_row" in
  *'${LENS_EXTRA'*) ;;
  *) fail "lens row does not expand LENS_EXTRA: $lens_row" ;;
esac

others=$(printf '%s\n' "$body" | grep -E '^ *"(dns|tls|http|email|ip)\|' | grep 'LENS_EXTRA' || true)
[ -z "$others" ] || fail "LENS_EXTRA reaches a service other than lens: $others"

total=$(printf '%s\n' "$body" | grep -c 'LENS_EXTRA')
[ "$total" -ge 1 ] || fail "recipe never mentions LENS_EXTRA"

grep -Eq "name: *'result-page'.*testDir: *'\./result-page'" tests/acceptance/playwright.config.ts \
  || fail "playwright.config.ts has no project 'result-page' with testDir './result-page'"

[ "$fails" -eq 0 ] && echo "PASS: test_acceptance_lens_extra"
exit "$fails"
