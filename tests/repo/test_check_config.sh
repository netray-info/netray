#!/usr/bin/env bash
# C3/C4/C10: `netray <sub> --check-config <path>` validates a config file and exits
# without starting a listener, against the production-shaped fixture of each service.
set -u
source "$(dirname "$0")/lib/netray.sh"

bin=$(netray_bin) || exit 1
tmp=$(mktemp -d "${TMPDIR:-/tmp}/check_config.XXXXXX")
trap 'rm -rf "$tmp"; _netray_cleanup' EXIT

failures=0
fail() { echo "FAIL: $*" >&2; failures=$((failures + 1)); }

# run_check <sub> <path>: sets rc and out; kills the run after 20 s (rc 137).
run_check() {
    local log="$tmp/run.log" pid watchdog
    "$bin" "$1" --check-config "$2" >"$log" 2>&1 </dev/null &
    pid=$!
    ( sleep 20; kill -9 "$pid" 2>/dev/null ) &
    watchdog=$!
    wait "$pid"
    rc=$?
    kill "$watchdog" 2>/dev/null
    wait "$watchdog" 2>/dev/null
    [ "$rc" -eq 137 ] && echo "FAIL: $1 --check-config did not exit within 20 s" >&2
    out=$(cat "$log")
}

# sub:fixture:bad-value sed expression ("" = validate() has no rejecting rule to exercise)
table=(
    "lens:crates/lens/tests/fixtures/lens.production.toml:s/^per_ip_per_minute = .*/per_ip_per_minute = 0/"
    "dns:crates/mhost-prism/tests/fixtures/prism.production.toml:s/^per_ip_per_minute = .*/per_ip_per_minute = 0/"
    "tls:crates/tlsight/tests/fixtures/tlsight.production.toml:s/^per_ip_per_minute = .*/per_ip_per_minute = 0/"
    "http:crates/spectra/tests/fixtures/spectra.production.toml:"
    "email:crates/beacon/tests/fixtures/beacon.production.toml:"
    "ip:crates/ifconfig-rs/tests/fixtures/ifconfig.production.toml:s/^per_ip_per_minute = .*/per_ip_per_minute = 0/"
)

for row in "${table[@]}"; do
    IFS=: read -r sub fixture badsed <<<"$row"
    fixture="$REPO_ROOT/$fixture"
    [ -f "$fixture" ] || { fail "$sub: fixture missing: $fixture"; continue; }

    run_check "$sub" "$fixture"
    [ "$rc" -eq 0 ] || fail "$sub: valid fixture exited $rc, expected 0 ($out)"
    grep -qF "config ok: $fixture" <<<"$out" || fail "$sub: stdout lacks 'config ok: $fixture' ($out)"

    { echo 'bogus_key = 1'; cat "$fixture"; } >"$tmp/$sub.unknown.toml"
    run_check "$sub" "$tmp/$sub.unknown.toml"
    [ "$rc" -eq 1 ] || fail "$sub: unknown key exited $rc, expected 1"

    run_check "$sub" "$tmp/$sub.does-not-exist.toml"
    [ "$rc" -eq 1 ] || fail "$sub: missing file exited $rc, expected 1"

    if [ -n "$badsed" ]; then
        sed "$badsed" "$fixture" >"$tmp/$sub.invalid.toml"
        run_check "$sub" "$tmp/$sub.invalid.toml"
        [ "$rc" -eq 1 ] || fail "$sub: validate()-rejected value exited $rc, expected 1"
    fi
done

[ "$failures" -eq 0 ] || { echo "FAIL: test_check_config: $failures failure(s)" >&2; exit 1; }
echo "PASS: test_check_config"
