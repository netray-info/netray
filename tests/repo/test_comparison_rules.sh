#!/usr/bin/env bash
# comparison-page-rules.md describes the generated /compare page (compare.toml,
# catalogue-derived netray column, generated sitemap), not the old hand-edited
# site/compare/index.html.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fail() { echo "FAIL: $1"; exit 1; }

f=specs/rules/comparison-page-rules.md
[ -f "$f" ] || fail "$f missing"

for need in 'crates/web/compare.toml' 'templates/pages/compare.html' 'catalogue' 'sitemap' \
            '✓' '~' '✗' '—' 'Last reviewed' 'Quarterly review pass' 'netray'; do
  grep -qF -- "$need" "$f" || fail "$f does not mention '$need'"
done
grep -qiE 'two tables|Table 1.*Table 2' "$f" || fail "$f does not describe the two tables"
grep -qi 'generated' "$f" || fail "$f does not say anything is generated"

bad=""
grep -qi 'self-contained html' "$f" && bad="$bad 'self-contained HTML'"
grep -qF 'build-sitemap.mjs' "$f" && bad="$bad 'build-sitemap.mjs'"
grep -qF 'site/compare/index.html' "$f" && bad="$bad 'site/compare/index.html'"
for w in lens prism tlsight spectra beacon ifconfig-rs; do
  grep -qiw -- "$w" "$f" && bad="$bad '$w'"
done
[ -z "$bad" ] || fail "$f still mentions:$bad"

row=$(grep -F 'specs/rules/comparison-page-rules.md' CLAUDE.md | grep '^|') || fail "CLAUDE.md has no comparison-page-rules row"
echo "$row" | grep -qF 'crates/web/compare.toml' || fail "CLAUDE.md row does not point at crates/web/compare.toml"
echo "$row" | grep -qF 'site/compare/index.html' && fail "CLAUDE.md row still names site/compare/index.html"

echo "PASS: comparison page rules match the generated page"
