#!/usr/bin/env bash
# v2-ci-speed R5: ok_handlers builds the ifconfig-rs app (GeoLite2 load) once per test binary,
# not once per test (113 builds took 127 s of CI time).
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 1

f=crates/ip/tests/ok_handlers.rs
[ -f "$f" ] || f=$(git ls-files 'crates/ip/tests/**ok_handlers.rs' | head -1)
[ -n "$f" ] && [ -f "$f" ] || { echo "FAIL: ok_handlers.rs not found"; exit 1; }

fail=0
n=$(grep -c 'build_app(' "$f")
[ "$n" -eq 1 ] || { echo "FAIL: $f calls build_app $n times, want 1 (inside a OnceCell initialiser)"; fail=1; }
grep -q 'OnceCell' "$f" || { echo "FAIL: $f holds no OnceCell for the shared app"; fail=1; }
awk '/OnceCell/{s=1} s&&/build_app\(/{found=1} END{exit !found}' "$f" || { echo "FAIL: build_app is not called after the OnceCell"; fail=1; }

[ $fail -eq 0 ] && echo ok
exit $fail
