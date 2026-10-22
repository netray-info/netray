#!/usr/bin/env bash
# v2-release-cleanup requirements 9 to 11: the docs of the module and library crates describe V2
# modules of `netray serve`, not the V1 services. Each CHANGELOG opens with a pointer to the root
# CHANGELOG before its first entry; the READMEs and CLAUDE.md files name no V1 subcommand,
# frontend, per-service config file or retired env prefix.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

failures=0
fail() { echo "FAIL: $*"; failures=$((failures + 1)); }

POINTER='root [CHANGELOG.md](../../CHANGELOG.md)'
STALE='netray (dns|tls|http|email|ip) |frontend/|SolidJS|(prism|tlsight|spectra|beacon|ifconfig)\.toml|(PRISM|TLSIGHT|SPECTRA|BEACON|IFCONFIG|LENS)_|NETRAY_(DNS|TLS|HTTP|EMAIL|IP)_'

for c in dns tls http email ip common; do
    log="crates/$c/CHANGELOG.md"
    if [ ! -f "$log" ]; then
        fail "$log missing"
    else
        head=$(awk '/^## /{exit} {print}' "$log")
        grep -qF -- "$POINTER" <<<"$head" || fail "$log: no pointer to the $POINTER before its first entry"
    fi
    for doc in "crates/$c/README.md" "crates/$c/CLAUDE.md"; do
        [ -f "$doc" ] || { fail "$doc missing"; continue; }
        hits=$(grep -nE -- "$STALE" "$doc" | head -3)
        [ -z "$hits" ] || fail "$doc names V1 as current: $hits"
    done
done

[ "$failures" -eq 0 ] || exit 1
echo "PASS: crate docs describe V2 modules"
