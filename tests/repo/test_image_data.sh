#!/usr/bin/env bash
# The root Dockerfile bakes the ifconfig-rs data image in: a named data stage,
# a COPY of its /data to /netray/data, and a final stage working in /netray.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fail() { echo "FAIL: $1"; exit 1; }

[ -f Dockerfile ] || fail "Dockerfile missing"

name=$(sed -nE 's#^FROM[[:space:]]+ghcr\.io/netray-info/ifconfig-rs-data:latest[[:space:]]+[Aa][Ss][[:space:]]+([A-Za-z0-9_.-]+)[[:space:]]*$#\1#p' Dockerfile | head -n1)
[ -n "$name" ] || fail "no 'FROM ghcr.io/netray-info/ifconfig-rs-data:latest AS <name>' stage"

grep -qE "^COPY[[:space:]]+--from=${name}[[:space:]]+/data/?[[:space:]]+/netray/data/?[[:space:]]*$" Dockerfile \
  || fail "no 'COPY --from=${name} /data /netray/data'"

last_workdir=$(grep -E '^WORKDIR[[:space:]]' Dockerfile | tail -n1 | sed -E 's/^WORKDIR[[:space:]]+//; s/[[:space:]]+$//')
[ "$last_workdir" = "/netray" ] || fail "last WORKDIR is '${last_workdir}', expected /netray"

# The last WORKDIR must sit in the final stage (after the last FROM).
last_from=$(grep -nE '^FROM[[:space:]]' Dockerfile | tail -n1 | cut -d: -f1)
last_wd_line=$(grep -nE '^WORKDIR[[:space:]]' Dockerfile | tail -n1 | cut -d: -f1)
[ "$last_wd_line" -gt "$last_from" ] || fail "final stage has no WORKDIR"

echo "PASS: data image stage wired into the Dockerfile"
