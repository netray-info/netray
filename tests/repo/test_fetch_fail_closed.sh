#!/usr/bin/env bash
# specs/features/fetch-fail-closed: fetch.sh fails closed. Offline: a stub curl (real -f
# semantics) and a stub geoipupdate run first on PATH; fetch.sh is copied into a fresh dir
# per scenario because it cds to its own directory.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1
root=$PWD
src=crates/ifconfig-rs/data/fetch.sh

fails=0
fail() { echo "FAIL: $1"; fails=1; }

[ -f "$src" ] || { echo "FAIL: $src missing"; exit 1; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
stubs="$work/stubs"
mkdir -p "$stubs"

cat > "$stubs/curl" <<'STUB'
#!/usr/bin/env bash
# Minimal curl: -s -S -f (also clustered), -o FILE, URL. FAIL_URLS (space separated
# substrings) answer HTTP 404: with -f exit 22 and no output, without -f an HTML page, exit 0.
# HTML_URLS answer HTTP 200 with an HTML error page (exit 0 even with -f): a 2xx garbage body.
# Upstream drift (2026-10-08): openai.com/gptbot-ranges.txt answers 403 (like FAIL_URLS, but 403);
# the old googlebot.json URL answers 301 to common-crawlers.json: with -L the stub follows, without
# -L it writes the redirect HTML and exits 0 even under -f (curl -f only fails on >= 400).
# PARTIAL_URLS emulate a transfer dying mid-body: with -f part of the body is written, exit 22.
f=0 L=0 out="" url=""
while [ $# -gt 0 ]; do
    case "$1" in
        -o) out=$2; shift ;;
        --*) ;;
        -*) case "$1" in *f*) f=1 ;; esac; case "$1" in *L*) L=1 ;; esac ;;
        *) url=$1 ;;
    esac
    shift
done
emit() { if [ -n "$out" ]; then cat > "$out"; else cat; fi; }
for pat in ${FAIL_URLS:-}; do
    case "$url" in
        *"$pat"*)
            if [ "$f" = 1 ]; then
                echo "curl: (22) The requested URL returned error: 404" >&2
                exit 22
            fi
            echo '<html><body>404 Not Found</body></html>' | emit
            exit 0 ;;
    esac
done
for pat in ${HTML_URLS:-}; do
    case "$url" in
        *"$pat"*) echo '<html><body>Service Unavailable, try again later</body></html>' | emit; exit 0 ;;
    esac
done
for pat in ${PARTIAL_URLS:-}; do
    case "$url" in
        *"$pat"*)
            if [ "$f" = 1 ]; then
                printf '192.0.2.0/2' | emit
                echo "curl: (18) transfer closed with outstanding read data remaining" >&2
                exit 22
            fi ;;
    esac
done
case "$url" in
    *openai.com/gptbot-ranges.txt)
        if [ "$f" = 1 ]; then
            echo "curl: (22) The requested URL returned error: 403" >&2
            exit 22
        fi
        echo '<html><body>403 Forbidden</body></html>' | emit
        exit 0 ;;
    *developers.google.com/search/apis/ipranges/googlebot.json)
        if [ "$L" = 1 ]; then
            url=https://developers.google.com/static/crawling/ipranges/common-crawlers.json
        else
            echo '<html><body>Redirecting...</body></html>' | emit
            exit 0
        fi ;;
