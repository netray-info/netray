#!/usr/bin/env bash
# Spec monorepo-p3, Phase 2 (D3, D4, R6, R7): every response of `lens dns tls http
# email ip` carries the production header set Traefik applies today (secure-headers,
# csp-tool-spa, cors-public-api); /docs keeps a relaxed CSP; a CORS preflight answers
# with the D3 methods/headers/max-age; `netray site` keeps its own set (D4); ifconfig-rs
# no longer hand-rolls HSTS or CSP (R7).
#
# Start-up mirrors test_smoke_services.sh. Failures accumulate; the script exits 1 at the end.
set -uo pipefail
source "$(dirname "$0")/lib/netray.sh"

bin=$(netray_bin) || exit 1
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"; _netray_cleanup' EXIT
mkdir -p "$tmp/custom_cas"

failures=0
bad() { echo "FAIL: $*" >&2; failures=$((failures + 1)); }

# hdr <file> <name> -> value of the first header of that name (case-insensitive), trimmed
hdr() { grep -i "^$2:" "$1" | head -n 1 | sed -E 's/^[^:]*:[[:space:]]*//; s/[[:space:]]*$//'; }
has_hdr() { grep -qi "^$2:" "$1"; }

# expect <label> <file> <name> <exact value>
expect() {
    local got
    got=$(hdr "$2" "$3")
    [ "$got" = "$4" ] || bad "$1: $3 is '$got', want '$4'"
}
absent() { has_hdr "$2" "$3" && bad "$1: header $3 must be absent"; return 0; }
# contains <label> <file> <name> <substring>
contains() {
    local got
    got=$(hdr "$2" "$3")
    case "$got" in *"$4"*) ;; *) bad "$1: $3 is '$got', want it to contain '$4'" ;; esac
}
present() { has_hdr "$2" "$3" || bad "$1: header $3 missing"; }

CSP="default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; font-src 'self'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; object-src 'none'; form-action 'self'"
PP='camera=(), microphone=(), geolocation=(), payment=()'

# fetch <port> <out-file> <curl args...> -> status code on stdout; response headers in out-file
fetch() {
    local port=$1 out=$2
    shift 2
    curl -s -A "Mozilla/5.0" -D "$out" -o /dev/null -w '%{http_code}' --max-time 5 "$@" 2>/dev/null
    sed -i.bak 's/\r$//' "$out" 2>/dev/null; rm -f "$out.bak"
}

# sub | crate dir | config | http bind var | metrics bind var | extra env | HSTS
rows=(
  "lens|lens|lens.dev.toml|LENS_SERVER__BIND|LENS_SERVER__METRICS_BIND|LENS_SNAPSHOTS__DB_PATH=$tmp/snapshots.db|max-age=31536000; includeSubDomains; preload"
  "dns|mhost-prism|prism.dev.toml|PRISM_SERVER__BIND|PRISM_SERVER__METRICS_BIND||max-age=31536000; includeSubDomains; preload"
  "tls|tlsight|tlsight.dev.toml|TLSIGHT_SERVER__BIND|TLSIGHT_SERVER__METRICS_BIND|TLSIGHT_VALIDATION__CUSTOM_CA_DIR=$tmp/custom_cas|max-age=31536000; includeSubDomains; preload"
  "http|http|spectra.dev.toml|NETRAY_HTTP_SERVER__BIND|NETRAY_HTTP_SERVER__METRICS_BIND||max-age=31536000; includeSubDomains; preload"
  "email|email|beacon.dev.toml|BEACON__SERVER__BIND|BEACON__SERVER__METRICS_BIND||max-age=31536000; includeSubDomains; preload"
  "ip|ifconfig-rs|$REPO_ROOT/tests/repo/fixtures/ifconfig.smoke.toml|IFCONFIG_SERVER__BIND|IFCONFIG_SERVER__ADMIN_BIND||max-age=63072000; includeSubDomains; preload"
)

for row in "${rows[@]}"; do
    IFS='|' read -r sub dir cfg bind_var metrics_var extra hsts <<<"$row"
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
    if ! wait_http "http://127.0.0.1:$port/health" 30; then
        bad "$sub: did not answer on 127.0.0.1:$port ($(tail -n 1 "$log" 2>/dev/null))"
        continue
    fi
    base="http://127.0.0.1:$port"

    for path in / /health; do
        l="$sub GET $path"
        h="$tmp/$sub.h"
        fetch "$port" "$h" "$base$path" >/dev/null
        expect "$l" "$h" strict-transport-security "$hsts"
        expect "$l" "$h" x-frame-options DENY
        expect "$l" "$h" x-content-type-options nosniff
        expect "$l" "$h" referrer-policy strict-origin-when-cross-origin
        expect "$l" "$h" permissions-policy "$PP"
        expect "$l" "$h" cross-origin-opener-policy same-origin
        expect "$l" "$h" content-security-policy "$CSP"
        expect "$l" "$h" access-control-allow-origin '*'
        expect "$l" "$h" cross-origin-resource-policy cross-origin
        absent "$l" "$h" server
        absent "$l" "$h" x-powered-by
    done

    l="$sub GET /docs"
    h="$tmp/$sub.h"
    code=$(fetch "$port" "$h" "$base/docs")
    case "$code" in 2* | 3*) ;; *) bad "$l: status $code, want 2xx/3xx" ;; esac
    present "$l" "$h" strict-transport-security
    present "$l" "$h" x-frame-options
    expect "$l" "$h" x-content-type-options nosniff
    expect "$l" "$h" access-control-allow-origin '*'
    contains "$l" "$h" content-security-policy "https://cdn.jsdelivr.net"
    [ "$(hdr "$h" content-security-policy)" != "$CSP" ] || bad "$l: CSP must differ from csp-tool-spa"

    # The docs page's own CDN scripts must match a script-src source under CSP source
    # matching (host-only, a path ending in "/" as prefix, or the exact URL); containing
    # the host as text is not enough.
    curl -sL -D "$tmp/$sub.docs.h" -o "$tmp/$sub.docs.html" "$base/docs"
    docs_csp=$(grep -i '^content-security-policy:' "$tmp/$sub.docs.h" | tail -n1 | cut -d: -f2- | tr -d '\r')
    scripts=$(tr '\n' ' ' <"$tmp/$sub.docs.html" | grep -oE '<script[^>]*src="https://cdn\.jsdelivr\.net[^"]*"' | sed -E 's/.*src="([^"]*)"/\1/')
    [ -n "$scripts" ] || bad "$l: the docs page loads no jsDelivr script"
    for src in $scripts; do
        python3 -I - "$docs_csp" "$src" <<'PY' || bad "$l: script $src is not allowed by script-src"
