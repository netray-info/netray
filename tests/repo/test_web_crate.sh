#!/usr/bin/env bash
# specs/features/v2-design-system, requirement 1 (with 3), amended by v2-pages requirement 8,
# v2-entries requirement 9 and v2-api requirement 1: crates/web is the package netray-web, edition
# 2024, a library; a `Router` appears only under crates/web/src/entries/, pages/ and api/; it
# depends on netray-common and netray-engine by path, askama 0.16, axum, and sha2/base64 via
# [workspace.dependencies]; the netray binary depends on it (crates/netray builds the parts; lens is
# gone, v2-cutover Phase 7). The crate's shape includes its token source (tokens.json) and a build.rs. The
# Router scan is a function over a root dir, self-tested on scratch input. Cargo is asked via
# `cargo metadata --no-deps --offline`.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

[ -f crates/web/tokens.json ] || fail "crates/web/tokens.json is missing (the crate's token source)"
[ -f crates/web/build.rs ] || fail "crates/web/build.rs is missing"

# router_offenders <src-root>: files naming a Router outside entries/, pages/, api/ and host/.
router_offenders() {
    local root=$1 f rel
    grep -rlE '\bRouter\b' "$root" 2>/dev/null | while IFS= read -r f; do
        rel=${f#"$root"/}
        case "$rel" in
            entries/*|pages/*|api/*|host/*) ;;
            *) echo "$rel" ;;
        esac
    done
}

scratch=$(mktemp -d) || exit 1
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/ok/entries" "$scratch/ok/pages" "$scratch/ok/api" "$scratch/ok/host" "$scratch/bad/entries"
for f in entries/router.rs pages/x.rs api/x.rs host/x.rs; do echo 'fn r() -> Router {}' > "$scratch/ok/$f"; done
echo 'fn r() -> Router {}' > "$scratch/bad/entries/router.rs"
echo 'fn r() -> Router {}' > "$scratch/bad/components.rs"
[ -z "$(router_offenders "$scratch/ok")" ] || fail "self-test: a Router under entries/, pages/, api/ or host/ was refused"
[ "$(router_offenders "$scratch/bad")" = "components.rs" ] || fail "self-test: a Router in components.rs was not the one refusal"

if [ -d crates/web/src ]; then
    while IFS= read -r f; do
        fail "crates/web/src/$f names a Router outside entries/, pages/, api/ and host/"
    done < <(router_offenders crates/web/src)
fi

# manifest_report: reads `cargo metadata` JSON on stdin; prints facts and `ERR <what>` lines.
manifest_report() {
    python3 -I -c '
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
bins = [p for p in meta["packages"] if p["name"] == "netray"]
if not bins:
    errs.append("package netray not found")
else:
    nd = sorted(d["name"] for d in bins[0]["dependencies"])
    print("netray deps=%s" % nd)
    if "netray-web" not in nd: errs.append("netray does not depend on netray-web")
for e in errs: print("ERR " + e)
'
}

# self-test: a synthetic metadata with edition 2021 is refused on the edition.
synthetic='{"workspace_root":"/w","packages":[{"name":"netray-web","manifest_path":"/w/crates/web/Cargo.toml","edition":"2021","targets":[{"kind":["lib"]}],"dependencies":[]}]}'
printf '%s' "$synthetic" | manifest_report | grep -q '^ERR netray-web edition is 2021$' \
    || fail "self-test: edition 2021 was not refused"

json=$(cargo metadata --no-deps --offline --format-version 1) || { fail "cargo metadata failed"; json=""; }
if [ -n "$json" ]; then
    out=$(printf '%s' "$json" | manifest_report)

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
trap 'rm -rf "$tmp" "$scratch"' EXIT
cp -R crates/web/src "$tmp/src"
printf '%s\n' 'pub fn r() -> axum::Router { todo!() }' >> "$tmp/src/components.rs"
if ! router_offenders "$tmp/src" | grep -qx 'components.rs'; then
    fail "self-test: the Router scan missed a Router planted in components.rs"
fi

[ "$fails" -eq 0 ] || exit 1
echo "PASS: test_web_crate"
