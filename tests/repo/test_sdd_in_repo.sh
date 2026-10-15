#!/usr/bin/env bash
# specs/features/sdd-into-monorepo, specs/features/v2-design-system: the V2 SDD lives in this
# repository, its token source is crates/web/tokens.json, and CLAUDE.md points at both as its own
# (R4). The SDD's netray-web row lists common as a dependency. A self-test runs the same check on
# scratch trees it must refuse: CLAUDE.md still citing the planning SDD, tokens that are not valid
# JSON (a trailing comma, NaN), the old token path, and a netray-web row without common.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

# problems <root>: print one line per violation under <root>.
problems() {
    [ -s "$1/specs/sdd/v2.md" ] || echo "specs/sdd/v2.md is missing or empty"
    if [ ! -f "$1/crates/web/tokens.json" ]; then
        echo "crates/web/tokens.json is missing"
    elif ! python3 -I -c '
import json, sys
def refuse(name):
    raise ValueError(name)
json.load(open(sys.argv[1]), parse_constant=refuse)' "$1/crates/web/tokens.json" 2>/dev/null; then
        echo "crates/web/tokens.json is not valid JSON"
    fi
    [ ! -e "$1/specs/sdd/v2-tokens.json" ] || echo "specs/sdd/v2-tokens.json still exists"
    grep -q 'crates/web/tokens\.json' "$1/specs/sdd/v2.md" 2>/dev/null || echo "specs/sdd/v2.md does not name crates/web/tokens.json"
    grep -q 'crates/web/tokens\.json' "$1/CLAUDE.md" || echo "CLAUDE.md does not name crates/web/tokens.json"
    grep -q 'v2-tokens\.json' "$1/specs/sdd/v2.md" 2>/dev/null && echo "specs/sdd/v2.md still names v2-tokens.json"
    grep -q 'v2-tokens\.json' "$1/CLAUDE.md" && echo "CLAUDE.md still names v2-tokens.json"
    awk -F'|' '$2 ~ /^ *`netray-web` *$/ { found = 1; if ($4 ~ /`common`/) ok = 1 }
        END { exit !(found && ok) }' "$1/specs/sdd/v2.md" 2>/dev/null ||
        echo "specs/sdd/v2.md: the netray-web row does not list common in Depends on"
    grep -q 'specs/sdd/v2\.md' "$1/CLAUDE.md" || echo "CLAUDE.md does not name specs/sdd/v2.md"
    grep -q -i 'planning SDD' "$1/CLAUDE.md" && echo "CLAUDE.md still says planning SDD"
    return 0
}

# good <dir>: a scratch tree that violates nothing.
good() {
    mkdir -p "$1/specs/sdd" "$1/crates/web"
    printf '%s\n' '# SDD' '`crates/web/tokens.json`' '| `netray-web` | router | `engine`, `common` | x |' > "$1/specs/sdd/v2.md"
    echo '{"a": 1}' > "$1/crates/web/tokens.json"
    printf '%s\n' '`specs/sdd/v2.md` `crates/web/tokens.json`' > "$1/CLAUDE.md"
}

fails=0
fail() { echo "FAIL: $1"; fails=1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# Self-test: the good tree reports nothing; every fault is reported on its own tree. NaN is not
# JSON (RFC 8259), though Python's json accepts it by default.
good "$tmp/good"
[ -z "$(problems "$tmp/good")" ] || fail "self-test: the good tree reported '$(problems "$tmp/good")'"

expect() { # expect <name> <want>: the tree $tmp/<name> must report <want>
    got=$(problems "$tmp/$1")
    printf '%s\n' "$got" | grep -q "$2" || fail "self-test: $1 did not report '$2'"
}

good "$tmp/nan"; echo '{"a": NaN}' > "$tmp/nan/crates/web/tokens.json"
expect nan "not valid JSON"

good "$tmp/trailing"; echo '{"a": 1,}' > "$tmp/trailing/crates/web/tokens.json"
expect trailing "not valid JSON"

good "$tmp/missing"; rm "$tmp/missing/crates/web/tokens.json"
expect missing "crates/web/tokens.json is missing"

good "$tmp/old"; echo '{}' > "$tmp/old/specs/sdd/v2-tokens.json"
expect old "v2-tokens.json still exists"

good "$tmp/sddname"; echo '# SDD' > "$tmp/sddname/specs/sdd/v2.md"
expect sddname "v2.md does not name crates/web/tokens.json"

good "$tmp/claudename"; echo '`specs/sdd/v2.md`' > "$tmp/claudename/CLAUDE.md"
expect claudename "CLAUDE.md does not name crates/web/tokens.json"

good "$tmp/sddold"; echo '`v2-tokens.json`' >> "$tmp/sddold/specs/sdd/v2.md"
expect sddold "v2.md still names v2-tokens.json"

good "$tmp/claudeold"; echo '`specs/sdd/v2-tokens.json`' >> "$tmp/claudeold/CLAUDE.md"
expect claudeold "CLAUDE.md still names v2-tokens.json"

good "$tmp/nocommon"
printf '%s\n' '`crates/web/tokens.json`' '| `netray-web` | router uses `common` | `engine` | x |' > "$tmp/nocommon/specs/sdd/v2.md"
expect nocommon "netray-web row does not list common"

good "$tmp/norow"; echo '`crates/web/tokens.json`' > "$tmp/norow/specs/sdd/v2.md"
expect norow "netray-web row does not list common"

good "$tmp/nov2"; rm "$tmp/nov2/specs/sdd/v2.md"
expect nov2 "specs/sdd/v2.md is missing"

good "$tmp/planning"; echo 'V2 core (planning SDD `v2.md` §3)' > "$tmp/planning/CLAUDE.md"
expect planning "does not name specs/sdd/v2.md"
expect planning "still says planning SDD"

out=$(problems .)
if [ -n "$out" ]; then
    while IFS= read -r line; do fail "$line"; done <<< "$out"
fi

[ "$fails" -eq 0 ] && echo "ok: specs/sdd/v2.md, crates/web/tokens.json and the CLAUDE.md pointer"
exit "$fails"
