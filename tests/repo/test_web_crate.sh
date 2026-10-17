#!/usr/bin/env bash
# specs/features/v2-design-system, requirement 1 (with 3), amended by v2-pages requirement 8:
# crates/web is the package netray-web, edition 2024, a library; a Router only under entries/,
# pages/, api/; depends on netray-common and netray-engine by path, askama 0.16, axum, and
# sha2/base64 via [workspace.dependencies]. The crate's shape includes its token source
# (tokens.json) and a build.rs. Cargo is asked via `cargo metadata --no-deps --offline`.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

[ -f crates/web/tokens.json ] || fail "crates/web/tokens.json is missing (the crate's token source)"
[ -f crates/web/build.rs ] || fail "crates/web/build.rs is missing"

# Requirement 8: a Router is allowed under entries/, pages/ and api/ and nowhere else in the crate.
router_scan() {
    local dir="$1" f rel
    [ -d "$dir" ] || return 0
    while IFS= read -r f; do
        rel="${f#"$dir"/}"
        case "$rel" in
            entries/*|pages/*|api/*) ;;
            *) echo "$rel" ;;
        esac
    done < <(grep -rElw '\bRouter\b' "$dir" 2>/dev/null | sort)
}

while IFS= read -r f; do
    [ -n "$f" ] && fail "crates/web/src/$f names a Router (allowed only under entries/, pages/, api/)"
done < <(router_scan crates/web/src)

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
    for need in ("axum", "sha2", "base64"):
        if need not in deps: errs.append("netray-web does not depend on " + need)
    e = deps.get("netray-engine")
    if not e or not e.get("path"): errs.append("netray-web does not depend on netray-engine by path")
for e in errs: print("ERR " + e)
')
    printf '%s\n' "$out" | grep -v '^ERR '
    while IFS= read -r e; do fail "${e#ERR }"; done < <(printf '%s\n' "$out" | grep '^ERR ')
fi

# base64 and sha2 resolve through [workspace.dependencies].
wsout=$(python3 -I -c '
import tomllib
root = tomllib.load(open("Cargo.toml", "rb"))
web = tomllib.load(open("crates/web/Cargo.toml", "rb"))
wsd = root.get("workspace", {}).get("dependencies", {})
d = web.get("dependencies", {})
for n in ("base64", "sha2"):
    if n not in wsd: print("ERR %s is not in the root [workspace.dependencies]" % n)
    v = d.get(n)
    if not (isinstance(v, dict) and v.get("workspace") is True):
        print("ERR crates/web/Cargo.toml does not declare %s with workspace = true" % n)
')
while IFS= read -r e; do [ -n "$e" ] && fail "${e#ERR }"; done <<< "$wsout"

# Self-test: the Router scan reports a Router planted outside the allowed directories.
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cp -R crates/web/src "$tmp/src"
printf '%s\n' 'pub fn r() -> axum::Router { todo!() }' >> "$tmp/src/components.rs"
if ! router_scan "$tmp/src" | grep -qx 'components.rs'; then
    fail "self-test: the Router scan missed a Router planted in components.rs"
fi

[ "$fails" -eq 0 ] || exit 1
echo "PASS: test_web_crate"
