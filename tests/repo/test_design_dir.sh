#!/usr/bin/env bash
# v2-design-dir-path Requirements 1-3: the design-check recipe and specs/rules/design-rules.md
# default the private canvas to the netray.info checkout beside the main checkout.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

rc=0
fail() { echo "FAIL: $1"; rc=1; }

rules=specs/rules/design-rules.md
recipe=$(awk '/^design-check/{on=1; print; next} on && /^[^ \t#]/{exit} on' justfile)
[ -n "$recipe" ] || { echo "FAIL: justfile has no design-check recipe"; exit 1; }

# R1: the default sits behind the parameter expansion, so an explicit value wins.
grep -qF 'NETRAY_DESIGN_DIR="${NETRAY_DESIGN_DIR:-$main_checkout/../netray.info/specs/design/canvas}"' <<<"$recipe" \
  || fail "design-check does not default NETRAY_DESIGN_DIR to \$main_checkout/../netray.info/specs/design/canvas"

# R2: the rules name the netray.info checkout and the default.
grep -qF '../netray.info/specs/design/canvas' "$rules" || fail "$rules does not name ../netray.info/specs/design/canvas"
grep -qF 'netray.info checkout' "$rules" || fail "$rules does not name the netray.info checkout"

# R3: neither names the old location.
for old in netray-ops specs/netray/design; do
  grep -qF -- "$old" justfile && fail "justfile still names '$old'"
  grep -qF -- "$old" "$rules" && fail "$rules still names '$old'"
done

[ "$rc" -eq 0 ] && echo "ok: design canvas defaults to the netray.info checkout"
exit "$rc"
