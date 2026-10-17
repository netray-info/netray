#!/usr/bin/env bash
# specs/features/v2-spectrum, R17 and the SDD part of R18: SDD 5.5 point 3 names the two row tokens
# `spectrum.labelLeader` and `spectrum.labelRow`; 8.3's `spectrum` row says it renders a layout with
# every run's state and one `band-<protocol>` group per protocol; 5.5 point 4 gives the markup shape
# (`band-<protocol>`, `data-run`, `data-shown`, `data-unmeasured`). A self-test runs the same checks
# on scratch input it must refuse.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

# sdd_problems <v2.md>
sdd_problems() {
    local sdd=$1 sec p w
    sec=$(awk '/^### 5\.5/ { on = 1; next } /^### 5\.6/ { on = 0 } on' "$sdd" 2>/dev/null)
    if [ -z "$sec" ]; then
        echo "SDD has no section 5.5"
    else
        p=$(awk '/^3\. / { on = 1 } /^4\. / { on = 0 } on' <<< "$sec")
        if [ -z "$p" ]; then
            echo "SDD 5.5 has no point 3"
        else
            for w in 'spectrum.labelLeader' 'spectrum.labelRow'; do
                grep -qF "$w" <<< "$p" || echo "SDD 5.5 point 3 does not name $w"
            done
        fi
        p=$(awk '/^4\. / { on = 1 } on' <<< "$sec")
        if [ -z "$p" ]; then
            echo "SDD 5.5 has no point 4"
        else
            for w in 'band-<protocol>' 'data-run' 'data-shown' 'data-unmeasured'; do
                grep -qF "$w" <<< "$p" || echo "SDD 5.5 point 4 does not contain $w"
            done
        fi
    fi
    sec=$(awk '/^### 8\.3/ { on = 1; next } /^### 8\.4/ { on = 0 } on' "$sdd" 2>/dev/null)
    if [ -z "$sec" ]; then
        echo "SDD has no section 8.3"
        return 0
    fi
    p=$(awk -F'|' '/^\|/ { c = $2; gsub(/[ `]/, "", c); if (c == "spectrum") { print; exit } }' <<< "$sec")
    if [ -z "$p" ]; then
        echo "SDD 8.3 table has no row for spectrum"
    else
        for w in 'layout' 'every run' 'band-<protocol>'; do
            grep -qF "$w" <<< "$p" || echo "SDD 8.3 spectrum row does not contain '$w'"
        done
    fi
}

fails=0
fail() { echo "FAIL: $1"; fails=1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
expect() { # expect <output> <want> <label>
    grep -qF -- "$2" <<< "$1" || fail "self-test: $3 did not report '$2'"
}

good_sdd() {
    printf '%s\n' '### 5.5 Label placement' 'intro' \
        '2. second point' \
        '3. Rows: `spectrum.labelLeader` and `spectrum.labelRow` set the pitch.' '   more of three' \
        '4. Replay: one `band-<protocol>` group, `data-run`, `data-shown`, `data-unmeasured`.' \
        '### 5.6 Trace' '### 8.3 Components' '| Component | Used by |' '|---|---|' \
        '| `spectrum` | renders a layout (`layout`) with every run'"'"'s state: every run, one `band-<protocol>` group per protocol |' \
        '| `trace` | page |' '### 8.4 Islands' > "$1"
}
good_sdd "$tmp/ok.md"
[ -z "$(sdd_problems "$tmp/ok.md")" ] || fail "self-test: good SDD reported '$(sdd_problems "$tmp/ok.md")'"
bad() { # bad <name> <sed expr> <want>
    good_sdd "$tmp/$1.md"; sed -i.bak "$2" "$tmp/$1.md"; rm "$tmp/$1.md.bak"
    expect "$(sdd_problems "$tmp/$1.md")" "$3" "$1"
}
bad s_leader 's/spectrum.labelLeader/x/' "point 3 does not name spectrum.labelLeader"
bad s_row 's/spectrum.labelRow/x/' "point 3 does not name spectrum.labelRow"
bad s_band4 '/^4\./s/band-<protocol>/band/' "point 4 does not contain band-<protocol>"
bad s_run 's/data-run/x/' "point 4 does not contain data-run"
bad s_shown 's/data-shown/x/' "point 4 does not contain data-shown"
bad s_unm 's/data-unmeasured/x/' "point 4 does not contain data-unmeasured"
bad s_nolayout 's/(`layout`)//; s/renders a layout/renders/' "spectrum row does not contain 'layout'"
bad s_every 's/every run/each run/g' "spectrum row does not contain 'every run'"
bad s_band83 '/^| `spectrum`/s/band-<protocol>/band/g' "spectrum row does not contain 'band-<protocol>'"
bad s_norow 's/^| `spectrum` |/| `other` |/' "no row for spectrum"

out=$(sdd_problems specs/sdd/v2.md)
while IFS= read -r line; do
    [ -n "$line" ] && fail "$line"
done <<< "$out"

[ "$fails" -eq 0 ] && echo "ok: SDD 5.5 and 8.3 carry the spectrum component wording"
exit "$fails"
