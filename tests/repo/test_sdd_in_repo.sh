#!/usr/bin/env bash
# specs/features/sdd-into-monorepo: the V2 SDD and its token source live in this repository, and
# CLAUDE.md points at them as its own (R4). A self-test runs the same check on scratch trees it
# must refuse: CLAUDE.md still citing the planning SDD, and tokens that are not valid JSON.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

# problems <root>: print one line per violation under <root>.
problems() {
    [ -s "$1/specs/sdd/v2.md" ] || echo "specs/sdd/v2.md is missing or empty"
    if [ ! -f "$1/specs/sdd/v2-tokens.json" ]; then
        echo "specs/sdd/v2-tokens.json is missing"
    elif ! python3 -I -c 'import json, sys; json.load(open(sys.argv[1]))' "$1/specs/sdd/v2-tokens.json" 2>/dev/null; then
        echo "specs/sdd/v2-tokens.json is not valid JSON"
    fi
    grep -q 'specs/sdd/v2\.md' "$1/CLAUDE.md" || echo "CLAUDE.md does not name specs/sdd/v2.md"
    grep -q -i 'planning SDD' "$1/CLAUDE.md" && echo "CLAUDE.md still says planning SDD"
    return 0
}

fails=0
fail() { echo "FAIL: $1"; fails=1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# Self-test: a tree with every fault must report each one.
mkdir -p "$tmp/bad/specs/sdd"
echo '# SDD' > "$tmp/bad/specs/sdd/v2.md"
echo '{"a": 1,}' > "$tmp/bad/specs/sdd/v2-tokens.json"
echo 'V2 core (planning SDD `v2.md` §3)' > "$tmp/bad/CLAUDE.md"
bad=$(problems "$tmp/bad")
for want in "not valid JSON" "does not name specs/sdd/v2.md" "still says planning SDD"; do
    printf '%s\n' "$bad" | grep -q "$want" || fail "self-test: a bad tree did not report '$want'"
done

out=$(problems .)
if [ -n "$out" ]; then
    while IFS= read -r line; do fail "$line"; done <<< "$out"
fi

[ "$fails" -eq 0 ] && echo "ok: specs/sdd/v2.md, specs/sdd/v2-tokens.json and the CLAUDE.md pointer"
exit "$fails"
