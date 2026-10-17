#!/usr/bin/env bash
# specs/features/v2-pages, R34 as Phase 4 owns it (with R21): SDD 5.1's reserved segments name `guide`
# (not `guides`) and `privacy`; V4.9, S25 and 8.5 say the speculation rules are the only executable
# inline script and that `application/ld+json` data blocks are allowed. A self-test runs the same
# checks on scratch input it must refuse.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

# sdd_problems <v2.md>
sdd_problems() {
    local sdd=$1 sec row
    sec=$(awk '/^### 5\.1/ { on = 1; next } /^### 5\.2/ { on = 0 } on' "$sdd" 2>/dev/null)
    if [ -z "$sec" ]; then
        echo "SDD has no section 5.1"
    else
        row=$(tr '\n' ' ' <<< "$sec" | grep -o 'the segments [^:]*root files' | head -1)
        [ -n "$row" ] || row=$(tr '\n' ' ' <<< "$sec")
        grep -qF '`guide`' <<< "$row" || echo "SDD 5.1 reserved segments do not name \`guide\`"
        grep -qF '`guides`' <<< "$row" && echo "SDD 5.1 reserved segments still name \`guides\`"
        grep -qF '`privacy`' <<< "$row" || echo "SDD 5.1 reserved segments do not name \`privacy\`"
    fi
    row=$(grep -E '^\| V4\.9 ' "$sdd" 2>/dev/null)
    if [ -z "$row" ]; then
        echo "SDD has no V4.9 row"
    else
        grep -qF 'the only executable inline script is the speculation rules; `application/ld+json` data blocks are allowed and are not executable' <<< "$row" ||
            echo "SDD V4.9 lacks the executable-inline-script wording"
        grep -qF 'the only inline script is the speculation rules' <<< "$row" &&
            echo "SDD V4.9 still says 'the only inline script'"
    fi
    row=$(grep -E '^\| S25 ' "$sdd" 2>/dev/null)
    if [ -z "$row" ]; then
        echo "SDD has no S25 row"
    else
        grep -qE 'the only (inline executable|executable inline) script' <<< "$row" ||
            echo "SDD S25 lacks 'the only inline executable script'"
    fi
    sec=$(awk '/^### 8\.5/ { on = 1; next } /^### 8\.6/ { on = 0 } on' "$sdd" 2>/dev/null)
    if [ -z "$sec" ]; then
        echo "SDD has no section 8.5"
    else
        tr '\n' ' ' <<< "$sec" | grep -qE 'the only (inline executable|executable inline) script' ||
            echo "SDD 8.5 lacks 'the only inline executable script'"
    fi
}

fails=0
fail() { echo "FAIL: $1"; fails=1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
expect() { grep -qF -- "$2" <<< "$1" || fail "self-test: $3 did not report '$2'"; }

good_sdd() {
    printf '%s\n' '### 5.1 Routes' '' \
        'Reserved paths match first: the segments `r`, `api`, `guide`, `status`, `privacy`, `static`, the prefix' \
        '`/.well-known/`, and the root files `robots.txt`. After them, a domain.' \
        '### 5.2 A visit' \
        '| S25 | No Datastar. The CSP stays without inline scripts: the only inline executable script is the rules. |' \
        '### 8.5 The gate' '3. an inline script other than the speculation rules appears; the only inline executable script is the rules.' \
        '### 8.6 Rules' \
        '| V4.9 | CSP; the only executable inline script is the speculation rules; `application/ld+json` data blocks are allowed and are not executable; x. | S8 |' > "$1"
}
good_sdd "$tmp/ok.md"
[ -z "$(sdd_problems "$tmp/ok.md")" ] || fail "self-test: good SDD reported '$(sdd_problems "$tmp/ok.md")'"
bad() { good_sdd "$tmp/$1.md"; sed -i.bak "$2" "$tmp/$1.md"; rm "$tmp/$1.md.bak"; expect "$(sdd_problems "$tmp/$1.md")" "$3" "$1"; }
bad s_guides 's/`guide`/`guides`/' 'still name `guides`'
bad s_priv 's/`privacy`/`x`/' 'do not name `privacy`'
bad s_v49 's/the only executable inline script/the only inline script/' 'V4.9 still says'
bad s_ld 's/ data blocks are allowed and are not executable//' 'V4.9 lacks'
bad s_s25 '/^| S25/s/inline executable/inline/' 'S25 lacks'
bad s_85 '/^3\./s/inline executable/inline/' '8.5 lacks'

out=$(sdd_problems specs/sdd/v2.md)
while IFS= read -r line; do [ -n "$line" ] && fail "$line"; done <<< "$out"

[ "$fails" -eq 0 ] && echo "ok: SDD 5.1, S25, 8.5 and V4.9 carry the Phase 4 wording"
exit "$fails"
