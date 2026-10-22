#!/usr/bin/env bash
# v2-ci-speed R4: a second build with no change finds netray-ip fresh. A build script with a
# rerun-if-changed path that does not exist reruns on every call, rebuilding netray-ip and every
# crate above it (each netray_bin in test-repo cost ~27 s instead of ~1 s). netray-ip's own build
# script went with its V1 service (v2-cutover Phase 7).
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 1

fail=0

# A second build with no change finds netray-ip fresh
cargo build -q -p netray --bin netray >/dev/null 2>&1 || { echo "FAIL: cargo build -p netray"; exit 1; }
msg=$(cargo build -p netray --bin netray --message-format=json 2>/dev/null | grep '"reason":"compiler-artifact"' | grep '"name":"netray_ip"' | grep '"kind":\["lib"\]')
grep -q '"fresh":true' <<<"$msg" || { echo "FAIL: netray-ip rebuilt on a second cargo build with no change"; fail=1; }

[ $fail -eq 0 ] && echo ok
exit $fail
