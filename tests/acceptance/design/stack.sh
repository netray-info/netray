# Design stack helper: source it after tests/repo/lib/netray.sh.
#
#   design_stack_up <tmp> <fixtures> [host...]
#
# Seeds a fresh history store from <fixtures>, starts lens and each named host (dns tls http email ip),
# and exports LOCAL_<NAME>_URL for each. The caller owns the EXIT trap (_netray_cleanup).
#
# lens runs with a cache ttl of 315360000 s (ten years): larger than any seeded fixture's age (up to 14 days),
# so every stored state is fresh and the result page renders it without starting a live run, and below
# moka's maximum ttl. A client budget of 0 admits no fresh run at all, so a visit can never start one.

design_stack_up() {
    local tmp=$1 fixtures=$2
    shift 2
    local bin
    bin=$(netray_bin) || return 2
    mkdir -p "$tmp/custom_cas"

    cargo run -q -p netray-store --example seed -- "$tmp/history.db" "$fixtures" \
        || { echo "seed failed" >&2; return 2; }

    # sub | crate dir | config | bind var | metrics bind var | extra env (see the justfile's acceptance-local)
    local rows=(
      "lens|lens|lens.dev.toml|LENS_SERVER__BIND|LENS_SERVER__METRICS_BIND|LENS_SNAPSHOTS__DB_PATH=$tmp/snapshots.db LENS_HISTORY__DB_PATH=$tmp/history.db LENS_CACHE__TTL_SECONDS=315360000 LENS_ADMISSION__CLIENT_BUDGET_PER_HOUR=0"
      "dns|dns|prism.dev.toml|NETRAY_DNS_SERVER__BIND|NETRAY_DNS_SERVER__METRICS_BIND|"
      "tls|tls|tlsight.dev.toml|NETRAY_TLS_SERVER__BIND|NETRAY_TLS_SERVER__METRICS_BIND|NETRAY_TLS_VALIDATION__CUSTOM_CA_DIR=$tmp/custom_cas"
      "http|http|spectra.dev.toml|NETRAY_HTTP_SERVER__BIND|NETRAY_HTTP_SERVER__METRICS_BIND|"
      "email|email|beacon.dev.toml|NETRAY_EMAIL_SERVER__BIND|NETRAY_EMAIL_SERVER__METRICS_BIND|"
      "ip|ip|$REPO_ROOT/tests/repo/fixtures/ifconfig.smoke.toml|NETRAY_IP_SERVER__BIND|NETRAY_IP_SERVER__ADMIN_BIND|"
    )

    local wanted=(lens) host row
    for host in "$@"; do
        case "$host" in
            lens) ;;
            dns|tls|http|email|ip) wanted+=("$host") ;;
            *) echo "unknown design stack host: $host" >&2; return 2 ;;
        esac
    done

    # lens's config loader reads every LENS_* variable as a key: LENS_EXTRA itself must not reach it.
    unset LENS_EXTRA
    local sub dir cfg bind_var metrics_var extra port
    for row in "${rows[@]}"; do
        IFS='|' read -r sub dir cfg bind_var metrics_var extra <<<"$row"
        case " ${wanted[*]} " in *" $sub "*) ;; *) continue ;; esac
        port=$(free_port)
        (
            cd "$REPO_ROOT/crates/$dir" || exit 1
            # shellcheck disable=SC2086
            start_bg "$tmp/$sub.log" env "$bind_var=127.0.0.1:$port" "$metrics_var=127.0.0.1:$(free_port)" $extra \
                "$bin" "$sub" "$cfg"
            echo "$!" >"$tmp/$sub.pid"
        )
        _NETRAY_PIDS+=("$(cat "$tmp/$sub.pid")")
        wait_http "http://127.0.0.1:$port/health" 30 \
            || { echo "netray $sub did not answer on 127.0.0.1:$port: $(tail -n 1 "$tmp/$sub.log")" >&2; return 2; }
        export "LOCAL_$(printf %s "$sub" | tr a-z A-Z)_URL=http://localhost:$port"
    done
}
