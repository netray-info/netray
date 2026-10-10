#!/usr/bin/env bash
# specs/features/v2-catalogue Phase 3, C9: the (check id, title) pairs of CHECK_LABELS in
# crates/lens/frontend/src/lib/checkMeta.ts (keyed by the name after the protocol prefix, unique
# across protocols in v3) equal the (id, title) pairs of the [[check]] tables in
# crates/engine/catalogue/checks.toml, as full sets. Every difference is printed. A self-test runs
# the same comparison on tiny fixtures: a drifted title, a missing and an extra id it must flag,
# and an identical pair it must pass.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

# diff_sets <checkMeta.ts> <checks.toml>: print one line per difference, nothing when the sets are
# equal; exit 2 when either file cannot be parsed into a non-empty set.
diff_sets() {
    python3 -I -c '
import re, sys, tomllib
ts = open(sys.argv[1], encoding="utf-8").read()
m = re.search(r"export const CHECK_LABELS[^=]*=\s*\{(.*?)^\}", ts, re.S | re.M)
if not m:
    print("cannot find CHECK_LABELS in " + sys.argv[1]); sys.exit(2)
pair = re.compile(r"^\s*(?:\x27([^\x27]+)\x27|\"([^\"]+)\"|([A-Za-z_][\w.]*))\s*:\s*(?:\x27((?:[^\x27\\]|\\.)*)\x27|\"((?:[^\"\\]|\\.)*)\")\s*,?\s*(?://.*)?$")
meta = set()
for line in m.group(1).splitlines():
    if not line.strip() or line.strip().startswith("//"):
        continue
    p = pair.match(line)
    if not p:
        print("unparsed CHECK_LABELS line: " + line.strip()); sys.exit(2)
    key = p.group(1) or p.group(2) or p.group(3)
    title = p.group(4) if p.group(4) is not None else p.group(5)
    meta.add((key, title.replace("\\\x27", "\x27")))
with open(sys.argv[2], "rb") as f:
    cat = {(c["id"].split(".", 1)[1], c["title"]) for c in tomllib.load(f).get("check", [])}
if not meta or not cat:
    print("empty set: checkMeta=%d catalogue=%d" % (len(meta), len(cat))); sys.exit(2)
for k, t in sorted(cat - meta):
    print("in checks.toml only: %s = %r" % (k, t))
for k, t in sorted(meta - cat):
    print("in checkMeta.ts only: %s = %r" % (k, t))
' "$1" "$2"
}

ts=crates/lens/frontend/src/lib/checkMeta.ts
toml=crates/engine/catalogue/checks.toml
for f in "$ts" "$toml"; do [ -f "$f" ] || { echo "FAIL: missing $f"; exit 1; }; done
command -v python3 >/dev/null || { echo "FAIL: python3 not found"; exit 1; }

# --- self-test ---
tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT
cat > "$tmp/cat.toml" <<'T'
[[check]]
id = "dns.spf"
title = "SPF record"
[[check]]
id = "tls.expiry"
title = "Expiry window"
T
write_ts() { printf "export const CHECK_LABELS: Record<string, string> = {\n  // DNS\n%s\n};\n\nexport const CHECK_DESCRIPTIONS: Record<string, string> = {\n  'x': 'y',\n};\n" "$1" > "$tmp/meta.ts"; }

write_ts "  'spf': 'SPF record',
  'expiry': 'Expiry window',"
out=$(diff_sets "$tmp/meta.ts" "$tmp/cat.toml"); rc=$?
{ [ "$rc" -eq 0 ] && [ -z "$out" ]; } || fail "self-test: identical sets flagged: $out"

write_ts "  'spf': 'SPF Record',
  'expiry': 'Expiry window',"
out=$(diff_sets "$tmp/meta.ts" "$tmp/cat.toml")
echo "$out" | grep -q "spf" || fail "self-test: drifted title not flagged"

write_ts "  'spf': 'SPF record',
  'extra': 'Extra',"
out=$(diff_sets "$tmp/meta.ts" "$tmp/cat.toml")
echo "$out" | grep -q "checks.toml only: expiry" || fail "self-test: missing id not flagged"
echo "$out" | grep -q "checkMeta.ts only: extra" || fail "self-test: extra id not flagged"
[ "$fails" -eq 0 ] || exit 1

# --- the check ---
out=$(diff_sets "$ts" "$toml"); rc=$?
if [ "$rc" -ne 0 ] || [ -n "$out" ]; then
    echo "$out"
    fail "checkMeta.ts CHECK_LABELS differs from the check catalogue (rc=$rc)"
fi

exit $fails
