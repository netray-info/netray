#!/usr/bin/env bash
# v2-ci-speed R5: ok_handlers builds the ifconfig-rs app (GeoLite2 load) once per test binary,
# not once per test (113 builds took 127 s of CI time).
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 1

f=crates/ip/tests/ok_handlers.rs
[ -f "$f" ] || f=$(git ls-files 'crates/ip/tests/**ok_handlers.rs' | head -1)
[ -n "$f" ] && [ -f "$f" ] || { echo "FAIL: ok_handlers.rs not found"; exit 1; }

fail=0
# One call builds the shared dev-config app; a test that needs its own config (it assigns a config
# field in the lines before) may build its own. Any other call rebuilds what the shared app holds.
n=$(grep -c 'build_app(' "$f")
[ "$n" -ge 1 ] || { echo "FAIL: $f never calls build_app"; fail=1; }
stray=$(awk '/build_app\(/{ if (prev !~ /config\.[a-z_.]+ = / && !in_cell) print FILENAME":"NR": "$0 } /OnceCell/{in_cell=1} /^}/{in_cell=0} {prev2=prev; prev=$0}' "$f")
[ -z "$stray" ] || { echo "FAIL: build_app called with the unchanged dev config outside the shared OnceCell:"; echo "$stray"; fail=1; }
grep -q 'OnceCell' "$f" || { echo "FAIL: $f holds no OnceCell for the shared app"; fail=1; }
awk '/OnceCell/{s=1} s&&/build_app\(/{found=1} END{exit !found}' "$f" || { echo "FAIL: build_app is not called after the OnceCell"; fail=1; }

[ $fail -eq 0 ] && echo ok
exit $fail
