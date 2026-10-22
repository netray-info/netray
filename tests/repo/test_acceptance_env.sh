#!/usr/bin/env bash
# specs/features/v2-result-page R27, v2-cutover requirement 7: `just acceptance-local` appends the
# optional ACCEPTANCE_ENV (`NETRAY_KEY=VALUE ...`) to the environment of its one `netray serve`,
# and no LENS_ variable is left (serve refuses every LENS_ variable); the Playwright config declares
# the project `result-page` with testDir ./result-page. Textual shape checks on the recipe.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

command -v just >/dev/null 2>&1 || { echo "FAIL: just not found"; exit 1; }
body=$(just --show acceptance-local 2>&1) || { echo "FAIL: just --show acceptance-local failed: $body"; exit 1; }

start=$(printf '%s\n' "$body" | awk '/start_bg/ { s = 1 } s { printf "%s ", $0 } s && !/\\$/ { exit }')
[ -n "$start" ] || fail "recipe starts nothing"
case "$start" in
  *'${ACCEPTANCE_ENV'*) ;;
  *) fail "the serve start does not expand ACCEPTANCE_ENV: $start" ;;
esac
case "$start" in
  *' serve '*|*' serve') ;;
  *) fail "the start line does not run serve: $start" ;;
esac
printf '%s\n' "$body" | grep -q 'LENS_' && fail "recipe still names a LENS_ variable"

grep -Eq "name: *'result-page'.*testDir: *'\./result-page'" tests/acceptance/playwright.config.ts \
  || fail "playwright.config.ts has no project 'result-page' with testDir './result-page'"

[ "$fails" -eq 0 ] && echo "PASS: test_acceptance_env"
exit "$fails"
