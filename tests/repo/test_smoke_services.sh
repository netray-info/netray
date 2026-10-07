#!/usr/bin/env bash
# Spec monorepo-p1, Phase 2, requirement 6: every service subcommand of `netray`
# answers GET / and GET /health with 200, and an unknown path with 200 and the
# SPA's index.html (the same body as GET /).
#
# Each service starts from its dev config with the HTTP and metrics binds moved to
# free ports through the service's own env override. Dev configs carry relative
# paths, so the service runs from its crate directory.
set -uo pipefail
source "$(dirname "$0")/lib/netray.sh"

bin=$(netray_bin) || exit 1
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"; _netray_cleanup' EXIT
mkdir -p "$tmp/custom_cas"

fail() { echo "FAIL: $*" >&2; exit 1; }

# sub | crate dir | config (relative to the crate dir) | env var holding the HTTP bind
# | env var holding the metrics bind | extra env (space separated KEY=VALUE)
#
# Env syntax differs per service: LENS_/PRISM_/TLSIGHT_/IFCONFIG_ use a single
# underscore after the prefix, SPECTRA__/BEACON__ a double one.
# Offline start-up needs two overrides and one fixture:
#   lens     snapshot DB path (dev config's data/ directory does not exist)
#   tls      custom_ca_dir    (dev config's custom_cas/ directory does not exist)
#   ip       fixture config   (the dev config needs the GeoIP .mmdb files, which are not in the tree)
rows=(
  "lens|lens|lens.dev.toml|LENS_SERVER__BIND|LENS_SERVER__METRICS_BIND|LENS_SNAPSHOTS__DB_PATH=$tmp/snapshots.db"
  "dns|mhost-prism|prism.dev.toml|PRISM_SERVER__BIND|PRISM_SERVER__METRICS_BIND|"
  "tls|tlsight|tlsight.dev.toml|TLSIGHT_SERVER__BIND|TLSIGHT_SERVER__METRICS_BIND|TLSIGHT_VALIDATION__CUSTOM_CA_DIR=$tmp/custom_cas"
  "http|spectra|spectra.dev.toml|SPECTRA__SERVER__BIND|SPECTRA__SERVER__METRICS_BIND|"
  "email|beacon|beacon.dev.toml|BEACON__SERVER__BIND|BEACON__SERVER__METRICS_BIND|"
  "ip|ifconfig-rs|$REPO_ROOT/tests/repo/fixtures/ifconfig.smoke.toml|IFCONFIG_SERVER__BIND|IFCONFIG_SERVER__ADMIN_BIND|"
)

for row in "${rows[@]}"; do
    IFS='|' read -r sub dir cfg bind_var metrics_var extra <<<"$row"
    port=$(free_port)
    mport=$(free_port)
    log="$tmp/$sub.log"

    (
        cd "$REPO_ROOT/crates/$dir" || exit 1
        # shellcheck disable=SC2086
        start_bg "$log" env "$bind_var=127.0.0.1:$port" "$metrics_var=127.0.0.1:$mport" $extra \
            "$bin" "$sub" "$cfg"
        echo "$!" >"$tmp/$sub.pid"
    )
    pid=$(cat "$tmp/$sub.pid")
    _NETRAY_PIDS+=("$pid")

    wait_http "http://127.0.0.1:$port/health" 30 \
        || fail "netray $sub did not answer on 127.0.0.1:$port ($(tail -n 1 "$log" 2>/dev/null))"

    # A browser User-Agent: ifconfig-rs answers `/` with the plain-text IP to curl.
    get() { curl -s -A "Mozilla/5.0" -o "$2" -w '%{http_code}' --max-time 5 "http://127.0.0.1:$port$1"; }

    [ "$(get / "$tmp/$sub.root")" = 200 ] || fail "netray $sub: GET / is not 200"
    [ "$(get /health "$tmp/$sub.health")" = 200 ] || fail "netray $sub: GET /health is not 200"
    [ "$(get /does-not-exist "$tmp/$sub.unknown")" = 200 ] || fail "netray $sub: GET /does-not-exist is not 200"

    grep -q '<html' "$tmp/$sub.root" || fail "netray $sub: GET / body has no <html"
    cmp -s "$tmp/$sub.root" "$tmp/$sub.unknown" \
        || fail "netray $sub: GET /does-not-exist is not the same body as GET /"

    kill "$pid" 2>/dev/null
done

echo "OK: all six service subcommands serve /, /health and the SPA fallback"
