#!/usr/bin/env bash
# specs/features/v2-cutover Phase 6 (C3): crates/web does not depend on the package `lens`
# (crates/lens), of any kind (normal, dev, build, target), and `netray serve` reaches no lens code:
# in crates/netray only `src/v1.rs`, the V1 `netray lens` subcommand the operator keeps runnable as
# the reference until Phase 7 (Decision log 2026-10-19), names `lens`. Cargo is asked, not the
# TOML: `cargo metadata --no-deps` lists dependencies by real package name, whatever the rename or
# key syntax. Packages are found by manifest path, not by name.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

json=$(cargo metadata --no-deps --offline --format-version 1) || { echo "FAIL: cargo metadata failed"; exit 1; }

out=$(printf '%s' "$json" | python3 -I -c '
import json, os, sys
meta = json.load(sys.stdin)
for manifest in sys.argv[1:]:
    pkgs = [p for p in meta["packages"] if os.path.realpath(p["manifest_path"]) == os.path.realpath(manifest)]
    if not pkgs:
        print("FAIL: %s missing or not a workspace member" % manifest)
    for p in pkgs:
        for d in p["dependencies"]:
            if d["name"] == "lens":
                print("FAIL: package %s (%s) depends on lens (kind: %s)" % (p["name"], manifest, d["kind"] or "normal"))
' crates/web/Cargo.toml) || { echo "FAIL: dependency query failed"; exit 1; }

uses=$(git ls-files 'crates/netray/src/*.rs' 'crates/netray/tests/*.rs' \
    | grep -v '^crates/netray/src/v1\.rs$' \
    | xargs grep -nE '(^|[^A-Za-z0-9_])lens::' 2>/dev/null)
if [ -n "$uses" ]; then
    out="${out:+$out
}FAIL: lens named outside crates/netray/src/v1.rs:
$uses"
fi

if [ -n "$out" ]; then
    printf '%s\n' "$out"
    exit 1
fi
echo "PASS: test_no_lens_dependency"
