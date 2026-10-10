#!/usr/bin/env bash
# Workspace dependency alignment: the root Cargo.lock resolves exactly one
# version of reqwest and at most one version of axum-extra.
set -uo pipefail

cd "$(dirname "$0")/../.." || exit 1

lock=Cargo.lock
if [ ! -f "$lock" ]; then
    echo "FAIL: no root Cargo.lock (no workspace)"
    exit 1
fi

count() {
    grep -c "^name = \"$1\"\$" "$lock"
}

rc=0
reqwest=$(count reqwest)
axum_extra=$(count axum-extra)

if [ "$reqwest" -ne 1 ]; then
    echo "FAIL: expected exactly 1 reqwest version in $lock, found $reqwest"
    rc=1
fi
if [ "$axum_extra" -gt 1 ]; then
    echo "FAIL: expected at most 1 axum-extra version in $lock, found $axum_extra"
    rc=1
fi
exit $rc
