#!/usr/bin/env bash
# v2-cutover requirement 10: `netray serve` reads NETRAY_ variables; every legacy prefix (the
# per-service NETRAY_<SVC>_ ones and the codenames') fails fast naming NETRAY_CONFIG.
# C6: no file of the old crates/spectra, beacon or ifconfig-rs directory stays tracked.
set -u
source "$(dirname "$0")/lib/netray.sh"
cd "$REPO_ROOT" || exit 1

fail() { echo "FAIL: $*" >&2; exit 1; }

NETRAY=$(netray_bin) || exit 1
TMP=$(mktemp -d)
trap '_netray_cleanup; rm -rf "$TMP"' EXIT

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

# (a) serve reads NETRAY_
p=$(free_port); m=$(free_port)
start_bg "$TMP/serve.log" env "NETRAY_SERVER__BIND=127.0.0.1:$p" "NETRAY_SERVER__METRICS_BIND=127.0.0.1:$m" \
    "NETRAY_STORE__DB_PATH=$TMP/history.db" "$NETRAY" serve
wait_http "http://127.0.0.1:$p/health" 30 || fail "netray serve ignores NETRAY_SERVER__BIND (no /health on $p)"

# (b) every legacy prefix is refused, naming NETRAY_CONFIG (requirement 10)
for old in LENS_ NETRAY_DNS_ NETRAY_TLS_ NETRAY_HTTP_ NETRAY_EMAIL_ NETRAY_IP_ PRISM_ TLSIGHT_ SPECTRA_ BEACON_ IFCONFIG_; do
    run_expect_reject "$TMP/$old.log" "NETRAY_CONFIG" "${old}SERVER__BIND=127.0.0.1:$(free_port)" serve
    case $? in
        0) ;;
        2) fail "netray serve: refusal of ${old} does not name NETRAY_CONFIG" ;;
        *) fail "netray serve did not exit non-zero within 10 s with ${old}SERVER__BIND set" ;;
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

if git ls-files | grep -q '^crates/tlsight/'; then
    fail "files under crates/tlsight/ are still tracked"
fi

if git ls-files | grep -q '^crates/mhost-prism/'; then
    fail "files under crates/mhost-prism/ are still tracked"
fi

echo "PASS"
