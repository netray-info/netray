#!/usr/bin/env bash
# v2-cutover requirement 12 (and requirements 1, 9): `netray serve --check-config <path>` validates
# a config file and exits without starting a listener; values `serve` refuses at startup fail the
# check too (startup_rejects), except files the config names (the store, the GeoIP data), which
# only startup opens. Fixture: crates/netray/tests/fixtures/netray.production.toml.
set -u
source "$(dirname "$0")/lib/netray.sh"

bin=$(netray_bin) || exit 1
tmp=$(mktemp -d "${TMPDIR:-/tmp}/check_config.XXXXXX")
trap 'rm -rf "$tmp"; _netray_cleanup' EXIT

failures=0
fail() { echo "FAIL: $*" >&2; failures=$((failures + 1)); }

# run_check <path>: sets rc and out; kills the run after 20 s (rc 137).
run_check() {
    local log="$tmp/run.log" pid watchdog
    "$bin" serve --check-config "$1" >"$log" 2>&1 </dev/null &
    pid=$!
    ( sleep 20; kill -9 "$pid" 2>/dev/null ) &
    watchdog=$!
    wait "$pid"
    rc=$?
    kill "$watchdog" 2>/dev/null
    wait "$watchdog" 2>/dev/null
    [ "$rc" -eq 137 ] && echo "FAIL: serve --check-config did not exit within 20 s" >&2
    out=$(cat "$log")
}

# start_out <path>: runs `serve` for at most 20 s; sets startup_rc and startup_out.
start_out() {
    startup_out=$(env NETRAY_SERVER__BIND="127.0.0.1:$(free_port)" NETRAY_SERVER__METRICS_BIND="127.0.0.1:$(free_port)" \
        perl -e 'alarm 20; exec @ARGV' "$bin" serve "$1" 2>&1 </dev/null)
    startup_rc=$?
}

# The production fixture names the production data under /netray/data, which a checkout does not
# have. Checks that need a config that loads anywhere run on this copy, without [modules.ip].
fixture="$REPO_ROOT/crates/netray/tests/fixtures/netray.production.toml"
nodata="$tmp/netray.production.nodata.toml"
perl -ne 'if (/^\[/) { $skip = /^\[modules\.ip\]\s*$/ } print unless $skip' "$fixture" >"$nodata"
grep -q '^\[modules\.ip\]' "$fixture" || fail "production fixture has no [modules.ip]"
grep -q '^geoip_city_db = "/netray/data/' "$fixture" || fail "[modules.ip] does not carry the production data paths"
grep -q '^\[modules\.ip\]' "$nodata" && fail "[modules.ip] was not stripped from the data-free copy"

# The shipped configs and the data-free fixture load.
for f in "$nodata" "$REPO_ROOT/crates/netray/netray.example.toml" "$REPO_ROOT/crates/netray/netray.dev.toml"; do
    run_check "$f"
    [ "$rc" -eq 0 ] || fail "$f exited $rc, expected 0 ($out)"
    grep -qF "config ok: $f" <<<"$out" || fail "stdout lacks 'config ok: $f' ($out)"
done
{ echo 'bogus_key = 1'; cat "$nodata"; } >"$tmp/unknown.toml"
run_check "$tmp/unknown.toml"
[ "$rc" -eq 1 ] || fail "unknown key exited $rc, expected 1"
grep -qF 'config error:' <<<"$out" || fail "unknown key output lacks 'config error:' ($out)"
run_check "$tmp/does-not-exist.toml"
[ "$rc" -eq 1 ] || fail "missing file exited $rc, expected 1"

