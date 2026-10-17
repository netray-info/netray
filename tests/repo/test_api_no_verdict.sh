#!/usr/bin/env bash
# specs/features/v2-api, requirement 15: the API computes no verdict. No file under
# crates/web/src/api/ names a scoring, rescoring, diffing or fingerprinting symbol; only that
# directory is scanned. The scan is a function over a root dir, self-tested on the fixtures under
# tests/repo/fixtures/api_no_verdict/ (planted flagged, clean passed, a scoring call outside
# src/api/ not flagged), then applied to the repository. The module must exist (requirement 1).
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

# verdict_offenders <root>: "file:line" for each line under <root>/crates/web/src/api/ naming a
# verdict symbol. Nothing for a missing directory.
verdict_offenders() {
    grep -rnE 'netray_engine::score|scoring::|rescore|history::diff|netray_model::diff::diff|fingerprint|lens::score|score_run|diff::diff|netray_engine::\{[^}]*\bscore\b' \
        --include='*.rs' "$1/crates/web/src/api" 2>/dev/null | cut -d: -f1,2
}

fix=tests/repo/fixtures/api_no_verdict
for c in planted planted2 planted3 planted4 clean outside; do
    [ -d "$fix/$c" ] || fail "$fix/$c is missing"
done
[ -n "$(verdict_offenders "$fix/planted")" ] || fail "self-test: the planted fixture was not flagged"
for c in planted2 planted3 planted4; do
    [ -n "$(verdict_offenders "$fix/$c")" ] || fail "self-test: the $c fixture was not flagged"
done
[ -z "$(verdict_offenders "$fix/clean")" ] || fail "self-test: the clean fixture was flagged"
[ -z "$(verdict_offenders "$fix/outside")" ] || fail "self-test: a scoring call outside src/api/ was flagged"

if [ -d crates/web/src/api ]; then
    while IFS= read -r hit; do
        fail "$hit names a verdict symbol under crates/web/src/api"
    done < <(verdict_offenders .)
else
    fail "crates/web/src/api is missing"
fi

[ "$fails" -eq 0 ] || exit 1
echo "PASS: test_api_no_verdict"
