# Shared helpers for the tests/repo smoke tests. Source it; it is not a test itself.
#
#   source "$(dirname "$0")/lib/netray.sh"
#   netray_bin            -> path of the debug `netray` binary, built on first use
#   free_port             -> prints an unused TCP port on 127.0.0.1
#   start_bg <log> cmd... -> starts cmd in the background, records its pid for cleanup
#   wait_http <url> [s]   -> waits until <url> answers (any status), default 30 s
#
# Every process started with start_bg is killed on exit.

_NETRAY_PIDS=()
_netray_cleanup() {
    local pid
    for pid in "${_NETRAY_PIDS[@]:-}"; do
        [ -n "$pid" ] && kill "$pid" 2>/dev/null
    done
    wait 2>/dev/null
}
trap _netray_cleanup EXIT

REPO_ROOT=$(git rev-parse --show-toplevel)

netray_bin() {
    if ! cargo build -q -p netray --bin netray >&2; then
        echo "FAIL: cannot build the netray binary (cargo build -p netray)" >&2
        return 1
    fi
    echo "$REPO_ROOT/target/debug/netray"
}

free_port() {
    python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()'
}

start_bg() {
    local log=$1
    shift
    "$@" >"$log" 2>&1 &
    _NETRAY_PIDS+=("$!")
}

wait_http() {
    local url=$1 timeout=${2:-30} i
    for ((i = 0; i < timeout * 4; i++)); do
        curl -s -o /dev/null --max-time 1 "$url" && return 0
        sleep 0.25
    done
    return 1
}
