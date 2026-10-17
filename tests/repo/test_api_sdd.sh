#!/usr/bin/env bash
# specs/features/v2-api, R33: the SDD carries the statements this spec adds: 3.1 names
# crates/web/src/api/ as the apex API v1 and its mount until v2-cutover; 3.5 or 4.1 records that a
# stored check's findings and evidence references stay outside the fingerprint and the diff; the
# Phase 3 row of section 10 names 0.30.0. A self-test runs the same checks on input it must refuse.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

# section_text <v2.md> <from-heading-prefix> <to-heading-prefix>, joined into one line
section_text() {
    awk -v from="$2" -v to="$3" 'index($0, from) == 1 { on = 1; next } (index($0, to) == 1 || /^## /) { on = 0 } on' "$1" 2>/dev/null | tr '\n' ' '
}

# rows: "<section>|<label>|<ERE>"; section 3.1, 3.5, 4.1, 3.5or4.1 or row:<phase>
rows() {
    cat <<'ROWS'
3.1|the apex API v1 lives in crates/web/src/api/|crates/web/src/api/
3.1|lens mounts the API ahead of its SPA fallback|lens mounts[^.]*ahead of (its|the) SPA fallback
3.1|the mount lasts until v2-cutover|until `?v2-cutover`?
3.1|v2-cutover mounts it in netray serve|`?v2-cutover`?[^.]*mounts[^.]*netray serve
3.5or4.1|findings and evidence stay outside the fingerprint|(findings[^.]*evidence|evidence[^.]*findings)[^.]*outside the fingerprint
3.5or4.1|findings and evidence stay outside the diff|outside the fingerprint and (the )?diff
row:3|the Phase 3 row names 0.30.0|0\.30\.0
ROWS
}

# sdd_problems <v2.md>
sdd_problems() {
    local sdd=$1 name label ere sec
    while IFS='|' read -r name label ere; do
        [ -n "$name" ] || continue
        case $name in
            3.1) sec=$(section_text "$sdd" '### 3.1' '### 3.2') ;;
            3.5or4.1) sec="$(section_text "$sdd" '### 3.5' '### 3.6') $(section_text "$sdd" '### 4.1' '### 4.2')" ;;
            row:*) sec=$(grep -F -- "| ${name#row:} |" "$sdd" 2>/dev/null) ;;
            *) echo "test bug: unknown section $name"; continue ;;
        esac
        if [ -z "${sec// /}" ]; then echo "SDD has no section or row $name"; continue; fi
        grep -qE -- "$ere" <<< "$sec" || echo "SDD $name does not say: $label"
    done < <(rows)
}

fails=0
fail() { echo "FAIL: $1"; fails=1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
expect() { grep -qF -- "$2" <<< "$1" || fail "self-test: $3 did not report '$2'"; }

good_sdd() {
    printf '%s\n' '### 3.1 Crates' \
        'The apex API v1 is `crates/web/src/api/`; lens mounts it ahead of its SPA fallback until' \
        '`v2-cutover`, which mounts it in `netray serve`.' \
        '### 3.2 Run' '### 3.5 Check model' \
        'A stored check carries its findings and evidence references outside the fingerprint and the diff.' \
        '### 3.6 Module' '| 3 | API v1 | `0.30.0` | yes |' > "$1"
}
good_sdd "$tmp/ok.md"
[ -z "$(sdd_problems "$tmp/ok.md")" ] || fail "self-test: good SDD reported '$(sdd_problems "$tmp/ok.md")'"
bad() { good_sdd "$tmp/$1.md"; sed -i.bak "$2" "$tmp/$1.md"; rm "$tmp/$1.md.bak"; expect "$(sdd_problems "$tmp/$1.md")" "$3" "$1"; }
bad s_path 's#crates/web/src/api/#crates/api/#' "lives in crates/web/src/api/"
bad s_ahead 's/ahead of/behind/' "ahead of its SPA fallback"
bad s_until 's/until$/after/' "until v2-cutover"
bad s_serve 's/netray serve/netray site/' "mounts it in netray serve"
bad s_outfp 's/outside the fingerprint/in the fingerprint/' "outside the fingerprint"
bad s_outdiff 's/ and the diff/ only/' "outside the diff"
bad s_row 's/0\.30\.0/0.28.0/' "names 0.30.0"

# --- the real tree
while IFS= read -r line; do
    [ -n "$line" ] && fail "$line"
done <<< "$(sdd_problems specs/sdd/v2.md)"

[ "$fails" -eq 0 ] && echo "PASS: test_api_sdd"
exit "$fails"
