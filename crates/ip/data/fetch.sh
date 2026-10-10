#!/usr/bin/env bash
# Fetch the data files ifconfig-rs loads at runtime. Ports the former Makefile:
# a file that already exists is not fetched again; `fetch.sh clean` removes them.
# Usage: fetch.sh [get_all|clean]   (run via `just ifconfig-data`)
set -euo pipefail
cd "$(dirname "$0")"

# Intermediates live in a per-run directory; cleanup removes only this run's directory and
# temp files: deleting another run's inputs or half-built files would break that run.
RUN=.fetch.$$
cleanup() {
    rm -rf -- "$RUN"
    rm -f -- ./*.tmp.$$ ./.*.tmp.$$
}
trap cleanup EXIT
# A directory with this PID can only be left by a killed earlier run: no live run shares it.
rm -rf -- "$RUN"
mkdir -- "$RUN"

# -L: an upstream that moves answers 3xx, whose body -f does not reject.
get() {
    curl -fsSL --max-redirs 5 "$1" -o "$2.tmp.$$" || return 1
    mv "$2.tmp.$$" "$2"
}

geoip_mmdbs() {
    geoipupdate -f .geoip.conf -d .
}

regexes_yaml() {
    [ -e regexes.yaml ] ||
        get https://raw.githubusercontent.com/ua-parser/uap-core/master/regexes.yaml regexes.yaml
}

tor_exit_nodes() {
    [ -e tor_exit_nodes.txt ] || get https://check.torproject.org/torbulkexitlist tor_exit_nodes.txt
}

feodo_botnet_ips() {
    [ -e feodo_botnet_ips.txt ] ||
        get https://feodotracker.abuse.ch/downloads/ipblocklist.txt feodo_botnet_ips.txt
}

vpn_ranges() {
    [ -e vpn_ranges.txt ] && return 0
    local out=vpn_ranges.txt.tmp.$$
    curl -fsSL --max-redirs 5 https://raw.githubusercontent.com/X4BNet/lists_vpn/main/output/vpn/ipv4.txt > $out
    curl -fsSL --max-redirs 5 https://raw.githubusercontent.com/X4BNet/lists_vpn/main/output/vpn/ipv6.txt >> $out
    mv $out vpn_ranges.txt
}

cloud_provider_ranges() {
    [ -e cloud_provider_ranges.jsonl ] && return 0
    get https://ip-ranges.amazonaws.com/ip-ranges.json "$RUN/aws.json"
    get https://www.gstatic.com/ipranges/cloud.json "$RUN/gcp.json"
    get https://www.cloudflare.com/ips-v4 "$RUN/cloudflare-v4.txt"
    get https://www.cloudflare.com/ips-v6 "$RUN/cloudflare-v6.txt"
    get https://docs.oracle.com/en-us/iaas/tools/public_ip_ranges.json "$RUN/oracle.json"
    get https://api.fastly.com/public-ip-list "$RUN/fastly.json"
    get https://www.digitalocean.com/geo/google.csv "$RUN/digitalocean.csv"
    get https://geoip.linode.com/ "$RUN/linode.csv"
    get https://api.github.com/meta "$RUN/github.json"
    get https://www.gstatic.com/ipranges/goog.json "$RUN/google-services.json"
    get https://cloud-ip-ranges.com/download/azure.txt "$RUN/azure.txt"

    local out=cloud_provider_ranges.jsonl.tmp.$$ cidr
    jq -c '.prefixes[] | {cidr: .ip_prefix, provider: "aws", service: .service, region: .region}' "$RUN/aws.json" > $out
    jq -c '.ipv6_prefixes[] | {cidr: .ipv6_prefix, provider: "aws", service: .service, region: .region}' "$RUN/aws.json" >> $out
    jq -c '.prefixes[] | {cidr: .ipv4Prefix, provider: "gcp", service: .service, region: .scope}' "$RUN/gcp.json" >> $out
    jq -c '.prefixes[] | select(.ipv6Prefix) | {cidr: .ipv6Prefix, provider: "gcp", service: .service, region: .scope}' "$RUN/gcp.json" >> $out
    while IFS= read -r cidr; do printf '{"cidr":"%s","provider":"cloudflare","service":null,"region":null}\n' "$cidr"; done < "$RUN/cloudflare-v4.txt" >> $out
    while IFS= read -r cidr; do printf '{"cidr":"%s","provider":"cloudflare","service":null,"region":null}\n' "$cidr"; done < "$RUN/cloudflare-v6.txt" >> $out
    jq -c '.regions[] | .region as $r | .cidrs[] | .cidr as $c | {cidr: $c, provider: "oracle", service: null, region: $r}' "$RUN/oracle.json" >> $out
    jq -c '.addresses[] | {cidr: ., provider: "fastly", service: null, region: null}' "$RUN/fastly.json" >> $out
    jq -c '.ipv6_addresses[] | {cidr: ., provider: "fastly", service: null, region: null}' "$RUN/fastly.json" >> $out
    awk -F, 'NR>1 && $1 ~ /\// {gsub(/^ +| +$/, "", $1); gsub(/^ +| +$/, "", $3); printf "{\"cidr\":\"%s\",\"provider\":\"digitalocean\",\"service\":null,\"region\":\"%s\"}\n", $1, $3}' "$RUN/digitalocean.csv" >> $out
    awk -F, 'NR>1 && $1 ~ /\// {gsub(/^ +| +$/, "", $1); gsub(/^ +| +$/, "", $3); printf "{\"cidr\":\"%s\",\"provider\":\"linode\",\"service\":null,\"region\":\"%s\"}\n", $1, $3}' "$RUN/linode.csv" >> $out
    jq -c 'to_entries[] | select(.value | type == "array") | .key as $svc | .value[] | select(type == "string" and contains("/")) | {cidr: ., provider: "github", service: $svc, region: null}' "$RUN/github.json" >> $out
    jq -c '.prefixes[] | if .ipv4Prefix then {cidr: .ipv4Prefix, provider: "google-services", service: null, region: null} elif .ipv6Prefix then {cidr: .ipv6Prefix, provider: "google-services", service: null, region: null} else empty end' "$RUN/google-services.json" >> $out
    while IFS= read -r cidr; do printf '{"cidr":"%s","provider":"azure","service":null,"region":null}\n' "$cidr"; done < "$RUN/azure.txt" >> $out
    mv $out cloud_provider_ranges.jsonl
}

datacenter_ranges() {
    [ -e datacenter_ranges.txt ] ||
        get https://raw.githubusercontent.com/X4BNet/lists_vpn/main/output/datacenter/ipv4.txt datacenter_ranges.txt
}

bot_ranges() {
    [ -e bot_ranges.jsonl ] && return 0
    get https://developers.google.com/static/crawling/ipranges/common-crawlers.json "$RUN/googlebot.json"
    get https://www.bing.com/toolbox/bingbot.json "$RUN/bingbot.json"
    get https://search.developer.apple.com/applebot.json "$RUN/applebot.json"
    get https://openai.com/gptbot.json "$RUN/gptbot.json"

    local out=bot_ranges.jsonl.tmp.$$
    jq -c '.prefixes[] | if .ipv4Prefix then {cidr: .ipv4Prefix, provider: "googlebot"} elif .ipv6Prefix then {cidr: .ipv6Prefix, provider: "googlebot"} else empty end' "$RUN/googlebot.json" > $out
    jq -c '.prefixes[] | if .ipv4Prefix then {cidr: .ipv4Prefix, provider: "bingbot"} elif .ipv6Prefix then {cidr: .ipv6Prefix, provider: "bingbot"} else empty end' "$RUN/bingbot.json" >> $out
    jq -c '.prefixes[] | if .ipv4Prefix then {cidr: .ipv4Prefix, provider: "applebot"} elif .ipv6Prefix then {cidr: .ipv6Prefix, provider: "applebot"} else empty end' "$RUN/applebot.json" >> $out
    jq -c '.prefixes[] | if .ipv4Prefix then {cidr: .ipv4Prefix, provider: "gptbot"} elif .ipv6Prefix then {cidr: .ipv6Prefix, provider: "gptbot"} else empty end' "$RUN/gptbot.json" >> $out
    mv $out bot_ranges.jsonl
}

spamhaus_drop() {
    [ -e spamhaus_drop.txt ] && return 0
    local out=spamhaus_drop.txt.tmp.$$
    { curl -fsSL --max-redirs 5 https://www.spamhaus.org/drop/drop.txt;
      curl -fsSL --max-redirs 5 https://www.spamhaus.org/drop/edrop.txt;
      curl -fsSL --max-redirs 5 https://www.spamhaus.org/drop/dropv6.txt; } \
    | sed -e 's/ ;.*//' -e '/^;/d' -e '/^$/d' > $out
    mv $out spamhaus_drop.txt
}

