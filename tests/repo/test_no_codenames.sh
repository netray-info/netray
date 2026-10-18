#!/usr/bin/env bash
# specs/features/v2-pages, requirement 16: the tracked .html, .css, .svg, .ts and .toml files under
# crates/web/templates, crates/web/static, crates/web/islands/src and crates/web/compare.toml hold
# no codename as a whole word (case-insensitive; `-` splits words, so `mhost-prism` hits `prism`).
# A self-test scans the planted fixture tree tests/repo/fixtures/no_codenames/ and must find it.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

WORDS="lens prism tlsight spectra beacon ifconfig-rs"

# scan_root <root>: read relative file paths on stdin, print `FAIL: <file>:<line>: <word>` per hit.
scan_root() {
    local root=$1 file word
    while IFS= read -r file; do
        [ -f "$root/$file" ] || continue
        for word in $WORDS; do
            grep -Eiwno -- "$word" "$root/$file" | cut -d: -f1 | sort -un | while read -r line; do
                echo "FAIL: $file:$line: $word"
            done
        done
    done
}

in_scope() {
    grep -E '^(crates/web/(templates|static|islands/src)/.*|crates/web/compare\.toml)$' \
        | grep -E '\.(html|css|svg|ts|toml)$'
}

# self-test: the planted fixture must be found, the clean file must not
fixture=tests/repo/fixtures/no_codenames
planted=$(cd "$fixture" && find crates -type f | sort | in_scope | scan_root "$PWD")
if ! printf '%s\n' "$planted" | grep -qx 'FAIL: crates/web/templates/x.html:2: lens'; then
    fail "self-test: planted fixture x.html:2 lens not reported (got: ${planted:-nothing})"
fi
if printf '%s\n' "$planted" | grep -q 'clean.html'; then
    fail "self-test: clean fixture reported"
fi

# the real scan
hits=$(git ls-files | in_scope | scan_root "$PWD")
if [ -n "$hits" ]; then
    printf '%s\n' "$hits"
    fails=1
fi

[ "$fails" -eq 0 ] || exit 1
echo "PASS: test_no_codenames"
