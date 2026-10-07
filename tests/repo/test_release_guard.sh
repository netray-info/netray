#!/usr/bin/env bash
# B5: `just release X.Y.Z` refuses to run on a dirty tree (no commit, no tag).
# Works only in a throwaway clone; the real repository is never modified.
set -uo pipefail
cd "$(dirname "$0")/../.." || { echo "FAIL: cannot cd to repo root"; exit 1; }
root=$PWD

tmp=$(mktemp -d) || { echo "FAIL: mktemp failed"; exit 1; }
trap 'rm -rf "$tmp"' EXIT

export GIT_CONFIG_COUNT=2
export GIT_CONFIG_KEY_0=commit.gpgsign GIT_CONFIG_VALUE_0=false
export GIT_CONFIG_KEY_1=tag.gpgsign GIT_CONFIG_VALUE_1=false

git clone -q "$root" "$tmp/c" || { echo "FAIL: clone failed"; exit 1; }
# HEAD may predate the workspace: overlay the working tree so the recipe has what it needs.
rsync -a --exclude .git --exclude target --exclude node_modules --exclude dist "$root/" "$tmp/c/" \
    || { echo "FAIL: rsync of working tree failed"; exit 1; }
cd "$tmp/c" || exit 1
git add -A
git -c user.name=t -c user.email=t@example.com -c commit.gpgsign=false commit -qm test 2>/dev/null
git diff --quiet HEAD || { echo "FAIL: clone not clean before the test"; exit 1; }
[ -f Cargo.toml ] && [ -f CHANGELOG.md ] || { echo "FAIL: clone lacks Cargo.toml or CHANGELOG.md"; exit 1; }

# Dirty the tree with an unrelated tracked change.
echo "dirty" >> AGENTS.md
before=$(git rev-parse HEAD)

out=$(just release 9.9.9 2>&1)
rc=$?

after=$(git rev-parse HEAD)
[ "$rc" -ne 0 ] || { echo "FAIL: just release exited 0 on a dirty tree"; exit 1; }
[ "$before" = "$after" ] || { echo "FAIL: release moved HEAD on a dirty tree"; exit 1; }
git rev-parse -q --verify refs/tags/v9.9.9 >/dev/null && { echo "FAIL: tag v9.9.9 exists after refused release"; exit 1; }

echo "PASS: release refused on dirty tree"
