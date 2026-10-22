#!/usr/bin/env bash
# v2-release-cleanup requirements 9 to 11, v2-v1-remnants requirements 4 to 9: the docs of the module and library crates describe V2
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

# v2-v1-remnants requirements 4 to 6 and 9: no crate keeps V1 record files (docs/, specs/,
# ROADMAP.md, SECURITY.md), no manifest calls a crate a service or names a former repository, and
# no crate-level doc comment names a V1 service.
MONOREPO='https://github.com/netray-info/netray'
CODENAMES='\b(prism|tlsight|spectra|beacon|ifconfig-rs|lens)\b|service ecosystem|pillar'

records=$(git ls-files ':(glob)crates/*/docs/**' ':(glob)crates/*/specs/**' ':(glob)crates/*/ROADMAP.md' ':(glob)crates/*/SECURITY.md' | head -5)
[ -z "$records" ] || fail "V1 record files are tracked: $records"

for manifest in $(git ls-files 'crates/*/Cargo.toml'); do
    desc=$(grep -E '^description *=' "$manifest")
    grep -qiE 'service' <<<"$desc" && fail "$manifest: description names a service: $desc"
    repo=$(grep -E '^repository *=' "$manifest" | sed -E 's/^repository *= *"(.*)"/\1/')
    [ -z "$repo" ] || [ "$repo" = "$MONOREPO" ] || fail "$manifest: repository is $repo, want $MONOREPO"
done

for lib in $(git ls-files 'crates/*/src/lib.rs' 'crates/common/src/*.rs'); do
    head=$(awk '/^[^\/]/{exit} {print}' "$lib")
    hits=$(grep -niE -- "$CODENAMES" <<<"$head" | head -2)
    [ -z "$hits" ] || fail "$lib: crate doc names V1: $hits"
done

# The root SECURITY.md covers the whole repository (requirement 8); the IP classification
# reference lives at the root (requirement 7).
sec=SECURITY.md
grep -q '^## Scope' "$sec" || fail "$sec has no Scope section"
grep -q 'mhost' "$sec" || fail "$sec does not put upstream mhost out of scope"
grep -q 'Report a vulnerability' "$sec" || fail "$sec lost the private report channel"
git ls-files --error-unmatch docs/ip-enrichment.md >/dev/null 2>&1 || fail "docs/ip-enrichment.md is not tracked"
for doc in crates/ip/README.md crates/ip/CLAUDE.md; do
    grep -qF '../../docs/ip-enrichment.md' "$doc" || fail "$doc does not link ../../docs/ip-enrichment.md"
done

[ "$failures" -eq 0 ] || exit 1
echo "PASS: crate docs describe V2 modules"