cins_army_ips() {
    [ -e cins_army_ips.txt ] && return 0
    local out=cins_army_ips.txt.tmp.$$
    curl -fsSL --max-redirs 5 https://cinsscore.com/list/ci-badguys.txt | grep -v '^#' | grep -v '^$' > $out
    mv $out cins_army_ips.txt
}

as_metadata() {
    [ -e as_metadata.jsonl ] && return 0
    get https://raw.githubusercontent.com/ipverse/as-metadata/master/as.json "$RUN/as_metadata.json"
    jq -c 'to_entries[] | select(.value.asn != null) | {asn: .value.asn, category: (.value.metadata.category // ""), network_role: (.value.metadata.networkRole // ""), registered: ((.value.metadata.registered // "")[:10])}' "$RUN/as_metadata.json" > as_metadata.jsonl.tmp.$$
    mv as_metadata.jsonl.tmp.$$ as_metadata.jsonl
}

get_all() {
    geoip_mmdbs
    regexes_yaml
    tor_exit_nodes
    feodo_botnet_ips
    vpn_ranges
    cloud_provider_ranges
    datacenter_ranges
    bot_ranges
    spamhaus_drop
    cins_army_ips
    as_metadata
}

clean() {
    rm -f regexes.yaml tor_exit_nodes.txt feodo_botnet_ips.txt vpn_ranges.txt datacenter_ranges.txt \
        spamhaus_drop.txt cins_army_ips.txt as_metadata.jsonl .as_metadata.json *.mmdb \
        cloud_provider_ranges.jsonl .aws.json .gcp.json .cloudflare-v4.txt .cloudflare-v6.txt \
        .oracle.json .fastly.json .digitalocean.csv .linode.csv .github.json .google-services.json .azure.txt \
        bot_ranges.jsonl .googlebot.json .bingbot.json .applebot.json .gptbot.txt
}

case "${1:-get_all}" in
    get_all) get_all ;;
    clean) clean ;;
    *) echo "usage: fetch.sh [get_all|clean]" >&2; exit 2 ;;
esac
