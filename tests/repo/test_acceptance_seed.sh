#!/usr/bin/env bash
# Requirement 23 / Phase 8 (v2-inspectors), v2-cutover requirement 7: `just acceptance-local` seeds
# the temporary history.db before `netray serve` starts, aborts when the seed fails, gives serve that
# store, and forwards its arguments to Playwright. Textual shape checks on the recipe body.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fail() { echo "FAIL: $1"; exit 1; }

command -v just >/dev/null 2>&1 || fail "just not found"
body=$(just --show acceptance-local 2>&1) || fail "just --show acceptance-local failed: $body"

# (a) the seed runs with the db path serve gets as NETRAY_STORE__DB_PATH and the fixture dir
db_path=$(printf '%s\n' "$body" | grep -o 'NETRAY_STORE__DB_PATH="[^"]*"' | head -n 1 | cut -d= -f2- | tr -d '"')
[ -n "$db_path" ] || fail "recipe gives serve no NETRAY_STORE__DB_PATH"
seed_line=$(printf '%s\n' "$body" | grep -n 'cargo run' | grep -e '-p netray-store' | grep -e '--example seed' | head -n 1)
[ -n "$seed_line" ] || fail "recipe has no 'cargo run -p netray-store --example seed' line"
seed_no=${seed_line%%:*}
case "$seed_line" in
  *"$db_path"*) ;;
  *) fail "seed line does not use serve's db path $db_path: $seed_line" ;;
esac
case "$seed_line" in
  *tests/acceptance/fixtures/history*) ;;
  *) fail "seed line does not name tests/acceptance/fixtures/history: $seed_line" ;;
esac

# (b) the seed comes before serve starts
serve_no=$(printf '%s\n' "$body" | grep -n 'start_bg' | head -n 1 | cut -d: -f1)
[ -n "$serve_no" ] || fail "recipe starts nothing"
[ "$seed_no" -lt "$serve_no" ] || fail "seed (line $seed_no) does not come before serve starts (line $serve_no)"

# (c) a failing seed aborts the recipe
case "$seed_line" in
  *'|| {'*exit\ 1*|*'|| exit'*) ;;
  *) fail "seed failure does not abort the recipe (want '|| { ...; exit 1; }'): $seed_line" ;;
esac

# (d) args are forwarded with a leading tests/acceptance/ stripped; a default list stays
printf '%s\n' "$body" | grep -q '{{ *args' || fail "recipe does not use its args"
printf '%s\n' "$body" | grep -v '^ *cd ' | grep -Eq '#tests/acceptance/|sed.*tests/acceptance/' \
  || fail "recipe does not strip a leading tests/acceptance/ from its args"
printf '%s\n' "$body" | grep -q 'specs=(smoke/' || fail "recipe lost its default spec list"

# playwright project 'local' covers ./v2
grep -Eq "name: *'local'.*testDir: *'\./v2'" tests/acceptance/playwright.config.ts \
  || fail "playwright.config.ts has no project 'local' with testDir './v2'"

echo "ok: acceptance-local seeds, aborts on failure, forwards args"
