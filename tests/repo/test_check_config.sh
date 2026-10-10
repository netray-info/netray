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

# The lens production fixture names the production data under /netray/data, which a checkout
# does not have, and `[modules.ip]` refuses a missing city or ASN database. Checks that need a
# lens config that loads anywhere run on this copy, without the data paths.
lens_fixture="$REPO_ROOT/crates/lens/tests/fixtures/lens.production.toml"
lens_nodata="$tmp/lens.production.nodata.toml"
perl -ne 'if (/^\[/) { $skip = /^\[modules\.ip\]\s*$/ } print unless $skip' "$lens_fixture" >"$lens_nodata"
grep -q '^\[modules\.ip\]' "$lens_nodata" && fail "lens: [modules.ip] was not stripped from the data-free copy"
grep -q '^\[modules\.ip\]' "$lens_fixture" || fail "lens: production fixture has no [modules.ip]"
grep -q '^geoip_city_db = "/netray/data/' "$lens_fixture" || fail "lens: [modules.ip] does not carry the production data paths"
sed -n '/^\[backends\.ip\]/,/^\[/p' "$lens_fixture" | grep -q '^url' && fail "lens: [backends.ip] still carries url"
sed -n '/^\[backends\.tls\]/,/^\[/p' "$lens_fixture" | grep -q '^url' && fail "lens: [backends.tls] still carries url"
grep -q '^\[modules\.tls\.limits\]' "$lens_fixture" || fail "lens: production fixture has no [modules.tls.limits]"
sed -n '/^\[backends\.dns\]/,/^\[/p' "$lens_fixture" | grep -q '^url' && fail "lens: [backends.dns] still carries url"
grep -q '^dns_servers' "$lens_fixture" && fail "lens: production fixture still carries [backends] dns_servers"
grep -q '^\[modules\.dns\]' "$lens_fixture" || fail "lens: production fixture has no [modules.dns]"
sed -n '/^\[modules\.dns\.dns\]/,/^\[/p' "$lens_fixture" | grep -q '^allow_system_resolvers = false' || fail "lens: [modules.dns.dns] does not carry allow_system_resolvers = false"

# sub:fixture:bad-value sed expression ("" = validate() has no rejecting rule to exercise)
table=(
    "lens:crates/lens/tests/fixtures/lens.production.toml:s/^per_ip_per_minute = .*/per_ip_per_minute = 0/"
    "dns:crates/dns/tests/fixtures/prism.production.toml:s/^per_ip_per_minute = .*/per_ip_per_minute = 0/"
    "tls:crates/tls/tests/fixtures/tlsight.production.toml:s/^per_ip_per_minute = .*/per_ip_per_minute = 0/"
    "http:crates/http/tests/fixtures/spectra.production.toml:"
    "email:crates/email/tests/fixtures/beacon.production.toml:"
    "ip:crates/ip/tests/fixtures/ifconfig.production.toml:s/^per_ip_per_minute = .*/per_ip_per_minute = 0/"
)

for row in "${table[@]}"; do
    IFS=: read -r sub fixture badsed <<<"$row"
    fixture="$REPO_ROOT/$fixture"
    [ -f "$fixture" ] || { fail "$sub: fixture missing: $fixture"; continue; }

    ok_fixture="$fixture"
    [ "$sub" = lens ] && ok_fixture="$lens_nodata"
    run_check "$sub" "$ok_fixture"
    [ "$rc" -eq 0 ] || fail "$sub: valid fixture exited $rc, expected 0 ($out)"
    grep -qF "config ok: $ok_fixture" <<<"$out" || fail "$sub: stdout lacks 'config ok: $ok_fixture' ($out)"

    { echo 'bogus_key = 1'; cat "$ok_fixture"; } >"$tmp/$sub.unknown.toml"
    run_check "$sub" "$tmp/$sub.unknown.toml"
    [ "$rc" -eq 1 ] || fail "$sub: unknown key exited $rc, expected 1"

    run_check "$sub" "$tmp/$sub.does-not-exist.toml"
    [ "$rc" -eq 1 ] || fail "$sub: missing file exited $rc, expected 1"

    if [ -n "$badsed" ]; then
        sed "$badsed" "$ok_fixture" >"$tmp/$sub.invalid.toml"
        run_check "$sub" "$tmp/$sub.invalid.toml"
        [ "$rc" -eq 1 ] || fail "$sub: validate()-rejected value exited $rc, expected 1"
    fi
