#!/usr/bin/env bash
# specs/features/v2-history C19: every script or recipe row that starts lens with its own
# snapshots db also points the history store into its temp dir (LENS_HISTORY__DB_PATH), or the
# run leaves history.db in the cwd.
# Run from the repo root. Prints one line per failure; exit 1 if any failed.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=()
fail() { fails+=("$1"); }

self=tests/repo/test_history_db_path.sh
while IFS= read -r f; do
    [ "$f" = "$self" ] && continue
    case "$f" in tests/repo/*.sh|justfile) ;; *) continue ;; esac
    while IFS=: read -r n line; do
        grep -q 'LENS_HISTORY__DB_PATH' <<<"$line" \
            || fail "$f:$n sets the lens snapshots db but not LENS_HISTORY__DB_PATH"
    done < <(grep -nE 'LENS_SNAPSHOTS__DB_PATH|\^db_path = ' "$f")
done < <(git ls-files tests/repo justfile)

if [ "${#fails[@]}" -gt 0 ]; then
    printf 'FAIL: %s\n' "${fails[@]}" >&2
    exit 1
fi
echo "PASS: test_history_db_path"
