#!/usr/bin/env bash
# specs/features/v2-model: crates/engine names no module crate. Cargo is asked, not the TOML:
# `cargo metadata --no-deps` lists the dependencies of netray-engine by real package name, whatever
# the rename, key syntax or kind (normal, dev, build, target). netray-model and external crates
# only. A self-test runs the same query on tiny offline workspaces built from fixtures: five bad
# manifests it must flag, one good manifest (with lookalikes) it must pass.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

modules='beacon tlsight spectra prism ifconfig-rs lens netray-dns netray-tls netray-http netray-email netray-ip'

# engine_deps <workspace-dir>: print the package name of each dependency of netray-engine.
engine_deps() {
    local json
    json=$(cd "$1" && cargo metadata --no-deps --offline --format-version 1) || return 1
    printf '%s' "$json" | python3 -I -c '
import json, sys
for p in json.load(sys.stdin)["packages"]:
    if p["name"] == "netray-engine":
        for d in p["dependencies"]:
            print(d["name"])
' | sort -u
}

# module_deps <workspace-dir>: print each module crate netray-engine depends on; return 1 if
# the query itself failed.
module_deps() {
    local deps
    deps=$(engine_deps "$1") || return 1
    local m
    for m in $modules; do
        printf '%s\n' "$deps" | grep -qx -- "$m" && echo "$m"
    done
    return 0
}

fixdir=tests/repo/fixtures/engine-deps
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# workspace <case>: build a temp workspace with the case as engine/Cargo.toml; print its path.
workspace() {
    local w="$tmp/$1"
    mkdir -p "$w/engine/src" "$w/beacon"
    printf '[workspace]\nmembers = ["engine", "beacon"]\nresolver = "3"\n' > "$w/Cargo.toml"
    cp -R "$fixdir/beacon/." "$w/beacon/"
    cp "$fixdir/$1.toml" "$w/engine/Cargo.toml"
    : > "$w/engine/src/lib.rs"
    echo "$w"
}

for c in rename dotted target-inline dev build; do
    if [ ! -f "$fixdir/$c.toml" ]; then fail "$fixdir/$c.toml missing"; continue; fi
    out=$(module_deps "$(workspace "$c")") || { fail "cargo metadata failed on fixture $c"; continue; }
    printf '%s\n' "$out" | grep -qx beacon || fail "self-test: fixture $c not reported"
done
if [ -f "$fixdir/good.toml" ]; then
    out=$(module_deps "$(workspace good)") || fail "cargo metadata failed on fixture good"
    [ -z "$out" ] || fail "self-test: fixture good flagged: $out"
else
    fail "$fixdir/good.toml missing"
fi

if [ -f crates/engine/Cargo.toml ]; then
    out=$(module_deps .) || fail "cargo metadata failed on the repo"
    while IFS= read -r name; do
        [ -n "$name" ] && fail "netray-engine depends on module crate $name"
    done <<< "$out"
else
    fail "crates/engine/Cargo.toml missing"
fi

[ "$fails" -eq 0 ] || exit 1
echo "PASS: test_engine_names_no_module"
