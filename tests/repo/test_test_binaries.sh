#!/usr/bin/env bash
# v2-ci-speed R7: each merged crate links its integration tests as one test target `it`
# (tests/it/main.rs, one module per file). Linking ~400 test binaries dominated CI's compile time.
# A test file may stay its own target only with required-features, harness = false, or a listed
# reason. Read from cargo metadata, never from the TOML text.
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 1

MERGED="netray-web lens netray-engine netray-model netray-store netray-dns netray-tls netray-email netray-http netray-common"
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
    # every file under tests/it is a module, declared unconditionally in its parent module file
    itdir = os.path.join(cdir, "tests", "it")
    files = subprocess.run(["git", "ls-files", itdir], capture_output=True, text=True).stdout.split()
    import re
    def declared(parent, name):
        if not os.path.isfile(parent):
            return False
        lines = open(parent).read().splitlines()
        for i, l in enumerate(lines):
            if re.match(rf"^\s*(pub(\([^)]*\))? )?mod {name};", l):
                return not (i > 0 and lines[i - 1].strip().startswith("#[cfg"))
        return False
    for f in files:
        rel = os.path.relpath(f, itdir)
        if not rel.endswith(".rs") or rel == "main.rs":
            continue
        parts = rel[:-3].split("/")
        if parts[-1] == "mod":
            parts = parts[:-1]
        parent = os.path.join(itdir, "main.rs")
        for depth, name in enumerate(parts):
            if not declared(parent, name):
                fails.append(f"{f}: mod {name}; is not declared unconditionally in {parent}, so it never compiles")
                break
            parent = os.path.join(itdir, *parts[: depth + 1], "mod.rs")
# a tests/it outside the merged crates is never run by the contract, yet ADLC-RAN would name its files
merged_dirs = {os.path.relpath(os.path.dirname(p["manifest_path"]), root) for p in meta["packages"] if p["name"] in merged}
for f in subprocess.run(["git", "ls-files", "crates/*/tests/it/*"], capture_output=True, text=True).stdout.split():
    cdir = "/".join(f.split("/")[:2])
    if cdir not in merged_dirs:
        fails.append(f"{f}: {cdir} is not a merged crate (MERGED), so no it target runs it, yet the contract would report it as run")
print("\n".join(fails))
' <<<"$meta") || { echo "FAIL: check crashed: $out"; exit 1; }

# adlc counts a Rust test file as run only when it is a target's src_path; the contract names the
# module files of each `it` target after its cargo test passed (ADLC-RAN lines).
just --show test-rust-offline 2>/dev/null | grep -vE '^[[:space:]]*#' | grep -qF "sed 's/^/ADLC-RAN /'" \
    || out+=$'\n'"justfile: test-rust-offline prints no ADLC-RAN line for the tests/it module files (adlc orphans)"
out=$(sed '/^$/d' <<<"$out")

if [ -n "$out" ]; then
    echo "$out" | sed "s/^/FAIL: /"
    exit 1
fi
echo ok
