#!/usr/bin/env bash
# specs/features/v2-history C19, v2-cutover requirement 7: every script or recipe line that starts
# `netray serve` points the history store into its temp dir (NETRAY_STORE__DB_PATH), or the run
# leaves a history.db in the checkout. A `--check-config` call opens no store and is exempt.
# Run from the repo root. Prints one line per failure; exit 1 if any failed.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=()
fail() { fails+=("$1"); }

self=tests/repo/test_history_db_path.sh
while IFS= read -r f; do
    [ "$f" = "$self" ] && continue
    # A start line joins its backslash continuations; it must name NETRAY_STORE__DB_PATH.
    while IFS=: read -r n line; do
        grep -q -- '--check-config' <<<"$line" && continue
        grep -q 'NETRAY_STORE__DB_PATH' <<<"$line" \
            || fail "$f:$n starts netray serve without NETRAY_STORE__DB_PATH"
    done < <(awk '{ if (buf != "") { buf = buf " " $0 } else { buf = $0; start = NR } }
                  /\\$/ { next }
                  { print start ":" buf; buf = "" }' "$f" \
             | grep -E '^[0-9]+:[[:space:]]*(start_bg|docker run) ' | grep -E ' serve( |$)|"\$image"')
done < <(git ls-files 'tests/repo/*.sh' 'tests/acceptance/*.sh' justfile .github/workflows/release.yml)

if [ "${#fails[@]}" -gt 0 ]; then
    printf 'FAIL: %s\n' "${fails[@]}" >&2
    exit 1
fi
echo "PASS: test_history_db_path"
