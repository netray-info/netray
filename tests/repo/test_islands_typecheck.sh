#!/usr/bin/env bash
# `npm test` in the islands package type-checks before vitest: a planted type
# error in src/main.ts exits non-zero, shows `error TS`, and vitest never starts.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

pkg=crates/web/islands
for f in "$pkg/package.json" "$pkg/src/main.ts"; do
  [ -f "$f" ] || { echo "FAIL: $f missing"; exit 1; }
done

mkdir -p target
copy="$PWD/target/islands-typecheck.$$"
trap 'rm -rf "$copy"' EXIT
mkdir -p "$copy"
tar -C "$pkg" --exclude=node_modules --exclude=dist -cf - . | tar -C "$copy" -xf -

printf '\nconst __planted: number = "not a number";\n' >> "$copy/src/main.ts"

out=$(cd "$copy" && npm test 2>&1)
rc=$?

fail=0
if [ "$rc" -eq 0 ]; then
  echo "FAIL: npm test exited 0 despite a planted type error"; fail=1
fi
if ! grep -q 'error TS' <<<"$out"; then
  echo "FAIL: output carries no TypeScript error (error TS)"; fail=1
fi
if grep -Eq '(^|[[:space:]])RUN |Test Files' <<<"$out"; then
  echo "FAIL: vitest started before the type check failed"; fail=1
fi
if [ "$fail" -ne 0 ]; then
  echo "$out" | tail -20
  exit 1
fi
echo "PASS: a type error stops npm test before vitest"
