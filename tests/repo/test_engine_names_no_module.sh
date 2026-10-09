#!/usr/bin/env bash
# specs/features/v2-model: crates/engine depends on netray-model only among workspace crates and
# names no module crate (R4, R5); crates/model depends on no workspace crate (R1). Cargo is asked,
# not the TOML: `cargo metadata --no-deps` lists each package's dependencies by real package name,
# whatever the rename, key syntax or kind (normal, dev, build, target), and the package is found
# by its manifest path, not its name. A self-test runs the same query on tiny offline workspaces
# built from fixtures: bad manifests it must flag, one good manifest (with lookalikes) it must pass.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

modules='beacon tlsight spectra prism ifconfig-rs lens netray-dns netray-tls netray-http netray-email netray-ip'

# refused <workspace-dir> <manifest> <allowed>: print each dependency of the package at
# <manifest> (relative to <workspace-dir>) that is a module crate or a workspace member other
# than <allowed> (space-separated); print MISSING when no package has that manifest. Return 1
# if the query itself failed.
refused() {
    local json
    json=$(cd "$1" && cargo metadata --no-deps --offline --format-version 1) || return 1
    printf '%s' "$json" | python3 -I -c '
import json, os, sys
manifest, allowed, modules = sys.argv[1], set(sys.argv[2].split()), set(sys.argv[3].split())
meta = json.load(sys.stdin)
members = {p["name"] for p in meta["packages"]}
pkgs = [p for p in meta["packages"] if os.path.realpath(p["manifest_path"]) == os.path.realpath(manifest)]
if not pkgs:
    print("MISSING")
for p in pkgs:
    for d in sorted({d["name"] for d in p["dependencies"]}):
        if d in modules or (d in members and d not in allowed):
            print(d)
' "$1/$2" "$3" "$modules"
}

fixdir=tests/repo/fixtures/engine-deps
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# workspace <case>: build a temp workspace with the case as engine/Cargo.toml; print its path.
workspace() {
    local w="$tmp/$1" c
    mkdir -p "$w/engine/src"
    printf '[workspace]\nmembers = ["engine", "beacon", "model", "common"]\nresolver = "3"\n' > "$w/Cargo.toml"
    for c in beacon model common; do cp -R "$fixdir/$c" "$w/$c"; done
    cp "$fixdir/$1.toml" "$w/engine/Cargo.toml"
    : > "$w/engine/src/lib.rs"
    echo "$w"
}

# case_flags <case> <expected>: the fixture must report <expected> as a refused dependency.
case_flags() {
    local out
    if [ ! -f "$fixdir/$1.toml" ]; then fail "$fixdir/$1.toml missing"; return; fi
    out=$(refused "$(workspace "$1")" engine/Cargo.toml netray-model) || { fail "cargo metadata failed on fixture $1"; return; }
    printf '%s\n' "$out" | grep -qx -- "$2" || fail "self-test: fixture $1 did not report $2 (got: $out)"
}
for c in rename dotted target-inline dev build renamed-package; do case_flags "$c" beacon; done
case_flags member netray-common

if [ -f "$fixdir/good.toml" ]; then
    out=$(refused "$(workspace good)" engine/Cargo.toml netray-model) || fail "cargo metadata failed on fixture good"
    [ -z "$out" ] || fail "self-test: fixture good flagged: $out"
else
    fail "$fixdir/good.toml missing"
fi

# check <manifest> <allowed>: the repo's package at <manifest> refuses nothing.
check() {
    local out name
    out=$(refused . "$1" "$2") || { fail "cargo metadata failed on the repo"; return; }
    while IFS= read -r name; do
        case "$name" in
            "") ;;
            MISSING) fail "$1 missing or not a workspace member" ;;
            *) fail "$1 depends on $name" ;;
        esac
    done <<< "$out"
}
check crates/engine/Cargo.toml netray-model
check crates/model/Cargo.toml ""

[ "$fails" -eq 0 ] || exit 1
echo "PASS: test_engine_names_no_module"
