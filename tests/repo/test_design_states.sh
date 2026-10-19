#!/usr/bin/env bash
# v2-design-gate Requirement 2 (C2, C7): the design stack helper seeds a fresh history store, starts lens
# with a ttl no seeded domain outlives, every seeded domain's result page renders data-state="fresh",
# and visiting the pages adds no history entry.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fail() { echo "FAIL: $1"; exit 1; }

source tests/repo/lib/netray.sh
[ -f tests/acceptance/design/stack.sh ] || fail "tests/acceptance/design/stack.sh is missing"
source tests/acceptance/design/stack.sh
declare -F design_stack_up >/dev/null || fail "stack.sh defines no design_stack_up"

fixtures=tests/acceptance/fixtures/history
tmp=$(mktemp -d) || fail "mktemp"
trap 'rm -rf "$tmp"; _netray_cleanup' EXIT
design_stack_up "$tmp" "$fixtures" || fail "design_stack_up failed"
[ -n "${LOCAL_LENS_URL:-}" ] || fail "design_stack_up exported no LOCAL_LENS_URL"

count_rows() {
    python3 -c 'import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); print(*c.execute("select count(*), coalesce(max(last_seen_at),0) from entries").fetchone())' "$tmp/history.db"
}

domains=$(python3 -c 'import json,sys; print("\n".join(sorted({json.load(open(f))["domain"] for f in sys.argv[1:]})))' "$fixtures"/*.json) \
    || fail "cannot read fixture domains"
[ -n "$domains" ] || fail "no seeded domains"

before=$(count_rows) || fail "cannot read entries before"

for d in $domains; do
    body=$(curl -s --max-time 10 -w '\n%{http_code}' "$LOCAL_LENS_URL/$d") || fail "curl $d"
    code=${body##*$'\n'}
    [ "$code" = 200 ] || fail "$d answered HTTP $code, want 200"
    case "$body" in
        *'data-state="fresh"'*) ;;
        *) fail "$d result page lacks data-state=\"fresh\"" ;;
    esac
done

sleep 2
after=$(count_rows) || fail "cannot read entries after"
[ "$before" = "$after" ] || fail "history changed by visiting pages: '$before' -> '$after'"

echo "ok: every seeded domain renders fresh and the history store is unchanged"
