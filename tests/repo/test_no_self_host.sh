#!/usr/bin/env bash
# No tracked file promises self-hosting. Historical records (specs/, docs/done/,
# CHANGELOG.md), this test directory and the root CONTRIBUTING.md (which states
# that self-hosting is unsupported) are exempt.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fail() { echo "FAIL: $1"; exit 1; }

hits=$(git grep -ilE 'self[- ]?host' -- . \
  ':(exclude,glob)**/specs/**' \
  ':(exclude)specs' \
  ':(exclude,glob)**/docs/done/**' \
  ':(exclude,glob)**/CHANGELOG.md' \
  ':(exclude)tests/repo' \
  ':(exclude)CONTRIBUTING.md' 2>/dev/null | sort -u)

[ -z "$hits" ] || fail "self-hosting wording in: $(echo "$hits" | tr '\n' ' ')"

echo "PASS: no self-host promises"
