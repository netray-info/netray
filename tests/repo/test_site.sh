#!/usr/bin/env bash
# C4, C10, C11, C12: `netray site` routing, MTA-STS host, security headers, cache headers.
set -uo pipefail
cd "$(dirname "$0")/.." && cd .. || { echo "FAIL: cannot cd to repo root"; exit 1; }
source tests/repo/lib/netray.sh

bin=$(netray_bin) || exit 1
port=$(free_port)
base="http://127.0.0.1:$port"
tmp=$(mktemp -d) || { echo "FAIL: mktemp failed"; exit 1; }
trap 'rm -rf "$tmp"; _netray_cleanup' EXIT

start_bg "$tmp/site.log" "$bin" site --bind "127.0.0.1:$port" --root site
wait_http "$base/guide/" 15 || { echo "FAIL: netray site did not answer on $base"; exit 1; }

fail() { echo "FAIL: $*"; exit 1; }

# fetch <name> [curl args...] <path>: writes $tmp/<name>.{h,b}, status in $status
fetch() {
    local name=$1; shift
    status=$(curl -s -o "$tmp/$name.b" -D "$tmp/$name.h" -w '%{http_code}' --max-time 5 "$@") \
        || fail "request failed: $*"
}
# hdr <name> <header>: value of header, case-insensitive name, CR stripped
hdr() { grep -i "^$2:" "$tmp/$1.h" | head -1 | sed -e "s/^[^:]*: *//" -e 's/\r$//'; }
expect_status() { [ "$status" = "$2" ] || fail "$1: expected $2, got $status"; }
expect_hdr() {
    local got; got=$(hdr "$1" "$2")
    [ "$got" = "$3" ] || fail "$1: header $2 expected '$3', got '$got'"
}

# C10 routing
fetch guide "$base/guide/";          expect_status "GET /guide/" 200
fetch dnssec "$base/guide/dnssec";   expect_status "GET /guide/dnssec" 200
for p in / /does-not-exist; do
    fetch nf "$base$p";              expect_status "GET $p" 404
    cmp -s "$tmp/nf.b" site/404.html || fail "GET $p: body differs from site/404.html"
done
fetch git "$base/.git/config";       expect_status "GET /.git/config" 404
fetch env "$base/.env";              expect_status "GET /.env" 404

# C11 MTA-STS
fetch mta -H 'Host: mta-sts.example.com' "$base/.well-known/mta-sts.txt"
expect_status "mta-sts policy" 200
case "$(hdr mta Content-Type)" in text/plain*) ;; *) fail "mta-sts Content-Type: '$(hdr mta Content-Type)'";; esac
expect_hdr mta Cache-Control "no-cache"
cmp -s "$tmp/mta.b" site/.well-known/mta-sts.txt || fail "mta-sts body differs from site/.well-known/mta-sts.txt"
fetch mtag -H 'Host: mta-sts.example.com' "$base/guide/"; expect_status "mta-sts host GET /guide/" 404
fetch mtap "$base/.well-known/mta-sts.txt"; expect_status "mta-sts without host" 200

# C12 security headers
csp="default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; font-src 'self'; img-src 'self' data:; connect-src 'self' https://stats.uptimerobot.com; frame-ancestors 'none'; base-uri 'self'; object-src 'none'; form-action 'self' https://*.netray.info"
for r in guide nf; do
    expect_hdr $r Content-Security-Policy "$csp"
    expect_hdr $r Cross-Origin-Resource-Policy "same-origin"
    expect_hdr $r Cross-Origin-Opener-Policy "same-origin"
    expect_hdr $r Strict-Transport-Security "max-age=31536000; includeSubDomains; preload"
    expect_hdr $r X-Frame-Options "DENY"
    expect_hdr $r X-Content-Type-Options "nosniff"
    expect_hdr $r Referrer-Policy "strict-origin-when-cross-origin"
    expect_hdr $r Permissions-Policy "camera=(), microphone=(), geolocation=(), payment=()"
done

# C4 cache
fetch css "$base/guide/style.css"; expect_status "GET /guide/style.css" 200
expect_hdr css Cache-Control "public, max-age=604800, immutable"
expect_hdr guide Cache-Control "public, max-age=3600"

echo "PASS: netray site routing, mta-sts, security and cache headers"
