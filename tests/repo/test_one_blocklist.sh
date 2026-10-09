#!/usr/bin/env bash
# specs/features/grade-integrity: one blocklist
# Outside crates/common/ no crate defines its own target blocklist: no copy of the
# shared function names, and no range checks (.is_private/.is_loopback) in security/ code.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

# scan DIR... reads file list on stdin, prints offending "file: reason" lines, and the
# number of files scanned on fd 3 via the variable SCANNED (set by caller through a temp file).
scan() {
    local count=0 f
    while IFS= read -r f; do
        [ -f "$f" ] || continue
        count=$((count + 1))
        for pat in 'fn is_allowed_target(' 'fn is_blocked_ip(' 'fn check_allowed('; do
            grep -qF "$pat" "$f" && echo "$f: defines $pat"
        done
        case "$f" in
            */security/*)
                grep -qE '\.is_private\(\)|\.is_loopback\(\)' "$f" && echo "$f: range check in security/"
                ;;
        esac
    done
    echo "SCANNED=$count"
}

# Self-test: a fixture with a duplicate definition must be reported.
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/security"
printf 'pub fn is_blocked_ip(ip: u8) -> bool { false }\n' > "$work/security/fixture.rs"
out=$(printf '%s\n' "$work/security/fixture.rs" | scan)
echo "$out" | grep -q 'defines fn is_blocked_ip(' || fail "self-test: grep did not fire on a fixture defining fn is_blocked_ip("
printf 'fn f(v: std::net::Ipv4Addr) { v.is_private(); }\n' > "$work/security/fixture2.rs"
out=$(printf '%s\n' "$work/security/fixture2.rs" | scan)
echo "$out" | grep -q 'range check' || fail "self-test: grep did not fire on is_private in security/"

out=$(git ls-files 'crates/*/src/**.rs' | grep -v '^crates/common/' | scan)
scanned=$(echo "$out" | sed -n 's/^SCANNED=//p')
echo "scanned $scanned files"
[ "${scanned:-0}" -gt 0 ] || fail "no files scanned"
hits=$(echo "$out" | grep -v '^SCANNED=')
[ -z "$hits" ] || { fail "second blocklist outside crates/common:"; echo "$hits"; }

[ "$fails" -eq 0 ] || exit 1
echo "PASS: test_one_blocklist"
