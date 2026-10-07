#!/usr/bin/env bash
# workflow-rules.md describes the monorepo ci.yml/release.yml setup, not the
# old per-service deploy/manifest-merge pipeline.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fail() { echo "FAIL: $1"; exit 1; }

f=specs/rules/workflow-rules.md
[ -f "$f" ] || fail "$f missing"

for need in 'ci.yml' 'release.yml'; do
  grep -qF "$need" "$f" || fail "$f does not mention $need"
done

bad=""
for pat in 'release-ref' 'deploy.yml' 'DEPLOY_WEBHOOK_SECRET' 'linux/amd64'; do
  grep -qF "$pat" "$f" && bad="$bad '$pat'"
done
grep -qi 'merge manifests' "$f" && bad="$bad 'merge manifests'"

[ -z "$bad" ] || fail "$f still mentions:$bad"

echo "PASS: workflow rules match the monorepo setup"
