#!/usr/bin/env bash
# v2-ci-speed R7: each merged crate links its integration tests as one test target `it`
# (tests/it/main.rs, one module per file). Linking ~400 test binaries dominated CI's compile time.
# A test file may stay its own target only with required-features, harness = false, or a listed
# reason. Read from cargo metadata, never from the TOML text.
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 1

MERGED="netray-web lens netray-engine netray-model netray-store netray-dns netray-tls netray-email netray-http"
# package:target -- reason
EXCEPTIONS="lens:admission_config lens:history_config"   # set LENS_* env vars lens's loader reads as keys

meta=$(cargo metadata --no-deps --format-version 1 2>/dev/null) || { echo "FAIL: cargo metadata"; exit 1; }
out=$(MERGED="$MERGED" EXCEPTIONS="$EXCEPTIONS" python3 -c '
import json, os, sys, subprocess
meta = json.load(sys.stdin)
merged = os.environ["MERGED"].split()
exc = set(os.environ["EXCEPTIONS"].split())
root = os.getcwd()
fails = []
for pkg in meta["packages"]:
    if pkg["name"] not in merged:
        continue
    n = pkg["name"]
    cdir = os.path.relpath(os.path.dirname(pkg["manifest_path"]), root)
    tests = [t for t in pkg["targets"] if "test" in t["kind"]]
    names = {t["name"] for t in tests}
    if "it" not in names:
        fails.append(f"{n}: no test target it ({cdir}/tests/it/main.rs)")
    for t in tests:
        tn, sp = t["name"], os.path.relpath(t["src_path"], root)
        if tn == "it":
            continue
        if t.get("required-features") or not t.get("test", True) or f"{n}:{tn}" in exc:
            continue
        if not t.get("harness", True):
            continue
        fails.append(f"{n}: test target {tn} ({sp}) belongs in {cdir}/tests/it/ as a module")
    # every file under tests/it is a module of main.rs
    itdir = os.path.join(cdir, "tests", "it")
    files = subprocess.run(["git", "ls-files", itdir], capture_output=True, text=True).stdout.split()
    main = os.path.join(itdir, "main.rs")
    decl = set()
    if os.path.isfile(main):
        import re
        decl = set(re.findall(r"^\s*mod (\w+);", open(main).read(), re.M))
    for f in files:
        rel = os.path.relpath(f, itdir)
        if not rel.endswith(".rs") or rel == "main.rs":
            continue
        top = rel.split("/")[0]
        mod = top[:-3] if top.endswith(".rs") else top
        if mod not in decl:
            fails.append(f"{f}: not declared in {main} (mod {mod};), so it never compiles")
print("\n".join(fails))
' <<<"$meta") || { echo "FAIL: check crashed: $out"; exit 1; }

# adlc counts a Rust test file as run only when it is a target's src_path; the contract names the
# module files of each `it` target after its cargo test passed (ADLC-RAN lines).
just --show test-rust-offline 2>/dev/null | grep -q 'ADLC-RAN' \
    || out+=$'\n'"justfile: test-rust-offline prints no ADLC-RAN line for the tests/it module files (adlc orphans)"
out=$(sed '/^$/d' <<<"$out")

if [ -n "$out" ]; then
    echo "$out" | sed "s/^/FAIL: /"
    exit 1
fi
echo ok
