#!/usr/bin/env bash
# Phase 2, requirement 4 (C1 + C5): the six service crates are libraries, and crates/netray
# is the one binary. Run from the repo root. Prints one line per failed check; exit 1 if any failed.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=()
fail() { fails+=("$1"); }

for s in ifconfig-rs mhost-prism tlsight http email lens; do
    m="crates/$s/Cargo.toml"
    if [ ! -f "$m" ]; then
        fail "$m is missing"
        continue
    fi
    grep -Eq '^\[\[bin\]\]' "$m" && fail "$m still declares a [[bin]] table"
    [ -n "$(git ls-files "crates/$s/src/main.rs")" ] && fail "crates/$s/src/main.rs is still tracked"
    [ -n "$(git ls-files "crates/$s/src/lib.rs")" ] || fail "crates/$s/src/lib.rs is not tracked"
done

n=crates/netray/Cargo.toml
if [ ! -f "$n" ]; then
    fail "$n is missing"
else
    if grep -Eq '^name *= *"netray"' "$n" && [ -n "$(git ls-files crates/netray/src/main.rs)" ]; then
        :
    elif awk '/^\[\[bin\]\]/{b=1;next} /^\[/{b=0} b && /^name *= *"netray"/{f=1} END{exit !f}' "$n"; then
        :
    else
        fail "$n does not declare the binary netray"
    fi
fi

if [ ${#fails[@]} -gt 0 ]; then
    printf 'FAIL: %s\n' "${fails[@]}" >&2
    exit 1
fi
echo "service libs and netray binary ok"
