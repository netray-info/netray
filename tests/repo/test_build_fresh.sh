#!/usr/bin/env bash
# v2-ci-speed R4: netray-ip's build script reruns only when git state changes. A rerun-if-changed
# path that does not exist makes cargo rerun the script on every call, rebuilding netray-ip and every
# crate above it (each netray_bin in test-repo cost ~27 s instead of ~1 s).
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 1

fail=0
bs=crates/ip/build.rs

# 1. no relative .git path (resolved against crates/ip/, where it never exists)
if grep -nE 'rerun-if-changed=\.git' "$bs"; then
    echo "FAIL: $bs names a relative .git path"; fail=1
fi

# 2. outside a git checkout (Docker build) the script names no git path and still sets both variables
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
if rustc --edition 2024 -o "$tmp/bs" "$bs" 2>"$tmp/rustc.log"; then
    out=$(cd "$tmp" && GIT_CEILING_DIRECTORIES="$tmp/.." ./bs 2>&1)
    grep -q 'GIT_SHORT_SHA=unknown' <<<"$out" || { echo "FAIL: without git, GIT_SHORT_SHA is not unknown: $out"; fail=1; }
    grep -q 'BUILD_DATE=unknown' <<<"$out" || { echo "FAIL: without git, BUILD_DATE is not unknown: $out"; fail=1; }
    if grep -E 'rerun-if-changed=.*(\.git|HEAD|refs)' <<<"$out"; then echo "FAIL: without git, the script names a git path"; fail=1; fi
    # inside the checkout every named path exists
    out=$(cd crates/ip && "$tmp/bs" 2>&1)
    while IFS= read -r p; do
        [ -e "crates/ip/$p" ] || [ -e "$p" ] || { echo "FAIL: rerun path does not exist: $p"; fail=1; }
    done < <(sed -n 's/^cargo::rerun-if-changed=//p' <<<"$out")
else
    echo "FAIL: $bs does not compile standalone: $(head -5 "$tmp/rustc.log")"; fail=1
fi

# 3. a second build with no change finds netray-ip fresh
cargo build -q -p netray --bin netray >/dev/null 2>&1 || { echo "FAIL: cargo build -p netray"; exit 1; }
msg=$(cargo build -p netray --bin netray --message-format=json 2>/dev/null | grep '"reason":"compiler-artifact"' | grep '"name":"netray_ip"' | grep '"kind":\["lib"\]')
grep -q '"fresh":true' <<<"$msg" || { echo "FAIL: netray-ip rebuilt on a second cargo build with no change"; fail=1; }

[ $fail -eq 0 ] && echo ok
exit $fail
