#!/usr/bin/env bash
# C6 + CLI half of C2: the single `netray` binary exposes seven subcommands.
set -u
source "$(dirname "$0")/lib/netray.sh"

fail() { echo "FAIL: $*" >&2; exit 1; }

bin=$(netray_bin) || exit 1

help=$("$bin" --help 2>&1) || fail "netray --help exited non-zero"
for sub in lens dns tls http email ip site; do
    grep -Eqw -- "$sub" <<<"$help" || fail "netray --help does not list subcommand '$sub'"
done

"$bin" no-such-subcommand >/dev/null 2>&1 && fail "unknown subcommand exited 0"

site_help=$("$bin" site --help 2>&1) || fail "netray site --help exited non-zero"
for flag in --bind --root; do
    grep -q -- "$flag" <<<"$site_help" || fail "netray site --help does not mention $flag"
done

echo "PASS: test_cli"