done

# Values the service refuses at startup must fail the check too, or the operator check
# passes a config that crash-loops (sub:fixture:perl substitution).
startup_rejects=(
    "http:crates/http/tests/fixtures/spectra.production.toml:s/^per_ip_per_minute = .*/per_ip_per_minute = 0/"
    "http:crates/http/tests/fixtures/spectra.production.toml:s/^per_ip_burst = .*/per_ip_burst = 0/"
    "email:crates/email/tests/fixtures/beacon.production.toml:s|^per_ip = .*|per_ip = \"0/min\"|"
    "email:crates/email/tests/fixtures/beacon.production.toml:s|^per_ip = .*|per_ip = \"ten\"|"
    "tls:crates/tls/tests/fixtures/tlsight.production.toml:s|^\\[validation\\]\$|[validation]\\ncustom_ca_dir = \"/nonexistent-ca-dir\"|"
    "ip:crates/ip/tests/fixtures/ifconfig.production.toml:s|^bind = .*|bind = \"nope\"|"
    "ip:crates/ip/tests/fixtures/ifconfig.production.toml:s|^admin_bind = .*|admin_bind = \"nope\"|"
    "ip:crates/ip/tests/fixtures/ifconfig.production.toml:s/^per_target_per_minute = .*/per_target_per_minute = 0/"
    "ip:crates/ip/tests/fixtures/ifconfig.production.toml:s/^per_target_burst = .*/per_target_burst = 0/"
    "ip:crates/ip/tests/fixtures/ifconfig.production.toml:s/^max_size = .*/max_size = 0/"
    "email:crates/email/tests/fixtures/beacon.production.toml:s|^bind = .*|bind = \"nope\"|"
    "email:crates/email/tests/fixtures/beacon.production.toml:s|^metrics_bind = .*|metrics_bind = \"nope\"|"
    "lens:crates/lens/tests/fixtures/lens.production.toml:s|^\\[site\\]\$|[badges]\\nttl_seconds = 0\\n\\n[site]|"
)
n=0
for row in "${startup_rejects[@]}"; do
    IFS=: read -r sub fixture expr <<<"$row"
    fixture="$REPO_ROOT/$fixture"
    n=$((n + 1))
    perl -pe "$expr" "$fixture" >"$tmp/$sub.reject$n.toml"
    if cmp -s "$fixture" "$tmp/$sub.reject$n.toml"; then
        fail "$sub: substitution '$expr' did not change the fixture"
        continue
    fi
    run_check "$sub" "$tmp/$sub.reject$n.toml"
    [ "$rc" -eq 1 ] || fail "$sub: startup-rejected value ($expr) exited $rc, expected 1"
done

# Telemetry init panics on an OTLP endpoint that is not a valid URI; the check must name
# the key, so a row cannot pass by tripping over an unknown field instead.
bad_otlp='otlp_endpoint = \"http://bad host:4318\"'
telemetry_rejects=(
    "lens:crates/lens/tests/fixtures/lens.production.toml:s|^\\[telemetry\\]\$|[telemetry]\\nenabled = true\\n$bad_otlp|"
    "dns:crates/dns/tests/fixtures/prism.production.toml:s|^\\[telemetry\\]\$|[telemetry]\\nenabled = true\\n$bad_otlp|"
    "tls:crates/tls/tests/fixtures/tlsight.production.toml:s|^\\[telemetry\\]\$|[telemetry]\\nenabled = true\\n$bad_otlp|"
    "http:crates/http/tests/fixtures/spectra.production.toml:s|^\\[telemetry\\]\$|[telemetry]\\nenabled = true\\n$bad_otlp|"
    "email:crates/email/tests/fixtures/beacon.production.toml:s|^\\[telemetry\\]\$|[telemetry]\\n$bad_otlp|"
    "ip:crates/ip/tests/fixtures/ifconfig.production.toml:s|^\\[telemetry\\]\$|[telemetry]\\nenabled = true\\n$bad_otlp|"
)
for row in "${telemetry_rejects[@]}"; do
    IFS=: read -r sub fixture expr <<<"$row"
    fixture="$REPO_ROOT/$fixture"
    perl -pe "$expr" "$fixture" >"$tmp/$sub.otlp.toml"
    if cmp -s "$fixture" "$tmp/$sub.otlp.toml"; then
        fail "$sub: telemetry substitution did not change the fixture"
        continue
    fi
    run_check "$sub" "$tmp/$sub.otlp.toml"
    [ "$rc" -eq 1 ] || fail "$sub: invalid otlp_endpoint exited $rc, expected 1"
    grep -qF 'telemetry.otlp_endpoint' <<<"$out" || fail "$sub: invalid otlp_endpoint error does not name telemetry.otlp_endpoint ($out)"
