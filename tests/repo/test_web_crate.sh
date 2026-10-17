#!/usr/bin/env bash
# specs/features/v2-design-system, requirement 1 (with 3): crates/web is the package netray-web,
# edition 2024, depending on netray-common by path and on askama 0.16; the netray binary does not
# depend on it. The crate's shape includes its token source (tokens.json) and a build.rs.
# specs/features/v2-entries, requirement 9: a `Router` appears only under crates/web/src/entries/,
# pages/ and api/ (the allow-list v2-pages and v2-api share), axum is allowed as a dependency, and
# lens depends on netray-web. The Router scan is a function over a root dir, self-tested on scratch
# input. Cargo is asked via `cargo metadata --no-deps --offline`.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

[ -f crates/web/tokens.json ] || fail "crates/web/tokens.json is missing (the crate's token source)"
[ -f crates/web/build.rs ] || fail "crates/web/build.rs is missing"

# router_offenders <src-root>: files naming a Router outside entries/, pages/ and api/.
router_offenders() {
    local root=$1 f rel
    grep -rlE '\bRouter\b' "$root" 2>/dev/null | while IFS= read -r f; do
        rel=${f#"$root"/}
        case "$rel" in
            entries/*|pages/*|api/*) ;;
            *) echo "$rel" ;;
        esac
    done
}

scratch=$(mktemp -d) || exit 1
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/ok/entries" "$scratch/ok/pages" "$scratch/ok/api" "$scratch/bad/entries"
for f in entries/router.rs pages/x.rs api/x.rs; do echo 'fn r() -> Router {}' > "$scratch/ok/$f"; done
echo 'fn r() -> Router {}' > "$scratch/bad/entries/router.rs"
echo 'fn r() -> Router {}' > "$scratch/bad/components.rs"
[ -z "$(router_offenders "$scratch/ok")" ] || fail "self-test: a Router under entries/, pages/ or api/ was refused"
[ "$(router_offenders "$scratch/bad")" = "components.rs" ] || fail "self-test: a Router in components.rs was not the one refusal"

if [ -d crates/web/src ]; then
    while IFS= read -r f; do
        fail "crates/web/src/$f names a Router outside entries/, pages/ and api/"
    done < <(router_offenders crates/web/src)
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
bins = [p for p in meta["packages"] if p["name"] == "netray"]
if not bins:
    errs.append("package netray not found")
else:
    nd = sorted(d["name"] for d in bins[0]["dependencies"])
    print("netray deps=%s" % nd)
    if "netray-web" in nd: errs.append("netray depends on netray-web")
lens = [p for p in meta["packages"] if p["name"] == "lens"]
if not lens:
    errs.append("package lens not found")
elif not any(d["name"] == "netray-web" and d.get("kind") is None for d in lens[0]["dependencies"]):
    errs.append("lens does not depend on netray-web")
for e in errs: print("ERR " + e)
')
    printf '%s\n' "$out" | grep -v '^ERR '
    while IFS= read -r e; do fail "${e#ERR }"; done < <(printf '%s\n' "$out" | grep '^ERR ')
fi

[ "$fails" -eq 0 ] || exit 1
echo "PASS: test_web_crate"
