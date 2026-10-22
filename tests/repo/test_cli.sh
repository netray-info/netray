#!/usr/bin/env bash
# specs/features/v2-cutover requirement 4: `netray --help` lists `serve` only; the V1
# subcommands exit non-zero as unknown subcommands.
set -u
source "$(dirname "$0")/lib/netray.sh"

fail() { echo "FAIL: $*" >&2; exit 1; }

bin=$(netray_bin) || exit 1

help=$("$bin" --help 2>&1) || fail "netray --help exited non-zero"
grep -Eqw -- serve <<<"$help" || fail "netray --help does not list serve"
for sub in lens dns tls http email ip site; do
    grep -Eqw -- "$sub" <<<"$(sed -n '/^Commands:/,/^$/p' <<<"$help")" \
        && fail "netray --help still lists subcommand '$sub'"
    "$bin" "$sub" --help >/dev/null 2>&1 && fail "netray $sub exited 0"
done

"$bin" no-such-subcommand >/dev/null 2>&1 && fail "unknown subcommand exited 0"

serve_help=$("$bin" serve --help 2>&1) || fail "netray serve --help exited non-zero"
grep -q -- --check-config <<<"$serve_help" || fail "netray serve --help does not mention --check-config"

echo "PASS: test_cli"