esac
prefixes='{"prefixes":[{"ipv4Prefix":"192.0.2.0/24","service":"svc","scope":"sc"},{"ipv6Prefix":"2001:db8::/32","service":"svc","scope":"sc"}]}'
case "$url" in
    *amazonaws.com*) echo '{"prefixes":[{"ip_prefix":"192.0.2.0/24","service":"S","region":"r"}],"ipv6_prefixes":[{"ipv6_prefix":"2001:db8::/32","service":"S","region":"r"}]}' ;;
    *gstatic.com/ipranges/cloud.json) echo "$prefixes" ;;
    *gstatic.com/ipranges/goog.json) echo "$prefixes" ;;
    *cloudflare.com/ips-v4) echo 192.0.2.0/24 ;;
    *cloudflare.com/ips-v6) echo 2001:db8::/32 ;;
    *oracle.com*) echo '{"regions":[{"region":"r","cidrs":[{"cidr":"192.0.2.0/24"}]}]}' ;;
    *fastly.com*) echo '{"addresses":["192.0.2.0/24"],"ipv6_addresses":["2001:db8::/32"]}' ;;
    *digitalocean.com*|*linode.com*) printf 'range,country,region,city\n192.0.2.0/24,US,US-CA,x\n' ;;
    *api.github.com*) echo '{"hooks":["192.0.2.0/24"],"verifiable_password_authentication":true}' ;;
    *cloud-ip-ranges.com*) echo 192.0.2.0/24 ;;
    *common-crawlers.json|*bingbot.json|*applebot.json) echo "$prefixes" ;;
    *openai.com/gptbot.json) echo '{"creationTime":"2026-10-08T00:00:00.000000","prefixes":[{"ipv4Prefix":"192.0.2.0/24"}]}' ;;
    *ipverse/as-metadata*) echo '{"64500":{"asn":64500,"metadata":{"category":"isp","networkRole":"transit","registered":"2020-01-01T00:00:00Z"}}}' ;;
    *regexes.yaml) printf 'user_agent_parsers:\n  - regex: (x)\n' ;;
    *spamhaus.org*) printf '; header\n192.0.2.0/24 ; SBL1\n' ;;
    *cinsscore.com*) printf '# cins\n192.0.2.1\n' ;;
    *) printf '192.0.2.0/24\n' ;;
esac | emit
STUB

