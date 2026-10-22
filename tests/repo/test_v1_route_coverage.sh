#!/usr/bin/env bash
# specs/features/v2-cutover, requirement 15: every route of the V1 routers (deleted in Phase 7,
# so the list is kept here) and every site/ path of rows 13l-n has a row in
# crates/web/tests/it/host_redirects.rs. A row is a line starting `("<host>", "<METHOD>", "<path>`.
#
# Matching rule: a plain path needs a row for exactly that path (the path is followed by `"` or
# `?`). A templated path (`{...}`) is written below as its prefix up to the first `{` and ends in
# `*`: it needs a row whose path starts with that prefix, i.e. a concrete instance.
# The check is a function over a table file, self-tested on a copy with one row deleted.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

TABLE=crates/web/tests/it/host_redirects.rs
git ls-files --error-unmatch "$TABLE" >/dev/null 2>&1 || { echo "FAIL: $TABLE is not tracked"; exit 1; }

# host|path pairs; the apex is netray.info. `/` and the probes on inspector hosts are excluded.
routes() {
    local f n h
    # dns: crates/dns/src/api/mod.rs:198-215
    for n in authcompare check compare config dnssec meta parse query record-types servers trace; do
        echo "dns.netray.info|/api/$n"
    done
    echo "dns.netray.info|/api/results/*"
    # tls, http: crates/{tls,http}/src/routes.rs
    for h in tls http; do echo "$h.netray.info|/api/inspect"; echo "$h.netray.info|/api/meta"; done
    echo "http.netray.info|/api/config"
    # email
    echo "email.netray.info|/inspect"
    echo "email.netray.info|/inspect/*"
    echo "email.netray.info|/api/meta"
    # /docs, /docs/ and the OpenAPI document on the five subdomains (rows 13h)
    for h in dns tls http email ip; do
        echo "$h.netray.info|/docs"
        echo "$h.netray.info|/docs/"
        echo "$h.netray.info|/api-docs/openapi.json"
    done
    # ip: crates/ip/src/routes.rs:107-166 and requirement 14
    for n in /json /yaml /toml /csv /ip /ip/json /ip/cidr /range /diff /batch /batch/json /meta /api/meta \
             /all /all/json /asn /asn/13335; do
        echo "ip.netray.info|$n"
    done
    for f in tcp location user_agent network host isp country city timezone latitude longitude region headers ipv4 ipv6; do
        echo "ip.netray.info|/$f"
        echo "ip.netray.info|/$f/json"
    done
    # lens: crates/lens/src/lib.rs:155-161, reached through row 13a
    # and lens's /og/{domain_png} and /r/{shortid} with its sub-paths (crates/lens/src/routes.rs:1570-1573)
    for n in /api/check '/api/check/*' /api/meta /docs /api-docs/openapi.json '/og/*' '/r/*'; do
        echo "lens.netray.info|$n"
    done
    # apex: lens's routes after the host redirect, and site/ rows 13l-n
    for n in /api/check '/api/check/*' /api/meta /docs /docs/ /api-docs/openapi.json '/og/*' '/r/*' \
             /tools /tools/ /api/ /api /api/index /api/dns /api/tls /api/http /api/email /api/ip /api/lens \
             /guide/lens; do
        echo "netray.info|$n"
    done
    for n in index dns tls http email ip lens; do echo "netray.info|/api/$n.html"; done
}

# missing_routes <table-file>: one "host path" per route with no row.
missing_routes() {
    local table=$1 host path pat
    routes | while IFS='|' read -r host path; do
        if [[ $path == *'*' ]]; then
            pat="^[[:space:]]*\\(\"${host//./\\.}\", \"[A-Z]+\", \"${path%\*}"
        else
            pat="^[[:space:]]*\\(\"${host//./\\.}\", \"[A-Z]+\", \"${path}[\"?]"
        fi
        grep -Eq -- "$pat" "$table" || echo "$host $path"
    done
}

missing=$(missing_routes "$TABLE")
[ -z "$missing" ] || while IFS= read -r m; do fail "no row in $TABLE for V1 route: $m"; done <<<"$missing"

# self-test: a table without the ip /range row, and one without the dns /api/results row, fails naming it.
scratch=$(mktemp -d) || exit 1
trap 'rm -rf "$scratch"' EXIT
selftest() {
    local label=$1 pattern=$2 want=$3 out
    grep -Ev -- "$pattern" "$TABLE" >"$scratch/table.rs"
    cmp -s "$TABLE" "$scratch/table.rs" && { fail "self-test $label: pattern deleted no row"; return; }
    out=$(missing_routes "$scratch/table.rs")
    [ "$out" = "$want" ] || fail "self-test $label: expected the check to name only '$want', got '$out'"
}
selftest "plain path" '^[[:space:]]*\("ip\.netray\.info", "[A-Z]+", "/range"' "ip.netray.info /range"
selftest "templated path" '^[[:space:]]*\("dns\.netray\.info", "[A-Z]+", "/api/results/' "dns.netray.info /api/results/*"

[ "$fails" -eq 0 ] && echo "ok: every V1 route has a row"
exit "$fails"
