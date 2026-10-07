#!/usr/bin/env bash
# C8 + C9 (and the config/metrics half of C2): one `netray` binary keeps each service's
# config precedence (argument, then <SVC>_CONFIG) and its Prometheus metric prefix.
# Baselines measured with the separate binaries: spectra_http_requests_total,
# prism_http_requests_total.
set -u
source "$(dirname "$0")/lib/netray.sh"
cd "$REPO_ROOT" || exit 1

fail() { echo "FAIL: $*" >&2; exit 1; }

NETRAY=$(netray_bin) || exit 1
TMP=$(mktemp -d)
trap '_netray_cleanup; rm -rf "$TMP"' EXIT

# make_cfg <src.toml> <dest> <bind port> <metrics port>
make_cfg() {
    sed -E "s/^bind = .*/bind = \"127.0.0.1:$3\"/; s/^metrics_bind = .*/metrics_bind = \"127.0.0.1:$4\"/" "$1" >"$2"
}

# --- C8: argument beats <SVC>_CONFIG ---------------------------------------
# check_precedence <subcommand> <ENV_VAR> <dev toml> <expect config_source in log: yes|no>
check_precedence() {
    local sub=$1 var=$2 src=$3 want_log=$4
    local pa pb ma mb
    pa=$(free_port); pb=$(free_port); ma=$(free_port); mb=$(free_port)
    make_cfg "$src" "$TMP/$sub-a.toml" "$pa" "$ma"
    make_cfg "$src" "$TMP/$sub-b.toml" "$pb" "$mb"
    env "$var=$TMP/$sub-b.toml" start_bg "$TMP/$sub-prec.log" "$NETRAY" "$sub" "$TMP/$sub-a.toml"
    wait_http "http://127.0.0.1:$pa/health" 30 \
        || fail "netray $sub does not listen on the port of the config argument ($pa)"
    if curl -s -o /dev/null --max-time 1 "http://127.0.0.1:$pb/health"; then
        fail "netray $sub also listens on the port of $var ($pb): env beat the argument"
    fi
    if [ "$want_log" = yes ]; then
        grep -Eq 'config_source[^A-Za-z]{1,12}argv' "$TMP/$sub-prec.log" \
            || fail "netray $sub startup log lacks config_source=argv"
        grep -Eq 'config_source[^A-Za-z]{1,12}BEACON_CONFIG' "$TMP/$sub-prec.log" \
            && fail "netray $sub startup log reports config_source=BEACON_CONFIG"
    fi
}

check_precedence email BEACON_CONFIG crates/beacon/beacon.dev.toml yes
# spectra: argument, then SPECTRA_CONFIG (no config_source log field)
check_precedence http SPECTRA_CONFIG crates/spectra/spectra.dev.toml no

# --- C9: metric names keep the service prefix ------------------------------
# check_metrics <subcommand> <ENV_PREFIX> <metric prefix>
check_metrics() {
    local sub=$1 envp=$2 prefix=$3 p m
    p=$(free_port); m=$(free_port)
    env "${envp}SERVER__BIND=127.0.0.1:$p" "${envp}SERVER__METRICS_BIND=127.0.0.1:$m" \
        start_bg "$TMP/$sub-metrics.log" "$NETRAY" "$sub"
    wait_http "http://127.0.0.1:$p/health" 30 || fail "netray $sub did not come up on $p"
    curl -s -o /dev/null "http://127.0.0.1:$p/api/meta"
    local body
    body=$(curl -s --max-time 5 "http://127.0.0.1:$m/metrics") \
        || fail "netray $sub: no metrics endpoint on $m"
    echo "$body" | grep -Eq "^${prefix}_http_requests_total\{[^}]*path=\"/health\"" \
        || fail "netray $sub: metric ${prefix}_http_requests_total missing"
    echo "$body" | grep -Eq "^${prefix}_http_request_duration_seconds" \
        || fail "netray $sub: metric ${prefix}_http_request_duration_seconds missing"
}

check_metrics http SPECTRA__ spectra
check_metrics dns PRISM_ prism

echo "PASS"
