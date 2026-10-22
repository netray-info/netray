# Design stack helper: source it after tests/repo/lib/netray.sh.
#
#   design_stack_up <tmp> <fixtures> [host...]
#
# Seeds a fresh history store from <fixtures>, starts one `netray serve` with apex `localhost`, and
# exports LOCAL_LENS_URL (the apex, `http://localhost:<port>`) and LOCAL_<NAME>_URL
# (`http://<name>.localhost:<port>`) for each named host (dns tls http email ip). The browser
# resolves `*.localhost` to the loopback address, so `serve` routes each by its Host. The caller
# owns the EXIT trap (_netray_cleanup).
#
# The cache window is 315360000 s (ten years): larger than any seeded fixture's age (up to 14 days),
# so every stored state is fresh and the result page renders it without starting a live run. A
# client budget of 0 admits no fresh run at all, so a visit can never start one. The TLS inspector's
# limiters are raised: a run is charged its probe connections (v2-tls-probe-close), and the light
# and dark boards inspect the same target seconds apart from one client.

design_stack_up() {
    local tmp=$1 fixtures=$2
    shift 2
    local bin host port
    bin=$(netray_bin) || return 2

    for host in "$@"; do
        case "$host" in
            lens|dns|tls|http|email|ip) ;;
            *) echo "unknown design stack host: $host" >&2; return 2 ;;
        esac
    done

    cargo run -q -p netray-store --example seed -- "$tmp/history.db" "$fixtures" \
        || { echo "seed failed" >&2; return 2; }

    port=$(free_port)
    start_bg "$tmp/serve.log" env \
        NETRAY_SERVER__BIND="127.0.0.1:$port" NETRAY_SERVER__METRICS_BIND="127.0.0.1:$(free_port)" \
        NETRAY_SERVER__APEX=localhost NETRAY_STORE__DB_PATH="$tmp/history.db" \
        NETRAY_ADMISSION__CACHE_WINDOW_SECONDS=315360000 NETRAY_ADMISSION__CLIENT_BUDGET_PER_HOUR=0 \
        NETRAY_API__TLS__PER_IP_BURST=200 NETRAY_API__TLS__PER_IP_PER_MINUTE=600 \
        NETRAY_API__TLS__PER_TARGET_BURST=200 NETRAY_API__TLS__PER_TARGET_PER_MINUTE=600 \
        "$bin" serve "$REPO_ROOT/crates/netray/netray.dev.toml"
    wait_http "http://127.0.0.1:$port/health" 30 \
        || { echo "netray serve did not answer on 127.0.0.1:$port: $(tail -n 1 "$tmp/serve.log")" >&2; return 2; }
    export LOCAL_LENS_URL="http://localhost:$port"
    for host in "$@"; do
        [ "$host" = lens ] && continue
        export "LOCAL_$(printf %s "$host" | tr a-z A-Z)_URL=http://$host.localhost:$port"
    done
}