done

# C7: backend timeouts that exceed the deadline budget (dns/tls/http/email 20000, ip 2000,
# forced regardless of the fixture) must fail the check and name the budget.
perl -pe '
    $s = $1 if /^\[backends\.(\w+)\]/;
    $s = "" if /^\[(?!backends\.)/;
    s/^timeout_ms = .*/"timeout_ms = " . ($s eq "ip" ? 2000 : $s ne "" ? 20000 : 0)/e if $s ne "";
' "$lens_fixture" >"$tmp/lens.budget.toml"
if [ "$(grep -c '^timeout_ms = 20000$' "$tmp/lens.budget.toml")" -ne 4 ] \
    || ! grep -qx 'timeout_ms = 2000' "$tmp/lens.budget.toml"; then
    fail "lens: budget substitution did not force 20000/2000"
else
    run_check lens "$tmp/lens.budget.toml"
    [ "$rc" -eq 1 ] || fail "lens: timeouts over the deadline budget exited $rc, expected 1"
    grep -qi 'timeout' <<<"$out" && grep -qi 'deadline' <<<"$out" \
        || fail "lens: over-budget error does not mention timeout and deadline ($out)"
fi

# V2 Phase 6: the resolve stage has its own budget inside the IP section's window (both count
# from the run start), so resolve_timeout_ms must stay below [backends.ip] timeout_ms; the error
# names resolve_timeout_ms.
perl -pe 's/^resolve_timeout_ms = .*/resolve_timeout_ms = 2000/' "$lens_nodata" >"$tmp/lens.resolve.toml"
grep -qx 'resolve_timeout_ms = 2000' "$tmp/lens.resolve.toml" \
    || fail "lens: resolve_timeout_ms substitution did not change the data-free copy"
run_check lens "$tmp/lens.resolve.toml"
[ "$rc" -eq 1 ] || fail "lens: resolve_timeout_ms 2000 >= ip 2000 exited $rc, expected 1 ($out)"
grep -qF 'resolve_timeout_ms' <<<"$out" || fail "lens: over-budget resolve error does not name resolve_timeout_ms ($out)"

# C8: the shipped lens configs stay loadable.
# The production fixture is checked without its data paths (`lens_nodata`).
for f in "$lens_nodata" "$REPO_ROOT/crates/lens/lens.dev.toml" "$REPO_ROOT/crates/lens/lens.example.toml"; do
    run_check lens "$f"
    [ "$rc" -eq 0 ] || fail "lens: $f exited $rc, expected 0 ($out)"
done

# V2 Phases 1 to 5: the HTTP, email, IP, TLS and DNS sections are modules. A leftover `[backends.<x>] url`
# and an unknown `[modules.<x>]` key both fail the check and the output names the offending key.
module_rejects=(
    "backends.http url:url:s|^\\[backends\\.http\\]\$|[backends.http]\\nurl = 'http://spectra:8082'|"
    "modules.http bogus:bogus:s|^\\[ecosystem\\]\$|[modules.http]\\nbogus = 1\\n\\n[ecosystem]|"
    "backends.email url:url:s|^\\[backends\\.email\\]\$|[backends.email]\\nurl = 'http://beacon:8084'|"
    "modules.email bogus:bogus:s|^\\[ecosystem\\]\$|[modules.email]\\nbogus = 1\\n\\n[ecosystem]|"
    "backends.ip url:url:s|^\\[backends\\.ip\\]\$|[backends.ip]\\nurl = 'http://ifconfig-rs:8000'|"
    "modules.ip bogus:bogus:s|^\\[modules\\.ip\\]\$|[modules.ip]\\nbogus = 1|"
    "backends.tls url:url:s|^\\[backends\\.tls\\]\$|[backends.tls]\\nurl = 'http://tlsight:8081'|"
    "modules.tls bogus:bogus:s|^\\[modules\\.tls\\.limits\\]\$|[modules.tls]\\nbogus = 1\\n\\n[modules.tls.limits]|"
    "backends.dns url:url:s|^\\[backends\\.dns\\]\$|[backends.dns]\\nurl = 'http://prism:8080'|"
    "backends dns_servers:dns_servers:s|^resolve_timeout_ms = .*\$|resolve_timeout_ms = 1500\\ndns_servers = ['google']|"
    "modules.dns bogus:bogus:s|^servers = .*\$|servers = ['google']\\nbogus = 1|"
)
n=0
for row in "${module_rejects[@]}"; do
    IFS=: read -r label key expr <<<"$row"
    n=$((n + 1))
    perl -pe "$expr" "$lens_fixture" >"$tmp/lens.module$n.toml"
    if cmp -s "$lens_fixture" "$tmp/lens.module$n.toml"; then
        fail "lens: substitution for '$label' did not change the fixture"
        continue
    fi
    run_check lens "$tmp/lens.module$n.toml"
    [ "$rc" -eq 1 ] || fail "lens: $label exited $rc, expected 1 ($out)"
    grep -qF "$key" <<<"$out" || fail "lens: $label error does not name '$key' ($out)"
done

# V2 Phase 3, C5: `[modules.ip]` loads its data at `--check-config`. A configured IP section
# must name `geoip_city_db` or `geoip_asn_db`: a missing database file, a list-only section and
# an empty table all refuse and the output names `geoip_city_db`. All rows run on the data-free
# copy with a `[modules.ip]` appended. ("A missing optional list only warns" is the unit test
# crates/ip/tests/module_data.rs: a config naming both GeoIP databases cannot load offline.)
ip_data_rows=(
    "missing city db|geoip_city_db = \"/nonexistent.mmdb\"\ngeoip_asn_db = \"/nonexistent-asn.mmdb\""
    "list only|feodo_botnet_ips = \"/nonexistent.txt\""
    "empty table|"
)
n=0
for row in "${ip_data_rows[@]}"; do
    IFS='|' read -r label body <<<"$row"
    n=$((n + 1))
    ipdata="$tmp/lens.ipdata$n.toml"
    { cat "$lens_nodata"; printf '\n[modules.ip]\n'; [ -n "$body" ] && printf '%b\n' "$body"; } >"$ipdata"
    run_check lens "$ipdata"
    [ "$rc" -eq 1 ] || fail "lens: [modules.ip] $label exited $rc, expected 1 ($out)"
    grep -qF 'geoip_city_db' <<<"$out" || fail "lens: [modules.ip] $label error does not name geoip_city_db ($out)"
done

# V2 Phase 3: an unconfigured section is off, and --check-config says so (exit stays 0).
run_check lens "$lens_nodata"
[ "$rc" -eq 0 ] || fail "lens: config without [modules.ip] exited $rc, expected 0 ($out)"
grep -qF 'modules.ip is not configured: the IP section is off' <<<"$out" \
    || fail "lens: config without [modules.ip] does not warn that the IP section is off ($out)"

# Without `[modules.http.enrichment] ip_url` the check warns (exit stays 0).
perl -ne 'if (/^\[/) { $skip = /^\[modules\.http\.enrichment\]\s*$/ } print unless $skip' "$lens_nodata" >"$tmp/lens.noipurl.toml"
cmp -s "$lens_nodata" "$tmp/lens.noipurl.toml" && fail "lens: ip_url removal did not change the data-free copy"
run_check lens "$tmp/lens.noipurl.toml"
[ "$rc" -eq 0 ] || fail "lens: config without enrichment ip_url exited $rc, expected 0 ($out)"
grep -qF 'modules.http.enrichment.ip_url is not set' <<<"$out" \
    || fail "lens: config without enrichment ip_url does not warn ($out)"

[ "$failures" -eq 0 ] || { echo "FAIL: test_check_config: $failures failure(s)" >&2; exit 1; }
echo "PASS: test_check_config"
