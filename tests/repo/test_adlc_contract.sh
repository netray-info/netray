#!/usr/bin/env bash
# adlc.toml declares the contract recipe "adlc-verify", carries no no-ci key, and the recipe exists.
set -uo pipefail
cd "$(dirname "$0")/../../" || { echo "FAIL: cannot cd to repo root"; exit 1; }

[ -f adlc.toml ] || { echo "FAIL: adlc.toml missing"; exit 1; }

recipe=$(python3 -c '
import tomllib, sys
with open("adlc.toml", "rb") as f:
    d = tomllib.load(f)
print(d.get("contract", {}).get("recipe", ""))
') || { echo "FAIL: adlc.toml does not parse as TOML"; exit 1; }

[ "$recipe" = "adlc-verify" ] \
    || { echo "FAIL: [contract] recipe is '$recipe', expected 'adlc-verify'"; exit 1; }

if grep -v '^[[:space:]]*#' adlc.toml | grep -q 'no-ci'; then
    echo "FAIL: adlc.toml still contains a no-ci key"
    exit 1
fi

just --summary | tr ' ' '\n' | grep -qx 'adlc-verify' \
    || { echo "FAIL: recipe adlc-verify not in root justfile (just --summary)"; exit 1; }

echo "PASS: contract recipe adlc-verify declared, no no-ci, recipe exists"