cat > "$stubs/geoipupdate" <<'STUB'
#!/usr/bin/env bash
d=.
while [ $# -gt 0 ]; do [ "$1" = -d ] && { d=$2; shift; }; shift; done
for n in GeoLite2-City GeoLite2-ASN; do echo dummy > "$d/$n.mmdb"; done
STUB
chmod +x "$stubs/curl" "$stubs/geoipupdate"

data_files="regexes.yaml tor_exit_nodes.txt feodo_botnet_ips.txt vpn_ranges.txt cloud_provider_ranges.jsonl datacenter_ranges.txt bot_ranges.jsonl spamhaus_drop.txt cins_army_ips.txt as_metadata.jsonl"

# run_scenario DIR FAIL_URLS [HTML_URLS [PARTIAL_URLS [ARG]]] -> exit code in $rc
run_scenario() {
    mkdir -p "$1"
    cp "$root/$src" "$1/fetch.sh"
    rc=0
    (cd "$1" && PATH="$stubs:$PATH" FAIL_URLS="$2" HTML_URLS="${3:-}" PARTIAL_URLS="${4:-}" \
        bash ./fetch.sh "${5:-get_all}" >"$1.out" 2>&1) || rc=$?
}

# Anything in DIR that is neither fetch.sh, a final data file, nor a *.mmdb is a leftover.
leftovers() {
    local f n known
    for f in "$1"/* "$1"/.[!.]*; do
        [ -e "$f" ] || continue
        n=$(basename "$f")
        case "$n" in fetch.sh|*.mmdb) continue ;; esac
        known=0
        for d in $data_files; do [ "$n" = "$d" ] && known=1; done
        [ "$known" = 1 ] || echo "$n"
    done
}

# C4 / C2: Tor list 404.
d="$work/c4"; run_scenario "$d" "torbulkexitlist"
[ "$rc" -ne 0 ] || fail "C4: Tor list 404 but fetch.sh exited 0"
[ ! -e "$d/tor_exit_nodes.txt" ] || fail "C4: tor_exit_nodes.txt exists after Tor list 404"
l=$(leftovers "$d"); [ -z "$l" ] || fail "C2: leftovers after Tor failure: $(echo $l)"

# C5: only the IPv6 VPN list fails.
d="$work/c5"; run_scenario "$d" "vpn/ipv6.txt"
[ "$rc" -ne 0 ] || fail "C5: IPv6 VPN list failed but fetch.sh exited 0"
[ ! -e "$d/vpn_ranges.txt" ] || fail "C5: vpn_ranges.txt exists after IPv6 VPN list failure"
l=$(leftovers "$d"); [ -z "$l" ] || fail "C2: leftovers after VPN failure: $(echo $l)"

# C6: one of the three Spamhaus lists fails (each in turn).
for u in spamhaus.org/drop/drop.txt spamhaus.org/drop/edrop.txt spamhaus.org/drop/dropv6.txt; do
    d="$work/c6-$(basename "$u")"; run_scenario "$d" "$u"
    [ "$rc" -ne 0 ] || fail "C6: $u failed but fetch.sh exited 0"
    [ ! -e "$d/spamhaus_drop.txt" ] || fail "C6: spamhaus_drop.txt exists after $u failure"
    l=$(leftovers "$d"); [ -z "$l" ] || fail "C2: leftovers after $u failure: $(echo $l)"
done

# C7: an existing regexes.yaml survives a failing download of another file.
d="$work/c7"; mkdir -p "$d"; printf 'known: content\n' > "$d/regexes.yaml"
run_scenario "$d" "torbulkexitlist"
[ "$rc" -ne 0 ] || fail "C7: Tor list 404 but fetch.sh exited 0"
[ "$(cat "$d/regexes.yaml" 2>/dev/null)" = "known: content" ] || fail "C7: existing regexes.yaml changed"

# C8: everything served -> exit 0, every data file present and non-empty, nothing left over.
d="$work/c8"; run_scenario "$d" ""
[ "$rc" -eq 0 ] || fail "C8: all URLs served but fetch.sh exited $rc: $(tail -n3 "$d.out")"
for f in $data_files GeoLite2-City.mmdb GeoLite2-ASN.mmdb; do
    [ -s "$d/$f" ] || fail "C8: $f missing or empty after a successful run"
done
grep -q '"provider":"gptbot"' "$d/bot_ranges.jsonl" 2>/dev/null || fail "C8: bot_ranges.jsonl has no gptbot entry (from gptbot.json)"
grep -q '"provider":"googlebot"' "$d/bot_ranges.jsonl" 2>/dev/null || fail "C8: bot_ranges.jsonl has no googlebot entry"
l=$(leftovers "$d"); [ -z "$l" ] || fail "C8: leftovers after a successful run: $(echo $l)"

# C9: every curl invocation carries -f (alone or in a flag cluster).
n=0
while IFS= read -r line; do
    n=$((n + 1))
    printf '%s\n' "$line" | grep -qE 'curl +(-[A-Za-z]*f[A-Za-z]*|.* -[A-Za-z]*f[A-Za-z]*)( |$)' \
        || fail "C9: curl without -f: $(echo $line)"
done < <(grep -E '(^|[^A-Za-z_])curl( |$)' "$src" | grep -vE '^[[:space:]]*#')
[ "$n" -gt 0 ] || fail "C9: no curl invocation found in $src"

# C10: a later input is bad (2xx HTML body, or a download dying mid-pipeline) after earlier
# steps of a multi-step writer succeeded: no final file, no leftovers, non-zero exit.
partial_case() { # NAME TARGET FAIL_URLS HTML_URLS
    local d="$work/c10-$1"
    run_scenario "$d" "$3" "$4"
    [ "$rc" -ne 0 ] || fail "C10 $1: bad input but fetch.sh exited 0"
    [ ! -e "$d/$2" ] || fail "C10 $1: $2 exists after a later input was bad"
    l=$(leftovers "$d"); [ -z "$l" ] || fail "C10 $1: leftovers: $(echo $l)"
}
partial_case cloud  cloud_provider_ranges.jsonl "" oracle.com
partial_case bots   bot_ranges.jsonl            "" bing.com
partial_case asmeta as_metadata.jsonl           "" as-metadata
partial_case cins   cins_army_ips.txt           cinsscore ""

# C11: get never writes the final name directly: a transfer failing mid-body leaves no file.
for u in torbulkexitlist regexes.yaml; do
    d="$work/c11-$u"; run_scenario "$d" "" "" "$u"
    case "$u" in torbulkexitlist) t=tor_exit_nodes.txt ;; *) t=regexes.yaml ;; esac
    [ "$rc" -ne 0 ] || fail "C11: partial transfer of $u but fetch.sh exited 0"
    [ ! -e "$d/$t" ] || fail "C11: $t exists after a partial transfer of $u"
    l=$(leftovers "$d"); [ -z "$l" ] || fail "C11: leftovers after partial $u: $(echo $l)"
done

# C12 under concurrency: files owned by another run (pid 99999) survive this run, on a usage
# error (typo) and on a failing run alike. Intermediates live in a per-run dir .fetch.$$,
# never under shared names such as .aws.json.
for mode in typo tor cins; do
    d="$work/c12-$mode"; mkdir -p "$d"
    printf 'other run\n' > "$d/vpn_ranges.txt.tmp.99999"
    printf 'other aws\n' > "$d/.aws.json"
    mkdir -p "$d/.fetch.99999"; printf 'other aws\n' > "$d/.fetch.99999/aws.json"
    case "$mode" in
        typo) run_scenario "$d" "" "" "" get-all
              [ "$rc" -eq 2 ] || fail "C12: typo'd subcommand exited $rc, expected 2" ;;
        tor)  run_scenario "$d" "torbulkexitlist"
              [ "$rc" -ne 0 ] || fail "C12: Tor list 404 but fetch.sh exited 0" ;;
        cins) run_scenario "$d" "cinsscore"
              [ "$rc" -ne 0 ] || fail "C12: CINS 404 but fetch.sh exited 0" ;;
    esac
    [ "$(cat "$d/vpn_ranges.txt.tmp.99999" 2>/dev/null)" = "other run" ] \
        || fail "C12 ($mode): another run's vpn_ranges.txt.tmp.99999 was removed or changed"
    [ "$(cat "$d/.fetch.99999/aws.json" 2>/dev/null)" = "other aws" ] \
        || fail "C12 ($mode): another run's .fetch.99999/aws.json was removed or changed"
    [ "$(cat "$d/.aws.json" 2>/dev/null)" = "other aws" ] \
        || fail "C12 ($mode): shared .aws.json was removed or changed"
    [ "$mode" = typo ] || [ ! -e "$d/cloud_provider_ranges.jsonl" ] || \
        ! grep -q 'other aws' "$d/cloud_provider_ranges.jsonl" 2>/dev/null \
        || fail "C12 ($mode): another run's .aws.json was consumed into the output"
done
# Own cleanup still works: no .fetch.* dir of this run remains after a failing run.
ls -d "$work"/c12-cins/.fetch.* 2>/dev/null | grep -v '\.fetch\.99999$' | grep -q . \
    && fail "C12: this run's per-run directory was not removed"

# A directory left by a killed run with the same PID must not block this run.
d="$work/c13-stale"; mkdir -p "$d"; cp "$root/$src" "$d/fetch.sh"
rc=0
(cd "$d" && PATH="$stubs:$PATH" FAIL_URLS="" HTML_URLS="" PARTIAL_URLS="" \
    bash -c 'mkdir ".fetch.$$" && exec bash ./fetch.sh get_all' >"$d.out" 2>&1) || rc=$?
[ "$rc" -eq 0 ] || fail "C13: a stale .fetch.<own pid> made the run fail (rc=$rc): $(tail -n1 "$d.out")"
ls -d "$d"/.fetch.* >/dev/null 2>&1 && fail "C13: the stale per-run directory was left behind"
# Killed runs leave .fetch.<pid>/ behind; it must stay out of git.
grep -qxF '.fetch.*/' "$root/crates/ifconfig-rs/data/.gitignore" \
    || fail "C13: crates/ifconfig-rs/data/.gitignore does not ignore .fetch.*/"

[ "$fails" -eq 0 ] || exit 1
echo "PASS: test_fetch_fail_closed"
