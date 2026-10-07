#!/usr/bin/env bash
# Phase 3, requirement 10 (C1 + C3): one root CONTRIBUTING.md, one root AGENTS.md, no dco.yml,
# and the root CONTRIBUTING.md carries the four points. Run from the repo root.
# Prints one line per failed check; exit 1 if any failed.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=()
fail() { fails+=("$1"); }

for name in CONTRIBUTING.md AGENTS.md; do
    found=$(git ls-files | awk -F/ -v n="$name" '$NF == n')
    count=$(printf '%s' "$found" | grep -c .)
    if [ "$count" -ne 1 ] || [ "$found" != "$name" ]; then
        fail "expected exactly one $name at the root, tracked: $(printf '%s' "$found" | tr '\n' ' ')"
    fi
done

dco=$(git ls-files | awk -F/ '$NF == "dco.yml"')
[ -n "$dco" ] && fail "dco.yml still tracked: $(printf '%s' "$dco" | tr '\n' ' ')"

c=CONTRIBUTING.md
if [ -f "$c" ]; then
    grep -qi 'maintainer' "$c" || fail "$c does not mention a maintainer"
    grep -qi 'issue' "$c" || fail "$c does not mention issues"
    grep -qi 'pull request' "$c" || fail "$c does not mention pull requests"
    grep -Ei 'self[- ]?host' "$c" | grep -Eqi '\b(not|no)\b' \
        || fail "$c does not state that self-hosting is not supported"
else
    fail "$c is missing at the root"
fi

if [ ${#fails[@]} -gt 0 ]; then
    printf 'FAIL: %s\n' "${fails[@]}" >&2
    exit 1
fi
echo "contributing docs ok"
