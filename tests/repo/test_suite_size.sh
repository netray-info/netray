#!/usr/bin/env bash
# C13: the imported suite keeps its size (no tests lost) and runs as one cargo workspace.
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 1

MIN_RUST=1650
MIN_FRONTEND=171

rust=0
while IFS= read -r f; do
    n=$(grep -cE '#\[(tokio::)?test(\]|\()' "$f")
    rust=$((rust + n))
done < <(git ls-files 'crates/*.rs')

fe=0
while IFS= read -r f; do
    n=$(grep -oE '\b(it|test)\(' "$f" | wc -l)
    fe=$((fe + n))
done < <(git ls-files 'crates/*/frontend/*' 'packages/common-frontend/*' \
    | grep -E '\.(ts|tsx|js)$' | grep -vE '(^|/)(node_modules|dist)/')

echo "rust tests: $rust (min $MIN_RUST); frontend tests: $fe (min $MIN_FRONTEND)"

fail=0
if [ "$rust" -lt "$MIN_RUST" ]; then
    echo "FAIL: $rust Rust test functions, expected at least $MIN_RUST"; fail=1
fi
if [ "$fe" -lt "$MIN_FRONTEND" ]; then
    echo "FAIL: $fe frontend test cases, expected at least $MIN_FRONTEND"; fail=1
fi
if ! grep -qE '^\[workspace\]' Cargo.toml 2>/dev/null; then
    echo "FAIL: root Cargo.toml missing or declares no [workspace]; cargo test --workspace cannot run from the root"
    fail=1
fi
exit $fail
