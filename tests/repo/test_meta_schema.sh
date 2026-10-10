#!/usr/bin/env bash
# Phase 2 req. 7, C1-C3: live /api/meta fixtures (lens, beacon) validate against the
# ecosystem-meta schema; an email_base_url that is empty, a non-https host or a path fails.
set -uo pipefail
cd "$(dirname "$0")/.." && cd .. || { echo "FAIL: cannot cd to repo root"; exit 1; }

schema=tests/acceptance/schemas/ecosystem-meta.schema.json
helper=tests/repo/lib/validate_meta.mjs
lens=tests/repo/fixtures/meta/lens.json
beacon=tests/repo/fixtures/meta/beacon.json
fail=0

for f in "$schema" "$helper" "$lens" "$beacon"; do
    [ -f "$f" ] || { echo "FAIL: missing $f"; exit 1; }
done

# expect <0|1> <label> <json> [--set ...]
expect() {
    local want=$1 label=$2 out rc
    shift 2
    out=$(node "$helper" "$schema" "$@" 2>&1)
    rc=$?
    if [ "$rc" -ne "$want" ]; then
        echo "FAIL: $label: expected exit $want, got $rc: $(echo "$out" | head -3 | tr '\n' ';')"
        fail=1
    fi
}

expect 0 "C1 lens fixture validates" "$lens"
expect 0 "C2 beacon fixture validates" "$beacon"
for v in "" "http://email.example.com" "/email"; do
    expect 1 "C3 lens with email_base_url='$v' is rejected" "$lens" --set "ecosystem.email_base_url=$v"
done
# ifconfig-rs sends empty strings for the ecosystem entries it does not use (netray-common
# ecosystem.rs contract); only its own ip_base_url must be set.
ifconfig=tests/repo/fixtures/meta/ifconfig-rs.json
[ -f "$ifconfig" ] || { echo "FAIL: missing $ifconfig"; exit 1; }
expect 0 "ifconfig-rs fixture (empty sibling URLs) validates" "$ifconfig"
expect 1 "ifconfig-rs with an empty ip_base_url is rejected" "$ifconfig" --set "ecosystem.ip_base_url="
expect 1 "beacon with an empty email_base_url is rejected" "$beacon" --set "ecosystem.email_base_url="
expect 1 "lens with a trailing space in email_base_url is rejected" "$lens" \
    --set "ecosystem.email_base_url=https://email.example.com "
# Control: a valid override must still pass, so the rejections above are not a broken helper.
expect 0 "control: lens with email_base_url=https://email.example.com validates" "$lens" \
    --set "ecosystem.email_base_url=https://email.example.com"

exit $fail