# startup_rejects: section|body|must-name. Each body is inserted after the section's header of the
# data-free fixture, or appended as a new table when the fixture lacks the section.
startup_rejects=(
    "admission|domain_run_slots = 0|"
    "admission|per_ip_per_minute = 0|"
    "admission|per_ip_burst = 0|"
    "admission|global_per_minute = 0|"
    "admission|global_burst = 0|"
    "badges|ttl_seconds = 0|"
    "api|per_ip_per_minute = 0|api.per_ip_per_minute"
    "api|per_ip_burst = 0|api.per_ip_burst"
    "api.ip|per_ip_per_minute = 0|api.ip.per_ip_per_minute"
    "api.ip|per_ip_per_minute = 601|api.ip.per_ip_per_minute"
    "api.ip|per_ip_burst = 1|api.ip.per_ip_burst"
    "api.ip|per_ip_burst = 101|api.ip.per_ip_burst"
    "api.ip|per_target_per_minute = 0|api.ip.per_target_per_minute"
    "api.ip|per_target_burst = 101|api.ip.per_target_burst"
    "api.ip|batch_max_size = 0|api.ip.batch_max_size"
    "api.ip|batch_max_size = 101|api.ip.batch_max_size"
    "api.ip|batch_addresses_per_minute = 601|api.ip.batch_addresses_per_minute"
    "api.ip|batch_addresses_burst = 50|api.ip.batch_addresses_burst"
    "api.tls|per_ip_per_minute = 0|api.tls.per_ip_per_minute"
    "api.tls|per_target_burst = 0|api.tls.per_target_burst"
    "api.http|per_ip_burst = 0|api.http.per_ip_burst"
    "api.http|per_target_per_minute = 0|api.http.per_target_per_minute"
    "api.email|per_ip_per_minute = 0|api.email.per_ip_per_minute"
    "api.email|max_concurrent_inspections = 0|api.email.max_concurrent_inspections"
    "api.dns|per_ip_burst = 17|api.dns.per_ip_burst"
    "api.dns|per_ip_per_minute = 0|api.dns.per_ip_per_minute"
    "api.dns|per_target_per_minute = 0|api.dns.per_target_per_minute"
    "api.dns|per_target_burst = 0|api.dns.per_target_burst"
    "api.dns|global_per_minute = 0|api.dns.global_per_minute"
    "api.tls|per_ip_burst = 0|api.tls.per_ip_burst"
    "api.tls|per_target_per_minute = 0|api.tls.per_target_per_minute"
    "api.http|per_ip_per_minute = 0|api.http.per_ip_per_minute"
    "api.http|per_target_burst = 0|api.http.per_target_burst"
    "api.email|per_ip_burst = 0|api.email.per_ip_burst"
    "api.ip|per_ip_burst = 0|api.ip.per_ip_burst"
    "api.ip|per_target_per_minute = 601|api.ip.per_target_per_minute"
    "api.ip|per_target_burst = 0|api.ip.per_target_burst"
    "api.ip|batch_addresses_per_minute = 0|api.ip.batch_addresses_per_minute"
    "api.ip|batch_addresses_burst = 101|api.ip.batch_addresses_burst"
    "api.dns|global_burst = 17|api.dns.global_burst"
    "api.dns|per_ip_max_streams = 0|api.dns.per_ip_max_streams"
    "inspect.dns|max_concurrent = 0|inspect.dns.max_concurrent"
    "inspect.tls|deadline_ms = 0|inspect.tls.deadline_ms"
    "telemetry|enabled = true\\notlp_endpoint = \"http://bad host:4318\"|telemetry.otlp_endpoint"
)
n=0
for row in "${startup_rejects[@]}"; do
    IFS='|' read -r section body names <<<"$row"
    n=$((n + 1))
    f="$tmp/reject$n.toml"
    if grep -q "^\\[$section\\]\$" "$nodata"; then
        perl -pe "s|^\\[$section\\]\$|[$section]\\n$body|" "$nodata" >"$f"
    else
        { cat "$nodata"; printf '\n[%s]\n%b\n' "$section" "$body"; } >"$f"
    fi
    cmp -s "$nodata" "$f" && { fail "[$section] $body did not change the fixture"; continue; }
    run_check "$f"
    [ "$rc" -eq 1 ] || fail "startup-rejected value ([$section] $body) exited $rc, expected 1 ($out)"
    if [ -n "$names" ]; then
        grep -qF "$names" <<<"$out" || fail "[$section] $body error does not name $names ($out)"
        ! grep -qi 'unknown field' <<<"$out" || fail "[$section] $body was refused as an unknown field ($out)"
    fi
