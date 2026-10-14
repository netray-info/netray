#!/usr/bin/env bash
# C5: `presentation` is not an identifier in the engine, the module adapters or lens.
set -u
cd "$(dirname "$0")/../.." || exit 1

targets=(crates/engine/src crates/lens/src)
for c in dns tls http email ip; do
    targets+=("crates/$c/src/module.rs")
done
for t in "${targets[@]}"; do
    [ -e "$t" ] || { echo "FAIL: missing $t" >&2; exit 1; }
done

if matches=$(grep -rnw -- presentation "${targets[@]}"); then
    echo "FAIL: 'presentation' still appears as an identifier:" >&2
    echo "$matches" >&2
    exit 1
fi

echo "PASS: test_no_presentation"
