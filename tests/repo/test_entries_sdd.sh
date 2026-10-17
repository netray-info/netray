#!/usr/bin/env bash
# specs/features/v2-entries, R17: SDD 4.6 states the lookup order of /r/<id> until V5.6 (history,
# then the V1 snapshots; 410 for a well-formed ID in neither; 404 for a malformed one) and the
# minimal entry page until V4.5 (netray-web's router, which lens mounts until V5.1). One row per
# statement in `rows`; later phases add rows for 4.6's OG card (R18) and for 4.7 (R19). A
# self-test runs the same checks on scratch input it must refuse.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

# section_text <v2.md> <from-heading-prefix> <to-heading-prefix>
section_text() {
    awk -v from="$2" -v to="$3" 'index($0, from) == 1 { on = 1; next } index($0, to) == 1 { on = 0 } on' "$1" 2>/dev/null
}

# rows: "<section>|<label>|<ERE>" -- section is 4.6 or 4.7; one row per load-bearing token.
rows() {
    cat <<'ROWS'
4.6|until V5.6 (the lookup order's end)|V5\.6
4.6|history is looked up first|history first|history, then
4.6|then the V1 snapshots|V1 snapshot
4.6|the snapshot lookup is a lens hand-off to netray-web|netray-web
4.6|a well-formed ID in neither answers 410|410
4.6|a malformed ID answers 404|404
4.6|a malformed ID is named as such|malformed
4.6|the minimal entry page lasts until V4.5|V4\.5
4.6|lens mounts netray-web's router until V5.1|V5\.1
4.6|lens mounts the router|mounts
ROWS
}

# sdd_problems <v2.md>
sdd_problems() {
    local sdd=$1 name label ere sec seen=""
    while IFS='|' read -r name label ere; do
        [ -n "$name" ] || continue
        case $name in
            4.6) sec=$(section_text "$sdd" '### 4.6' '### 4.7') ;;
            4.7) sec=$(section_text "$sdd" '### 4.7' '### 4.8') ;;
            *) echo "test bug: unknown section $name"; continue ;;
        esac
        if [ -z "$sec" ]; then
            case " $seen " in *" $name "*) ;; *) echo "SDD has no section $name"; seen="$seen $name" ;; esac
            continue
        fi
        grep -qE -- "$ere" <<< "$sec" || echo "SDD $name does not say: $label"
    done < <(rows)
}

fails=0
fail() { echo "FAIL: $1"; fails=1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
expect() { grep -qF -- "$2" <<< "$1" || fail "self-test: $3 did not report '$2'"; }

good_sdd() {
    printf '%s\n' '### 4.6 `/r/<id>` (R15)' \
        'Until V5.6 `/r/<id>` looks up the history first, then the V1 snapshots (a lookup lens hands' \
        'to `netray-web`); a well-formed ID in neither answers `410`, a malformed ID `404`.' \
        'Until V4.5 the entry renders the minimal page, served by the router lens mounts until V5.1.' \
        '### 4.7 Privacy (R16)' 'text' '### 4.8 Next' > "$1"
}
good_sdd "$tmp/ok.md"
[ -z "$(sdd_problems "$tmp/ok.md")" ] || fail "self-test: good SDD reported '$(sdd_problems "$tmp/ok.md")'"
bad() { # bad <name> <sed expr> <want>
    good_sdd "$tmp/$1.md"; sed -i.bak "$2" "$tmp/$1.md"; rm "$tmp/$1.md.bak"
    expect "$(sdd_problems "$tmp/$1.md")" "$3" "$1"
}
bad s_v56 's/V5\.6/later/' "until V5.6"
bad s_hist 's/history first, then/history and/' "looked up first"
bad s_snap 's/V1 snapshots/old ones/' "V1 snapshots"
bad s_web 's/netray-web/the web crate/' "hand-off to netray-web"
bad s_410 's/410/gone/' "answers 410"
bad s_404 's/404/error/' "answers 404"
bad s_mal 's/malformed/odd/' "named as such"
bad s_v45 's/V4\.5/V4.9/' "until V4.5"
bad s_v51 's/V5\.1/V5.9/' "until V5.1"
bad s_mount 's/mounts/serves/' "lens mounts the router"
printf '### 4.7 Privacy\n' > "$tmp/nosec.md"
expect "$(sdd_problems "$tmp/nosec.md")" "SDD has no section 4.6" "no 4.6"

# --- the real tree
while IFS= read -r line; do
    [ -n "$line" ] && fail "$line"
done <<< "$(sdd_problems specs/sdd/v2.md)"

[ "$fails" -eq 0 ] && echo "PASS: test_entries_sdd"
exit "$fails"
