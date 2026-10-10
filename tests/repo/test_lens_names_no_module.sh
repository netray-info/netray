#!/usr/bin/env bash
# specs/features/v2-modules (Phase 6, R20): lens names no module. crates/lens/src/backends/ holds
# no tracked file, and package `lens` has no normal dependency (cargo metadata `kind` null) on
# netray-dns, -tls, -http, -email or -ip; dev-dependencies are allowed. Cargo is asked, not the
# TOML. A self-test runs the same query on a tiny offline workspace: a normal dependency must be
# flagged, a dev-dependency must pass.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

# refused <workspace-dir> <manifest>: print each module crate the package at <manifest> (relative
# to <workspace-dir>) depends on with kind normal; print MISSING when no package has that manifest.
refused() {
    local json
    json=$(cd "$1" && cargo metadata --no-deps --offline --format-version 1) || return 1
    printf '%s' "$json" | python3 -I -c '
import json, os, sys
manifest = sys.argv[1]
modules = {"netray-dns", "netray-tls", "netray-http", "netray-email", "netray-ip"}
meta = json.load(sys.stdin)
pkgs = [p for p in meta["packages"] if os.path.realpath(p["manifest_path"]) == os.path.realpath(manifest)]
if not pkgs:
    print("MISSING")
for p in pkgs:
    for d in sorted({d["name"] for d in p["dependencies"] if d["kind"] is None and d["name"] in modules}):
        print(d)
' "$1/$2"
}

fixdir=tests/repo/fixtures/lens-deps
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# workspace <case>: temp workspace with the case as lens/Cargo.toml and a stub netray-dns.
workspace() {
    local w="$tmp/$1"
    mkdir -p "$w/lens/src"
    printf '[workspace]\nmembers = ["lens", "dns"]\nresolver = "3"\n' > "$w/Cargo.toml"
    cp -R "$fixdir/dns" "$w/dns"
    cp "$fixdir/$1.toml" "$w/lens/Cargo.toml"
    : > "$w/lens/src/lib.rs"
    echo "$w"
}

for c in normal dev; do [ -f "$fixdir/$c.toml" ] || fail "$fixdir/$c.toml missing"; done
if [ -f "$fixdir/normal.toml" ]; then
    out=$(refused "$(workspace normal)" lens/Cargo.toml) || fail "cargo metadata failed on fixture normal"
    printf '%s\n' "$out" | grep -qx netray-dns || fail "self-test: fixture normal did not report netray-dns (got: $out)"
fi
if [ -f "$fixdir/dev.toml" ]; then
    out=$(refused "$(workspace dev)" lens/Cargo.toml) || fail "cargo metadata failed on fixture dev"
    [ -z "$out" ] || fail "self-test: fixture dev flagged: $out"
fi

if [ -n "$(git ls-files crates/lens/src/backends)" ]; then
    fail "crates/lens/src/backends/ still holds tracked files:"
    git ls-files crates/lens/src/backends
fi

out=$(refused . crates/lens/Cargo.toml) || fail "cargo metadata failed on the repo"
while IFS= read -r name; do
    case "$name" in
        "") ;;
        MISSING) fail "crates/lens/Cargo.toml missing or not a workspace member" ;;
        *) fail "lens depends on $name" ;;
    esac
done <<< "$out"

[ "$fails" -eq 0 ] || exit 1
echo "PASS: test_lens_names_no_module"
