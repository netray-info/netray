#!/usr/bin/env bash
# v2-design-gate R1 (C1/C4/C5/C6): tests/acceptance/design/manifest.mjs loads a design manifest,
# refuses every unknown key, a missing board title, a duplicate element name on one board and an
# element class outside spacing/typography/colour; `--fixtures` prints the fixture directory.
set -u
source "$(dirname "$0")/lib/netray.sh"

loader="$REPO_ROOT/tests/acceptance/design/manifest.mjs"
tmp=$(mktemp -d "${TMPDIR:-/tmp}/design_manifest.XXXXXX")
trap 'rm -rf "$tmp"; _netray_cleanup' EXIT

failures=0
fail() { echo "FAIL: $*" >&2; failures=$((failures + 1)); }

[ -f "$loader" ] || fail "loader missing: tests/acceptance/design/manifest.mjs"

cat >"$tmp/base.json" <<'JSON'
{
  "export": "export/canvas.html",
  "boards": [
    {
      "title": "Result page, light",
      "host": "lens",
      "path": "/{target}",
      "width": 1280,
      "theme": "light",
      "target": "shop.example.com",
      "elements": [
        {"name": "h1 domain", "board": "h1", "page": "main h1", "class": "typography", "properties": ["font-size", "font-weight"]},
        {"name": "card gap", "board": ".card", "page": ".card", "class": "spacing", "properties": ["padding-top"]}
      ]
    }
  ],
  "accepted": [
    {"board": "Result page, light", "element": "h1 domain", "property": "font-size", "reason": "export rounds to 2rem"}
  ]
}
JSON

# mutate <js statement on m> -> writes $tmp/m.json from base.json
mutate() {
    node -e '
        const fs = require("fs");
        const m = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
        eval(process.argv[2]);
        fs.writeFileSync(process.argv[3], JSON.stringify(m));
    ' "$tmp/base.json" "$1" "$tmp/m.json"
}

# name | mutation | expected exit | stderr substrings (separated by ;;)
table=(
    "C1 valid manifest|;|0|"
    "C1 valid without optional keys|delete m.export; delete m.fixtures; delete m.accepted; delete m.boards[0].target; m.boards[0].path = \"/shop.example.com\"|0|"
    "C1 missing board title|delete m.boards[0].title|2|boards[0]"
    "C4 unknown top-level key|m.colour_scheme = 1|2|colour_scheme"
    "C4 unknown board key|m.boards[0].viewport = 1|2|viewport"
    "C4 unknown element key|m.boards[0].elements[0].weight = 1|2|weight"
    "C4 unknown accepted key|m.accepted[0].severity = 1|2|severity"
    "C5 duplicate element name|m.boards[0].elements[1].name = \"h1 domain\"|2|Result page, light;;h1 domain"
    "C1 target without placeholder|m.boards[0].path = \"/shop.example.com\"|2|boards[0];;{target}"
    "C1 placeholder without target|delete m.boards[0].target|2|boards[0];;{target}"
    "C1 two boards on one file slug|m.boards.push(JSON.parse(JSON.stringify(m.boards[0]))); m.boards[1].title = \"Result page light\"|2|boards[1];;result-page-light"
    "C1 duplicate accepted entry|m.accepted.push({...m.accepted[0]})|2|accepted[1];;duplicate"
    "C6 class motion|m.boards[0].elements[1].class = \"motion\"|2|card gap;;spacing;;typography;;colour"
    "R14 state on a board|m.boards[0].state = \"long domain\"|0|"
    "R14 two boards one title different states|m.boards[0].state = \"short domain\"; m.boards.push({...JSON.parse(JSON.stringify(m.boards[0])), state: \"long domain\"})|0|"
    "R14 one title and state twice|m.boards[0].state = \"long domain\"; m.boards.push(JSON.parse(JSON.stringify(m.boards[0])))|2|boards[1];;result-page-light-long-domain"
    "R14 empty state|m.boards[0].state = \"\"|2|state"
    "v2-inspector-shell wait selector on a board|m.boards[0].wait = \"form[data-stream-state=done]\"|0|"
    "v2-inspector-shell empty wait|m.boards[0].wait = \"\"|2|wait"
)

for row in "${table[@]}"; do
    IFS='|' read -r name mutation want_rc substrings <<<"$row"
    mutate "$mutation" || { fail "$name: could not build manifest"; continue; }
    err=$(node "$loader" "$tmp/m.json" 2>&1 >/dev/null </dev/null)
    rc=$?
    [ "$rc" -eq "$want_rc" ] || fail "$name: exit $rc, want $want_rc (stderr: $err)"
    if [ -n "$substrings" ]; then
        while IFS= read -r want; do
            case "$err" in
                *"$want"*) ;;
                *) fail "$name: stderr lacks '$want' (stderr: $err)" ;;
            esac
        done < <(printf '%s\n' "${substrings//;;/$'\n'}")
    fi
done

# Unreadable manifest: exit 2.
node "$loader" "$tmp/does-not-exist.json" >/dev/null 2>&1 </dev/null
rc=$?
[ "$rc" -eq 2 ] || fail "unreadable manifest: exit $rc, want 2"

# --fixtures: default and explicit (relative value resolves against the repo root).
mutate "delete m.fixtures"
got=$(node "$loader" --fixtures "$tmp/m.json" 2>/dev/null </dev/null)
[ "$got" = "$REPO_ROOT/tests/acceptance/fixtures/history" ] || fail "--fixtures default: got '$got'"

mutate 'm.fixtures = "tests/repo/fixtures"'
got=$(node "$loader" --fixtures "$tmp/m.json" 2>/dev/null </dev/null)
[ "$got" = "$REPO_ROOT/tests/repo/fixtures" ] || fail "--fixtures explicit: got '$got'"

[ "$failures" -eq 0 ] || { echo "$failures failure(s)" >&2; exit 1; }
echo "ok: design manifest loader"