import sys
csp, url = sys.argv[1], sys.argv[2]
srcs = next((d.split()[1:] for d in csp.split(";") if d.split()[:1] == ["script-src"]), [])
def ok(s):
    if not s.startswith("https://"): return False
    host = s[len("https://"):].split("/", 1)
    uhost = url[len("https://"):].split("/", 1)
    if host[0] != uhost[0]: return False
    if len(host) == 1 or host[1] == "": return True
    path = "/" + host[1]
    upath = "/" + (uhost[1] if len(uhost) > 1 else "")
    return upath.startswith(path) if path.endswith("/") else upath == path
sys.exit(0 if any(ok(s) for s in srcs) else 1)
PY
    done

    l="$sub OPTIONS /health"
    code=$(fetch "$port" "$h" -X OPTIONS -H 'Origin: https://example.com' \
        -H 'Access-Control-Request-Method: POST' -H 'Access-Control-Request-Headers: content-type' "$base/health")
    case "$code" in 2*) ;; *) bad "$l: status $code, want 2xx" ;; esac
    expect "$l" "$h" access-control-allow-origin '*'
    methods=$(hdr "$h" access-control-allow-methods | tr -d ' ' | tr a-z A-Z)
    for m in GET POST OPTIONS; do
        case ",$methods," in *",$m,"*) ;; *) bad "$l: access-control-allow-methods '$methods' lacks $m" ;; esac
    done
    allow=$(hdr "$h" access-control-allow-headers | tr -d ' ' | tr A-Z a-z)
    for k in content-type accept; do
        case ",$allow," in *",$k,"*) ;; *) bad "$l: access-control-allow-headers '$allow' lacks $k" ;; esac
    done
    expect "$l" "$h" access-control-max-age 600
    # architecture-rules: X-Request-Id on every response, preflights included
    present "$l" "$h" x-request-id

    kill "$pid" 2>/dev/null
done

# D4: netray site keeps its own header set
port=$(free_port)
# No subshell: start_bg must record the PID in this shell, or the EXIT trap leaks the server.
cd "$REPO_ROOT" || exit 1
start_bg "$tmp/site.log" "$bin" site --bind "127.0.0.1:$port" --root site
if wait_http "http://127.0.0.1:$port/" 30; then
    l="site GET /"
    h="$tmp/site.h"
    fetch "$port" "$h" "http://127.0.0.1:$port/" >/dev/null
    expect "$l" "$h" strict-transport-security "max-age=31536000; includeSubDomains; preload"
    expect "$l" "$h" x-frame-options DENY
    expect "$l" "$h" x-content-type-options nosniff
    expect "$l" "$h" referrer-policy strict-origin-when-cross-origin
    expect "$l" "$h" permissions-policy "$PP"
    expect "$l" "$h" cross-origin-opener-policy same-origin
    expect "$l" "$h" cross-origin-resource-policy same-origin
    contains "$l" "$h" content-security-policy "connect-src 'self' https://stats.uptimerobot.com"
else
    bad "site: did not answer on 127.0.0.1:$port"
fi

# R7 / C7: HSTS and CSP live in netray-common, not in ifconfig-rs
# Header names as strings and as axum/http constants, in every source file of the crate.
hits=$(grep -rniE 'strict-transport-security|content-security-policy|STRICT_TRANSPORT_SECURITY|CONTENT_SECURITY_POLICY' \
    "$REPO_ROOT/crates/ifconfig-rs/src" 2>/dev/null)
[ -z "$hits" ] || bad "R7: ifconfig-rs sets HSTS/CSP itself: $(head -n1 <<<"$hits")"
[ -n "$(find "$REPO_ROOT/crates/ifconfig-rs/src" -name '*.rs' | head -n1)" ] || bad "R7: no ifconfig-rs sources found"
awk '/pub struct SecurityHeadersConfig/,/^}/' "$REPO_ROOT/crates/common/src/security_headers.rs" | grep -qi 'hsts' \
    || bad "R7: SecurityHeadersConfig has no hsts field"

[ "$failures" -eq 0 ] || { echo "$failures assertion(s) failed" >&2; exit 1; }
echo "PASS: test_header_parity"
