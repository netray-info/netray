#!/usr/bin/env bash
# specs/features/backend-correctness (Phase 3, req 5): no frontend under crates/*/frontend/src or
# packages/common-frontend/src uses a raw HTML sink (innerHTML, outerHTML, insertAdjacentHTML,
# dangerouslySetInnerHTML). A self-test runs the same scanner on a fixture that has a sink.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

pattern='(innerHTML|outerHTML|insertAdjacentHTML|dangerouslySetInnerHTML)'

# scan FILE...: prints file:line for every sink match, returns 0 when something matched.
scan() {
    [ $# -gt 0 ] || return 1
    grep -nE "$pattern" -- "$@" /dev/null
}

# Scanner self-test: the fixture must be reported.
fixture=tests/repo/fixtures/raw-html-sink/sink.ts
if [ -f "$fixture" ]; then
    scan "$fixture" > /dev/null || fail "scanner does not report the sink in $fixture"
else
    fail "$fixture missing"
fi

# The tree.
files=()
while IFS= read -r f; do files+=("$f"); done < <(
    git ls-files -- 'crates/*/frontend/src/*' 'packages/common-frontend/src/*')
echo "scanned ${#files[@]} frontend files"
if [ "${#files[@]}" -eq 0 ]; then
    fail "no frontend files scanned (pathspec matched nothing)"
else
    hits=$(scan "${files[@]}")
    [ -z "$hits" ] || { echo "$hits"; fail "raw HTML sink in frontend source (file:line above)"; }
fi

[ "$fails" -eq 0 ] || exit 1
echo "PASS: no raw HTML sinks in frontend sources"
