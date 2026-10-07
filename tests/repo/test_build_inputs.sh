#!/usr/bin/env bash
# Tracked build inputs carry no publish step, GitHub Packages registry, per-repo
# make fan-out, sibling-checkout path, or registry auth token.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fail() { echo "FAIL: $1"; exit 1; }

# Build inputs: manifests, npm config, justfiles, Dockerfiles, build scripts,
# and root-level workflows only (per-crate .github is inert and removed later).
files=$(git ls-files \
  | grep -E '(^|/)(Cargo\.toml|package\.json|\.npmrc|justfile|build\.rs)$|(^|/)Dockerfile[^/]*$|^\.github/workflows/[^/]+\.yml$' \
  | sort -u)

[ -n "$files" ] || fail "no tracked build inputs found"

offenders=""
while IFS= read -r f; do
  [ -f "$f" ] || continue
  if grep -qE 'cargo publish|npm\.pkg\.github\.com|make -C|\.\./netray-common|NODE_AUTH_TOKEN' "$f"; then
    offenders="$offenders $f"
  fi
done <<< "$files"

[ -z "$offenders" ] || fail "forbidden build-input strings in:$offenders"

echo "PASS: build inputs clean"
