#!/usr/bin/env bash
# specs/features/v2-spectrum, phase 2 (R10 deps part, R14): netray-web depends normally on
# netray-model and netray-engine and on no lens or history package (read from
# `cargo metadata`, never the TOML text; v2-entries admits netray-store, SDD 3.1); `pub fn layout` in crates/web/src/spectrum.rs names no
# entry, run, history, store or lens type; SDD 3.9, 5.4, 8.1 and the V4.4 row carry the amended
# wording; the tokens.json linePitchMin description carries the 54-check arithmetic. A self-test
# runs the same checks on scratch input it must refuse.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

# dep_problems: metadata JSON on stdin; one line per violation.
dep_problems() {
    python3 -I -c '
import json, re, sys
meta = json.load(sys.stdin)
web = [p for p in meta["packages"] if p["name"] == "netray-web"]
if not web:
    print("package netray-web not found"); sys.exit(0)
normal = {d["name"] for d in web[0]["dependencies"] if d.get("kind") in (None, "normal")}
for need in ("netray-model", "netray-engine"):
    if need not in normal: print("netray-web has no normal dependency on " + need)
for n in sorted(normal):
    if n in ("lens", "netray-lens") or re.search("history", n):
        print("netray-web depends on " + n + " (lens and history stay out of the web crate)")
'
}

# sig_problems <spectrum.rs>: the `pub fn layout` signature, up to `{`.
sig_problems() {
    local sig
    sig=$(tr '\n' ' ' < "$1" 2>/dev/null | grep -oE 'pub fn layout[^{]*' | head -1)
    if [ -z "$sig" ]; then echo "$1 has no pub fn layout"; return 0; fi
    local pat
    for pat in 'Entry' 'Run<' 'History' 'store' 'lens'; do
        ! grep -q "$pat" <<< "$sig" || echo "pub fn layout signature names $pat"
    done
}

# sdd_problems <v2.md>
sdd_problems() {
    local sdd=$1 sec p
    sec=$(awk '/^### 5\.4/ { on = 1; next } /^### 5\.5/ { on = 0 } on' "$sdd" 2>/dev/null)
    if [ -z "$sec" ]; then
        echo "SDD has no section 5.4"
    else
        grep -qF 'spectrum::layout(current, older, catalogue, width)' <<< "$sec" ||
            echo "SDD 5.4 does not name spectrum::layout(current, older, catalogue, width)"
        grep -qF 'netray_web::spectrum' <<< "$sec" || echo "SDD 5.4 does not name netray_web::spectrum"
        grep -q 'maps' <<< "$sec" || echo "SDD 5.4 does not say a caller maps stored entries onto statuses"
    fi
    p=$(awk -v RS= '/The spectrum.s line height/' "$sdd" 2>/dev/null)
    if [ -z "$p" ]; then
        echo "SDD has no paragraph beginning 'The spectrum's line height'"
    else
        grep -q 'muted height' <<< "$p" || echo "SDD 3.9 line-height paragraph does not mention 'muted height'"
    fi
    sec=$(awk '/^### 8\.1/ { on = 1; next } /^### 8\.2/ { on = 0 } on' "$sdd" 2>/dev/null)
    if [ -z "$sec" ]; then
        echo "SDD has no section 8.1"
    else
        grep -q '^|.*src/spectrum\.rs' <<< "$sec" || echo "SDD 8.1 table has no row for src/spectrum.rs"
    fi
    p=$(grep -F '| V4.4 |' "$sdd" 2>/dev/null | head -1)
    if [ -z "$p" ]; then
        echo "SDD has no V4.4 row"
    else
        local w
        for w in v2-spectrum v2-result-page v2-inspectors og.png; do
            grep -qF "$w" <<< "$p" || echo "SDD V4.4 row does not name $w"
        done
    fi
}

# tokens_problems <tokens.json>
tokens_problems() {
    python3 -I -c '
import json, sys
try:
    d = json.load(open(sys.argv[1]))["spectrum"]["linePitchMin"]["$description"]
except Exception as e:
    print("tokens.json spectrum.linePitchMin $description unreadable: %s" % e); sys.exit(0)
for w in ("54 checks", "6.6"):
    if w not in d: print("linePitchMin $description does not contain " + repr(w))
if "41 checks" in d: print("linePitchMin $description still says 41 checks")
' "$1"
}

fails=0
fail() { echo "FAIL: $1"; fails=1; }

# --- self-test on scratch input
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
expect() { # expect <output> <want> <label>
    grep -qF -- "$2" <<< "$1" || fail "self-test: $3 did not report '$2'"
}

