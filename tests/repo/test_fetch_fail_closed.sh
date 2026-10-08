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
f=0 out="" url=""
while [ $# -gt 0 ]; do
    case "$1" in
        -o) out=$2; shift ;;
        --*) ;;
        -*) case "$1" in *f*) f=1 ;; esac ;;
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
    *googlebot.json|*bingbot.json|*applebot.json) echo "$prefixes" ;;
    *gptbot-ranges.txt) printf '# gptbot\n192.0.2.0/24\n' ;;
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

# run_scenario DIR FAIL_URLS -> exit code in $rc
run_scenario() {
    mkdir -p "$1"
    cp "$root/$src" "$1/fetch.sh"
    rc=0
    (cd "$1" && PATH="$stubs:$PATH" FAIL_URLS="$2" bash ./fetch.sh get_all >"$1.out" 2>&1) || rc=$?
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
l=$(leftovers "$d"); [ -z "$l" ] || fail "C8: leftovers after a successful run: $(echo $l)"

# C9: every curl invocation carries -f (alone or in a flag cluster).
n=0
while IFS= read -r line; do
    n=$((n + 1))
    printf '%s\n' "$line" | grep -qE 'curl +(-[A-Za-z]*f[A-Za-z]*|.* -[A-Za-z]*f[A-Za-z]*)( |$)' \
        || fail "C9: curl without -f: $(echo $line)"
done < <(grep -E '(^|[^A-Za-z_])curl( |$)' "$src" | grep -vE '^[[:space:]]*#')
[ "$n" -gt 0 ] || fail "C9: no curl invocation found in $src"

# C2 under concurrency: a run's cleanup removes only its own temp files (*.tmp.$$);
# deleting another run's half-built temp file lets that run rename a partial file into place.
if grep -E 'rm .*\*\.tmp\.\*' "$src" >/dev/null; then
    fail "C2: cleanup removes every run's temp files (*.tmp.*), not only this run's (*.tmp.\$\$)"
fi
grep -E 'rm .*\.tmp\.\$\$' "$src" >/dev/null || fail "C2: cleanup does not remove this run's temp files (*.tmp.\$\$)"

[ "$fails" -eq 0 ] || exit 1
echo "PASS: test_fetch_fail_closed"
