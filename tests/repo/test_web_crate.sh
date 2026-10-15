#!/usr/bin/env bash
# specs/features/v2-design-system, requirement 1 (with 3): crates/web is the package netray-web,
# edition 2024, a library with no route, depending on netray-common by path and on askama 0.16;
# the netray binary does not depend on it. The crate's shape includes its token source
# (tokens.json) and a build.rs. Cargo is asked via `cargo metadata --no-deps --offline`.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

[ -f crates/web/tokens.json ] || fail "crates/web/tokens.json is missing (the crate's token source)"
[ -f crates/web/build.rs ] || fail "crates/web/build.rs is missing"

if [ -d crates/web/src ] && grep -rEq '\bRouter\b' crates/web/src; then
    fail "crates/web/src names a Router (the crate holds no route)"
fi

json=$(cargo metadata --no-deps --offline --format-version 1) || { fail "cargo metadata failed"; json=""; }
if [ -n "$json" ]; then
    out=$(printf '%s' "$json" | python3 -I -c '
import json, os, sys
meta = json.load(sys.stdin)
root = meta["workspace_root"]
errs = []
web = [p for p in meta["packages"] if p["name"] == "netray-web"]
if not web:
    errs.append("package netray-web not found")
else:
    p = web[0]
    rel = os.path.relpath(p["manifest_path"], root)
    print("netray-web manifest=%s edition=%s" % (rel, p["edition"]))
    if rel != "crates/web/Cargo.toml": errs.append("netray-web manifest is " + rel)
    if p["edition"] != "2024": errs.append("netray-web edition is " + p["edition"])
    kinds = sorted({k for t in p["targets"] for k in t["kind"]})
    print("netray-web target kinds=%s" % kinds)
    if "lib" not in kinds: errs.append("netray-web has no lib target")
    deps = {d["name"]: d for d in p["dependencies"]}
    print("netray-web deps=%s" % sorted(deps))
    c = deps.get("netray-common")
    if not c or not c.get("path"): errs.append("netray-web does not depend on netray-common by path")
    a = deps.get("askama")
    if not a or not (a["req"].lstrip("^=~ ").startswith("0.16")): errs.append("askama req is %r, want 0.16" % (a and a["req"]))
    if "axum" in deps: errs.append("netray-web depends on axum")
bins = [p for p in meta["packages"] if p["name"] == "netray"]
if not bins:
    errs.append("package netray not found")
else:
    nd = sorted(d["name"] for d in bins[0]["dependencies"])
    print("netray deps=%s" % nd)
    if "netray-web" in nd: errs.append("netray depends on netray-web")
for e in errs: print("ERR " + e)
')
    printf '%s\n' "$out" | grep -v '^ERR '
    while IFS= read -r e; do fail "${e#ERR }"; done < <(printf '%s\n' "$out" | grep '^ERR ')
fi

[ "$fails" -eq 0 ] || exit 1
echo "PASS: test_web_crate"
