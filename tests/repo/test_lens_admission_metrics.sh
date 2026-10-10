#!/usr/bin/env bash
# specs/features/lens-admission-metrics R2, R4, R5: `netray lens` installs its recorder with the
# fresh-run buckets and creates the zero series before serving. Every backend points at a closed
# port, so a fresh run finishes offline and its duration renders as a histogram, not a summary.
set -u
source "$(dirname "$0")/lib/netray.sh"
cd "$REPO_ROOT" || exit 1

fail() { echo "FAIL: $*" >&2; exit 1; }

NETRAY=$(netray_bin) || exit 1
TMP=$(mktemp -d)
trap '_netray_cleanup; rm -rf "$TMP"' EXIT

p=$(free_port); m=$(free_port)
sed -E "s|^bind = .*|bind = \"127.0.0.1:$p\"|; s|^metrics_bind = .*|metrics_bind = \"127.0.0.1:$m\"|; s|^url = .*|url = \"http://127.0.0.1:1\"|; s|^db_path = .*|db_path = \"$TMP/snapshots.db\"|" \
    crates/lens/lens.dev.toml > "$TMP/lens.toml"
start_bg "$TMP/lens.log" "$NETRAY" lens "$TMP/lens.toml"
wait_http "http://127.0.0.1:$p/health" 30 || fail "netray lens did not come up on $p"

body=$(curl -s --max-time 5 "http://127.0.0.1:$m/metrics") || fail "no metrics endpoint on $m"
for series in \
    'lens_check_requests_total{result="fresh"} 0' \
    'lens_check_requests_total{result="cache_hit"} 0' \
    'lens_check_requests_total{result="rate_limited"} 0' \
    'lens_badge_requests_total{cache="hit"} 0' \
    'lens_badge_requests_total{cache="miss"} 0' \
    'lens_badge_requests_total{cache="throttled"} 0' \
    'lens_badge_requests_total{cache="failed"} 0' \
    'lens_rate_limit_hits_total{scope="per_ip"} 0' \
    'lens_rate_limit_hits_total{scope="global"} 0' \
    'lens_rate_limit_hits_total{scope="badge"} 0' \
    'lens_rate_limit_hits_total{scope="og"} 0' \
    'lens_runs_in_flight 0'; do
    printf '%s\n' "$body" | grep -qxF -- "$series" || fail "zero series missing before any request: $series"
done

curl -s -o /dev/null --max-time 30 "http://127.0.0.1:$p/api/check/example.com?sync=true"
body=$(curl -s --max-time 5 "http://127.0.0.1:$m/metrics")
printf '%s\n' "$body" | grep -qxF 'lens_check_requests_total{result="fresh"} 1' \
    || fail "fresh run not counted"
for le in 0.5 1 2 5 10 15 20 30 +Inf; do
    printf '%s\n' "$body" | grep -qF "lens_run_duration_seconds_bucket{le=\"$le\"} " \
        || fail "lens_run_duration_seconds lacks bucket le=$le"
done
printf '%s\n' "$body" | grep -q '^lens_run_duration_seconds{quantile=' \
    && fail "lens_run_duration_seconds renders as a summary"
printf '%s\n' "$body" | grep -qxF 'lens_runs_in_flight 0' || fail "runs in flight not back at 0"

echo "PASS"
