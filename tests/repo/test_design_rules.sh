#!/usr/bin/env bash
# v2-design-gate Requirement 8: specs/rules/design-rules.md states the design check's method, and
# CLAUDE.md's rules table names it for the three V2 frontend paths.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

rc=0
fail() { echo "FAIL: $1"; rc=1; }

f=specs/rules/design-rules.md
[ -f "$f" ] || { echo "FAIL: $f missing"; exit 1; }

# The five classes and the measured tolerances.
for need in spacing typography colour 'missing or extra' hierarchy '4 px' '1 px' 'lowercase hex'; do
  grep -qiF -- "$need" "$f" || fail "$f does not name '$need'"
done
# What does not count.
for need in 'sample data' 'rendering noise'; do
  grep -qiF -- "$need" "$f" || fail "$f does not say '$need' does not count"
done
# The three dispositions.
for need in rebuild 'deliberately different' 'adjust canvas' accepted 'decision log' 're-export'; do
  grep -qiF -- "$need" "$f" || fail "$f does not name the disposition term '$need'"
done
# Rows start open, the visual channel, and the place in a UI feature's flow.
grep -qF '`open`' "$f" || fail "$f does not say every row starts \`open\`"
grep -qF 'adlc next' "$f" || fail "$f does not say open rows keep adlc next on implement"
grep -qiF 'side-by-side' "$f" || fail "$f does not name the side-by-side images"
grep -qF 'just design-check' "$f" || fail "$f does not name just design-check"
grep -qiE 'before (the )?review' "$f" || fail "$f does not say a UI feature runs the check before review"

# CLAUDE.md's rules table names it with the three frontend paths.
row=$(grep -F '[`specs/rules/design-rules.md`](specs/rules/design-rules.md)' CLAUDE.md | head -n 1)
[ -n "$row" ] || fail "CLAUDE.md's rules table has no row for specs/rules/design-rules.md"
for p in crates/web/templates crates/web/static crates/web/islands; do
  case "$row" in *"\`$p\`"*) ;; *) fail "CLAUDE.md's design-rules row does not name \`$p\`" ;; esac
done

[ "$rc" -eq 0 ] && echo "ok: design rules and their CLAUDE.md row"
exit "$rc"
