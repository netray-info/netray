#!/usr/bin/env bash
# specs/features/v2-admission (C2, C15): SDD section 4.4 states the admission rules as built (the
# [admission] keys, the order of check_limits and admit, the 429 refusal, the V1 holder, the
# prefetch token rule), section 9.2's runs counter lists the badge and og triggers, and the
# V2.6 and S33 rows say who builds it and until when.
set -euo pipefail
cd "$(dirname "$0")/../.."
REPO_ROOT=$(pwd)
SDD="$REPO_ROOT/specs/sdd/v2.md"

fails=0
fail() { echo "FAIL: $1"; fails=1; }

[ -s "$SDD" ] || { echo "FAIL: specs/sdd/v2.md is missing or empty"; exit 1; }

s44=$(awk '/^### 4\.4/ {on=1; next} /^### 4\.5/ {on=0} on' "$SDD")
[ -n "$s44" ] || fail "section 4.4 not found"

for needle in '[admission]' 'client_budget_per_hour' 'domain_run_slots' 'check_limits' 'admit' \
    '429' 'Retry-After' 'keyed by entry id' 'PREFETCH_REFUSED' 'case-insensitive'; do
    grep -qF -- "$needle" <<< "$s44" || fail "SDD 4.4 does not mention '$needle'"
done

grep -F '`netray_runs_total`' "$SDD" | grep -qF 'visit, recheck, api, badge, og' ||
    fail "SDD 9.2 netray_runs_total row does not list 'visit, recheck, api, badge, og'"
grep '^| V2\.6 ' "$SDD" | grep -qF 'Built by `v2-admission`' ||
    fail "SDD V2.6 row does not contain 'Built by \`v2-admission\`'"
grep '^| S33 ' "$SDD" | grep -qF 'until the cutover' ||
    fail "SDD S33 row does not contain 'until the cutover'"

[ "$fails" -eq 0 ] && echo "PASS: SDD admission section"
exit "$fails"