meta() { # meta <deps json array>
    printf '{"packages":[{"name":"netray-web","dependencies":%s}]}' "$1"
}
okdeps='[{"name":"netray-model","kind":null},{"name":"netray-engine","kind":null},{"name":"serde","kind":null}]'
[ -z "$(meta "$okdeps" | dep_problems)" ] || fail "self-test: good deps reported '$(meta "$okdeps" | dep_problems)'"
expect "$(meta '[{"name":"netray-model","kind":null}]' | dep_problems)" "no normal dependency on netray-engine" "missing engine"
expect "$(meta '[{"name":"netray-model","kind":null},{"name":"netray-engine","kind":"dev"}]' | dep_problems)" "no normal dependency on netray-engine" "dev-only engine"
storedeps='[{"name":"netray-model","kind":null},{"name":"netray-engine","kind":null},{"name":"netray-store","kind":null}]'
[ -z "$(meta "$storedeps" | dep_problems)" ] || fail "self-test: netray-store reported '$(meta "$storedeps" | dep_problems)'"
expect "$(meta '[{"name":"netray-model","kind":null},{"name":"netray-engine","kind":null},{"name":"run-history","kind":null}]' | dep_problems)" "depends on run-history" "history"
expect "$(meta '[{"name":"netray-model","kind":null},{"name":"netray-engine","kind":null},{"name":"lens","kind":null}]' | dep_problems)" "depends on lens" "lens"
expect "$(printf '{"packages":[]}' | dep_problems)" "netray-web not found" "no package"

printf 'pub fn layout(\n    current: &[Status],\n    width: u32,\n) -> Out {\n}\n' > "$tmp/sig_ok.rs"
[ -z "$(sig_problems "$tmp/sig_ok.rs")" ] || fail "self-test: good signature reported '$(sig_problems "$tmp/sig_ok.rs")'"
printf 'pub fn layout(e: &[Entry], w: u32) -> Out {\n}\n' > "$tmp/sig_entry.rs"
expect "$(sig_problems "$tmp/sig_entry.rs")" "names Entry" "Entry"
printf 'pub fn layout(r: &Run<X>, w: u32) -> Out {\n}\n' > "$tmp/sig_run.rs"
expect "$(sig_problems "$tmp/sig_run.rs")" "names Run<" "Run<"
printf 'fn other() {}\n' > "$tmp/sig_none.rs"
expect "$(sig_problems "$tmp/sig_none.rs")" "has no pub fn layout" "no layout"

good_sdd() {
    printf '%s\n' '### 5.4 Spectrum' 'It is spectrum::layout(current, older, catalogue, width) in netray_web::spectrum;' \
        'a caller maps stored entries onto statuses with profiles.' '### 5.5 Label placement' \
        "The spectrum's line height is the weight; a muted height for the rest." '' \
        '### 8.1 Layout' '| `src/spectrum.rs` | layout |' '### 8.2 Tokens' \
        '| V4.4 | v2-spectrum, v2-result-page, v2-inspectors, og.png |' > "$1"
}
good_sdd "$tmp/sdd_ok.md"
[ -z "$(sdd_problems "$tmp/sdd_ok.md")" ] || fail "self-test: good SDD reported '$(sdd_problems "$tmp/sdd_ok.md")'"
bad() { # bad <name> <sed expr> <want>
    good_sdd "$tmp/$1.md"; sed -i.bak "$2" "$tmp/$1.md"; rm "$tmp/$1.md.bak"
    expect "$(sdd_problems "$tmp/$1.md")" "$3" "$1"
}
bad s_sig 's/spectrum::layout(current, older, catalogue, width)/layout/' "does not name spectrum::layout"
bad s_mod 's/netray_web::spectrum/web/' "does not name netray_web::spectrum"
bad s_maps 's/maps/takes/' "maps stored entries"
bad s_muted 's/muted height/grey/' "'muted height'"
bad s_row 's#src/spectrum.rs#src/other.rs#' "src/spectrum.rs"
bad s_v44 's/og.png/card/' "does not name og.png"
bad s_v44b 's/v2-inspectors/x/' "does not name v2-inspectors"

tok() { printf '{"spectrum":{"linePitchMin":{"$description":"%s"}}}' "$1" > "$tmp/tok.json"; tokens_problems "$tmp/tok.json"; }
[ -z "$(tok '54 checks at 390 px give 6.6 px')" ] || fail "self-test: good tokens reported '$(tok '54 checks at 390 px give 6.6 px')'"
expect "$(tok '41 checks give 8.7 px')" "still says 41 checks" "old tokens"
expect "$(tok '41 checks give 8.7 px')" "'54 checks'" "old tokens"
expect "$(tok '54 checks give 8 px')" "'6.6'" "no 6.6"

# --- the real tree
if json=$(cargo metadata --format-version 1 --no-deps --offline); then
    out=$(printf '%s' "$json" | dep_problems)
else
    out="cargo metadata failed"
fi
out+=$'\n'$(sig_problems crates/web/src/spectrum.rs)
out+=$'\n'$(sdd_problems specs/sdd/v2.md)
out+=$'\n'$(tokens_problems crates/web/tokens.json)
while IFS= read -r line; do
    [ -n "$line" ] && fail "$line"
done <<< "$out"

[ "$fails" -eq 0 ] && echo "ok: netray-web deps, layout signature, SDD amendments and linePitchMin description"
exit "$fails"
