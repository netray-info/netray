#!/usr/bin/env bash
# The published image carries no data files (specs/features/monorepo-no-data-in-image):
# no stage from ifconfig-rs-data, nothing copied into /netray/data, release.yml proves the
# absence before pushing, the rules say so, and `just image` needs no GHCR login.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

for f in Dockerfile .github/workflows/release.yml specs/rules/workflow-rules.md README.md CLAUDE.md; do
    [ -f "$f" ] || { echo "FAIL: $f missing"; exit 1; }
done

# Requirement 1: the Dockerfile.
grep -q 'ifconfig-rs-data' Dockerfile && fail "Dockerfile references ifconfig-rs-data"
grep -qE '^COPY[^#]*/netray/data' Dockerfile && fail "Dockerfile copies into /netray/data"
last_workdir=$(grep -E '^WORKDIR[[:space:]]' Dockerfile | tail -n1 | sed -E 's/^WORKDIR[[:space:]]+//; s/[[:space:]]+$//')
[ "$last_workdir" = "/netray" ] || fail "last WORKDIR is '${last_workdir}', expected /netray"

# Requirement 2: release.yml reads no data and proves the absence before pushing.
rel=.github/workflows/release.yml
grep -q 'ifconfig-rs-data' "$rel" && fail "release.yml references ifconfig-rs-data"
grep -qE '(IFCONFIG|NETRAY_IP)_GEOIP_' "$rel" && fail "release.yml sets GeoIP paths for the smoke test"
check_line=$(grep -nE 'mmdb' "$rel" | head -n1 | cut -d: -f1)
data_line=$(grep -nE '/netray/data' "$rel" | head -n1 | cut -d: -f1)
push_line=$(grep -nE 'docker push' "$rel" | head -n1 | cut -d: -f1)
# The check runs as root (root-only dirs are searchable) and must not fail on a clean
# image: `[ -d … ] && find` returns 1 when the dir is absent and aborts `bash -e`.
grep -qE 'docker run[^|]*--user 0[^|]*--entrypoint sh' "$rel" || fail "release.yml data check does not run as root (--user 0)"
grep -qE '\[ -d /netray/data \] &&' "$rel" && fail "release.yml data check ends in '[ -d … ] && find', which fails a clean image under bash -e"
# A search that cannot run must fail the step: the shell runs with -e.
grep -qE -- "--entrypoint sh[^|]*\"?[^ ]*\"? *(\\\\$)?" "$rel" && grep -qE -- "^ *-ec '" "$rel" || fail "release.yml data check does not run its search with sh -e"
# The published layers, not only the running filesystem: a file deleted by a later layer still ships.
save_line=$(grep -nE 'docker save' "$rel" | head -n1 | cut -d: -f1)
if [ -z "$save_line" ] || [ -z "$push_line" ] || [ "$save_line" -gt "$push_line" ]; then
    fail "release.yml does not scan the image layers (docker save) before pushing"
fi
if [ -z "$check_line" ] || [ -z "$data_line" ] || [ -z "$push_line" ]; then
    fail "release.yml has no image check for *.mmdb and /netray/data, or no push step"
elif [ "$check_line" -gt "$push_line" ] || [ "$data_line" -gt "$push_line" ]; then
    fail "release.yml checks the image for data only after pushing"
fi

# Requirement 3: the rules.
grep -qiE 'no data file[s]? (is|are) (ever )?baked' specs/rules/workflow-rules.md \
    || fail "workflow-rules.md does not forbid baking data into a published image"
grep -qE '`ci\.yml`[^.]*only[^.]*ifconfig-rs-data|only `ci\.yml`[^.]*ifconfig-rs-data' specs/rules/workflow-rules.md \
    || fail "workflow-rules.md does not name ci.yml as the only reader of ifconfig-rs-data"

# Requirement 4: docs.
grep -nE 'just image.*(login|GHCR)' README.md CLAUDE.md && fail "README/CLAUDE.md still say just image needs a GHCR login"

[ "$fails" -eq 0 ] || exit 1
echo "PASS: the published image carries no data"