done

# `client_budget_per_hour = 0` is valid (no per-client budget).
perl -pe 's/^client_budget_per_hour = .*/client_budget_per_hour = 0/' "$nodata" >"$tmp/budget0.toml"
run_check "$tmp/budget0.toml"
[ "$rc" -eq 0 ] || fail "client_budget_per_hour = 0 exited $rc, expected 0 ($out)"

# An unknown key under [api], [api.ip], [inspect.tls] or a module table fails and names the key.
for body in "[api]\nbogus = 1" "[api.ip]\nbogus = 1" "[inspect.tls]\nbogus = 1" "[modules.http]\nbogus = 1"; do
    { cat "$nodata"; printf '\n%b\n' "$body"; } >"$tmp/bogus.toml"
    run_check "$tmp/bogus.toml"
    [ "$rc" -eq 1 ] || fail "$body exited $rc, expected 1 ($out)"
    grep -qF 'bogus' <<<"$out" || fail "$body error does not name 'bogus' ($out)"
done

# A configured IP section names both GeoIP databases: a list-only section fails. (An empty
# `[modules.ip]` table is no configured section under `serve`: the IP section stays off.)
for body in 'feodo_botnet_ips = "/nonexistent.txt"'; do
    { cat "$nodata"; printf '\n[modules.ip]\n%s\n' "$body"; } >"$tmp/ipdata.toml"
    run_check "$tmp/ipdata.toml"
    [ "$rc" -eq 1 ] || fail "[modules.ip] '$body' exited $rc, expected 1 ($out)"
    grep -qF 'geoip_city_db' <<<"$out" || fail "[modules.ip] '$body' error does not name geoip_city_db ($out)"
done

# GeoIP files at missing paths pass the check (no data is read) and refuse startup, naming the key.
ipfiles="$tmp/ipfiles.toml"
{ cat "$nodata"; printf '\n[store]\nenabled = false\n\n[modules.ip]\ngeoip_city_db = "/nonexistent.mmdb"\ngeoip_asn_db = "/nonexistent-asn.mmdb"\n'; } >"$ipfiles"
run_check "$ipfiles"
[ "$rc" -eq 0 ] || fail "--check-config with missing GeoIP files exited $rc, expected 0 ($out)"
start_out "$ipfiles"
[ "$startup_rc" -ne 0 ] && [ "$startup_rc" -ne 142 ] || fail "startup with missing GeoIP files did not refuse (rc $startup_rc)"
grep -qF 'geoip_city_db' <<<"$startup_out" || fail "startup refusal does not name geoip_city_db ($startup_out)"

# An unopenable store passes the check and refuses startup, naming the history store.
storebad="$tmp/storebad.toml"
{ cat "$nodata"; printf '\n[store]\ndb_path = "/nonexistent-dir/history.db"\n'; } >"$storebad"
run_check "$storebad"
[ "$rc" -eq 0 ] || fail "--check-config with an unopenable store exited $rc, expected 0 ($out)"
start_out "$storebad"
[ "$startup_rc" -ne 0 ] && [ "$startup_rc" -ne 142 ] || fail "startup with an unopenable store did not refuse (rc $startup_rc)"
grep -qF 'history store' <<<"$startup_out" || fail "store refusal does not name the history store ($startup_out)"

[ "$failures" -eq 0 ] || { echo "FAIL: test_check_config: $failures failure(s)" >&2; exit 1; }
echo "PASS: test_check_config"
