#!/usr/bin/env bash
# frontend-rules.md describes the V2 frontend (templates, tokens, islands, the stream island
# contract), not the frozen SolidJS apps. Spec v2-pages requirement 31; SDD v2 §8.6.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

rc=0
fail() { echo "FAIL: $1"; rc=1; }

f=specs/rules/frontend-rules.md
[ -f "$f" ] || { echo "FAIL: $f missing"; exit 1; }

headings=$(grep -E '^#{1,6} ' "$f")

# Sections, case-insensitive on headings.
for pair in 'templates|components:templates/components' 'tokens:tokens' 'islands:islands' \
            'marker:marker' 'streaming|stream:streaming' 'accessibility:accessibility' 'testing:testing'; do
  re=${pair%%:*}; name=${pair#*:}
  echo "$headings" | grep -qiE "$re" || fail "$f has no heading covering $name"
done

# Islands' limits.
for need in 'register' 'mount' 'data-stream-url' 'data-stream-autostart' 'data-stream-state' 'fragment' 'end'; do
  grep -qiw -- "$need" "$f" || fail "$f does not mention '$need'"
done
grep -qiE 'data attributes?' "$f" || fail "$f does not say islands take data attributes only"
grep -qiE 'no style writes|never write(s)? styles?|style writes' "$f" || fail "$f does not name the no-style-writes limit"
grep -qE '30 KB|30720' "$f" || fail "$f does not name the 30 KB budget"

# No codename, except one line that is the codename check's word list.
words='lens|prism|tlsight|spectra|beacon|ifconfig-rs'
n=0
while IFS= read -r line; do
  all=1
  for w in lens prism tlsight spectra beacon ifconfig-rs; do
    echo "$line" | grep -qiw -- "$w" || all=0
  done
  if [ "$all" = 1 ]; then n=$((n + 1)); [ "$n" -le 1 ] || fail "$f has a second codename word-list line: $line"
  else fail "$f mentions a codename: $line"; fi
done < <(grep -iwE -- "$words" "$f")

# No SolidJS / common-frontend section; mentions only in "frozen" or sha-referenced lines.
echo "$headings" | grep -qiE 'solid|common-frontend' && fail "$f has a heading on Solid or common-frontend"
while IFS= read -r line; do
  echo "$line" | grep -qE 'frozen|[0-9a-f]{7,40}' || fail "$f mentions SolidJS/common-frontend outside a frozen sentence: $line"
done < <(grep -iE 'solidjs|common-frontend' "$f")

# Cited test paths exist.
while IFS= read -r p; do
  [ -e "$p" ] || fail "$f cites a missing path: $p"
done < <(grep -oE '`(tests/repo/[A-Za-z0-9_./-]+\.sh|crates/[A-Za-z0-9_-]+/tests/[A-Za-z0-9_./-]+\.rs|crates/web/islands/test/[A-Za-z0-9_./-]+\.ts)`' "$f" | tr -d '`' | sort -u)

for need in 'tests/repo/test_no_codenames.sh' 'crates/web/tests/gate.rs' 'crates/web/islands/test/budget.test.ts'; do
  grep -qF -- "$need" "$f" || fail "$f does not cite $need"
done

# CLAUDE.md.
row=$(grep -F 'specs/rules/frontend-rules.md' CLAUDE.md | grep '^|') || row=""
[ -n "$row" ] || fail "CLAUDE.md has no frontend-rules row"
for need in 'crates/web/templates' 'crates/web/static' 'crates/web/islands'; do
  echo "$row" | grep -qF -- "$need" || fail "CLAUDE.md frontend-rules row does not name $need"
done
echo "$row" | grep -qF 'packages/common-frontend' && fail "CLAUDE.md frontend-rules row still names packages/common-frontend"
grep -F 'site/' CLAUDE.md | grep -F 'frozen' | grep -qF 'v2-cutover' \
  || fail "CLAUDE.md has no line saying site/ is frozen until v2-cutover"

[ "$rc" = 0 ] || exit 1
echo "PASS: frontend rules describe the V2 frontend"
