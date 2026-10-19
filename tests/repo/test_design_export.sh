#!/usr/bin/env bash
# v2-design-gate R7: the design check tests the canvas export before it reads service URLs or
# launches a browser; a missing export exits 2 and names the absolute export path. The export
# key is relative to the manifest's directory, default export/canvas.html.
set -u
source "$(dirname "$0")/lib/netray.sh"

runner="$REPO_ROOT/tests/acceptance/design/check.mjs"
tmp=$(mktemp -d "${TMPDIR:-/tmp}/design_export.XXXXXX")
tmp=${tmp//\/\//\/}  # TMPDIR may end in a slash; the check prints a normalised path
trap 'rm -rf "$tmp"; _netray_cleanup' EXIT

failures=0
fail() { echo "FAIL: $*" >&2; failures=$((failures + 1)); }

[ -f "$runner" ] || fail "runner missing: tests/acceptance/design/check.mjs"

# no service URL may be set: the export test comes first
while IFS= read -r var; do unset "$var"; done < <(compgen -e | grep '^LOCAL_.*_URL$')

# write_manifest <export key line or empty>
write_manifest() {
    cat >"$tmp/manifest.json" <<JSON
{
  $1
  "boards": [
    {
      "title": "Result page, light",
      "host": "lens",
      "path": "/shop.example.com",
      "width": 1280,
      "theme": "light",
      "elements": [
        {"name": "h1 domain", "board": "h1", "page": "main h1", "class": "typography", "properties": ["font-size"]}
      ]
    }
  ]
}
JSON
}

# name | export key line | expected path in stderr
table=(
    "explicit export|\"export\": \"missing/canvas.html\",|$tmp/missing/canvas.html"
    "default export||$tmp/export/canvas.html"
)

for row in "${table[@]}"; do
    IFS='|' read -r name keyline want_path <<<"$row"
    write_manifest "$keyline"
    err=$(node "$runner" "$tmp/manifest.json" "$tmp/out" 2>&1 >/dev/null </dev/null)
    rc=$?
    [ "$rc" -eq 2 ] || fail "$name: exit $rc, want 2 (stderr: $err)"
    case "$err" in
        *"$want_path"*) ;;
        *) fail "$name: stderr lacks '$want_path' (stderr: $err)" ;;
    esac
done

[ "$failures" -eq 0 ] || { echo "$failures failure(s)" >&2; exit 1; }
echo "ok: design export check"
