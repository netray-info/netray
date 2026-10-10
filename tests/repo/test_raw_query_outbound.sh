#!/usr/bin/env bash
# specs/features/raw-query-policy: prism opens DNS sockets and resolves glue only in
# crates/dns/src/dns_raw.rs, behind `RawOutbound` and its outbound policy. The one
# other socket is meta.rs's readiness probe to operator-configured backend URLs. A self-test
# runs the scanner on a fixture with a stray socket.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

pattern='(UdpSocket::bind|TcpStream::connect|lookup_host|send_udp\(|send_tcp\()'
allowed='^crates/dns/src/(dns_raw\.rs|api/meta\.rs|.*_tests\.rs)$'

scan() {
    [ $# -gt 0 ] || return 1
    grep -nE "$pattern" -- "$@" /dev/null
}

fixture=tests/repo/fixtures/raw-socket/stray.rs
if [ -f "$fixture" ]; then
    scan "$fixture" > /dev/null || fail "scanner does not report the socket in $fixture"
else
    fail "$fixture missing"
fi

files=()
while IFS= read -r f; do
    [[ "$f" =~ $allowed ]] && continue
    files+=("$f")
done < <(git ls-files 'crates/dns/src/*.rs')
echo "scanned ${#files[@]} files"
[ "${#files[@]}" -gt 0 ] || fail "no prism source files scanned"

if hits=$(scan "${files[@]}"); then
    fail "raw socket or resolver outside dns_raw.rs:"
    echo "$hits"
fi

[ "$fails" -eq 0 ] || exit 1
echo "PASS: test_raw_query_outbound"
