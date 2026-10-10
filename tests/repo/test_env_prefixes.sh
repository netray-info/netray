#!/usr/bin/env bash
# Phase 1, requirement 3 (env prefix): each service reads NETRAY_<SVC>_ variables only.
# The legacy prefix and legacy *_CONFIG variable fail fast and name the new prefix.
# C6: no file of the old crates/spectra, beacon or ifconfig-rs directory stays tracked.
set -u
source "$(dirname "$0")/lib/netray.sh"
cd "$REPO_ROOT" || exit 1

fail() { echo "FAIL: $*" >&2; exit 1; }

NETRAY=$(netray_bin) || exit 1
TMP=$(mktemp -d)
trap '_netray_cleanup; rm -rf "$TMP"' EXIT

# sub | dev config | new prefix | legacy prefix | legacy config var | second bind key
rows=(
  "http|crates/http/spectra.dev.toml|NETRAY_HTTP_|SPECTRA__|SPECTRA_CONFIG|METRICS_BIND"
  "email|crates/email/beacon.dev.toml|NETRAY_EMAIL_|BEACON__|BEACON_CONFIG|METRICS_BIND"
  "ip|$REPO_ROOT/tests/repo/fixtures/ifconfig.smoke.toml|NETRAY_IP_|IFCONFIG_|IFCONFIG_CONFIG|ADMIN_BIND"
)

# run_expect_reject <log> <expected text> <env assignment> <netray args...>
# returns 0 = rejected as expected, 1 = did not exit non-zero in time, 2 = output lacks text
run_expect_reject() {
    local log=$1 want=$2 assign=$3 pid i
    shift 3
    env "$assign" "$NETRAY" "$@" >"$log" 2>&1 &
    pid=$!
    for ((i = 0; i < 40; i++)); do
        kill -0 "$pid" 2>/dev/null || break
        sleep 0.25
    done
    if kill -0 "$pid" 2>/dev/null; then
        kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
        return 1
    fi
    wait "$pid" && return 1
    grep -q "$want" "$log" || return 2
    return 0
}

for row in "${rows[@]}"; do
    IFS='|' read -r sub cfg new old oldcfg bind2 <<<"$row"
    p=$(free_port); m=$(free_port); q=$(free_port)

    # (a) new prefix is read
    start_bg "$TMP/$sub-new.log" \
        env "${new}SERVER__BIND=127.0.0.1:$p" "${new}SERVER__${bind2}=127.0.0.1:$m" "$NETRAY" "$sub"
    wait_http "http://127.0.0.1:$p/health" 30 \
        || fail "netray $sub ignores ${new}SERVER__BIND (no /health on $p)"

    # (b) legacy prefix is rejected, naming the new one
    run_expect_reject "$TMP/$sub-old.log" "$new" "${old}SERVER__BIND=127.0.0.1:$q" "$sub" "$cfg"
    case $? in
        0) ;;
        2) fail "netray $sub: rejection of ${old} does not name ${new}" ;;
        *) fail "netray $sub did not exit non-zero within 10 s with ${old}SERVER__BIND set" ;;
    esac

    # (c) legacy config variable is rejected, naming the new one
    run_expect_reject "$TMP/$sub-oldcfg.log" "${new}CONFIG" "$oldcfg=$cfg" "$sub"
    case $? in
        0) ;;
        2) fail "netray $sub: rejection of $oldcfg does not name ${new}CONFIG" ;;
        *) fail "netray $sub did not exit non-zero within 10 s with $oldcfg set" ;;
    esac
done

# --- C6: the old crate directory is gone from the index ---------------------
if git ls-files | grep -q '^crates/spectra/'; then
    fail "files under crates/spectra/ are still tracked"
fi
if git ls-files | grep -q '^crates/beacon/'; then
    fail "files under crates/beacon/ are still tracked"
fi

if git ls-files | grep -q '^crates/ifconfig-rs/'; then
    fail "files under crates/ifconfig-rs/ are still tracked"
fi

echo "PASS"
