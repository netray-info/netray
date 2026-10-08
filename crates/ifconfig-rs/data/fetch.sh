#!/usr/bin/env bash
# Fetch the data files ifconfig-rs loads at runtime. Ports the former Makefile:
# a file that already exists is not fetched again; `fetch.sh clean` removes them.
# Usage: fetch.sh [get_all|clean]   (run via `just ifconfig-data`)
set -euo pipefail
cd "$(dirname "$0")"

# Only this run's temp files: deleting another run's half-built file would let that run
# rename a partial file into place.
cleanup() {
    rm -f -- ./*.tmp.$$ ./.*.tmp.$$ \
        .aws.json .gcp.json .cloudflare-v4.txt .cloudflare-v6.txt .oracle.json .fastly.json \
        .digitalocean.csv .linode.csv .github.json .google-services.json .azure.txt \
        .googlebot.json .bingbot.json .applebot.json .gptbot.txt .as_metadata.json
}
trap cleanup EXIT

get() {
    curl -fsS "$1" -o "$2.tmp.$$" || return 1
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
    curl -fsS https://raw.githubusercontent.com/X4BNet/lists_vpn/main/output/vpn/ipv4.txt > $out
    curl -fsS https://raw.githubusercontent.com/X4BNet/lists_vpn/main/output/vpn/ipv6.txt >> $out
    mv $out vpn_ranges.txt
}

cloud_provider_ranges() {
    [ -e cloud_provider_ranges.jsonl ] && return 0
    [ -e .aws.json ] || get https://ip-ranges.amazonaws.com/ip-ranges.json .aws.json
    [ -e .gcp.json ] || get https://www.gstatic.com/ipranges/cloud.json .gcp.json
    [ -e .cloudflare-v4.txt ] || get https://www.cloudflare.com/ips-v4 .cloudflare-v4.txt
    [ -e .cloudflare-v6.txt ] || get https://www.cloudflare.com/ips-v6 .cloudflare-v6.txt
    [ -e .oracle.json ] || get https://docs.oracle.com/en-us/iaas/tools/public_ip_ranges.json .oracle.json
    [ -e .fastly.json ] || get https://api.fastly.com/public-ip-list .fastly.json
    [ -e .digitalocean.csv ] || get https://www.digitalocean.com/geo/google.csv .digitalocean.csv
    [ -e .linode.csv ] || get https://geoip.linode.com/ .linode.csv
    [ -e .github.json ] || get https://api.github.com/meta .github.json
    [ -e .google-services.json ] || get https://www.gstatic.com/ipranges/goog.json .google-services.json
    [ -e .azure.txt ] || get https://cloud-ip-ranges.com/download/azure.txt .azure.txt

    local out=cloud_provider_ranges.jsonl.tmp.$$ cidr
    jq -c '.prefixes[] | {cidr: .ip_prefix, provider: "aws", service: .service, region: .region}' .aws.json > $out
    jq -c '.ipv6_prefixes[] | {cidr: .ipv6_prefix, provider: "aws", service: .service, region: .region}' .aws.json >> $out
    jq -c '.prefixes[] | {cidr: .ipv4Prefix, provider: "gcp", service: .service, region: .scope}' .gcp.json >> $out
    jq -c '.prefixes[] | select(.ipv6Prefix) | {cidr: .ipv6Prefix, provider: "gcp", service: .service, region: .scope}' .gcp.json >> $out
    while IFS= read -r cidr; do printf '{"cidr":"%s","provider":"cloudflare","service":null,"region":null}\n' "$cidr"; done < .cloudflare-v4.txt >> $out
    while IFS= read -r cidr; do printf '{"cidr":"%s","provider":"cloudflare","service":null,"region":null}\n' "$cidr"; done < .cloudflare-v6.txt >> $out
    jq -c '.regions[] | .region as $r | .cidrs[] | .cidr as $c | {cidr: $c, provider: "oracle", service: null, region: $r}' .oracle.json >> $out
    jq -c '.addresses[] | {cidr: ., provider: "fastly", service: null, region: null}' .fastly.json >> $out
    jq -c '.ipv6_addresses[] | {cidr: ., provider: "fastly", service: null, region: null}' .fastly.json >> $out
    awk -F, 'NR>1 && $1 ~ /\// {gsub(/^ +| +$/, "", $1); gsub(/^ +| +$/, "", $3); printf "{\"cidr\":\"%s\",\"provider\":\"digitalocean\",\"service\":null,\"region\":\"%s\"}\n", $1, $3}' .digitalocean.csv >> $out
    awk -F, 'NR>1 && $1 ~ /\// {gsub(/^ +| +$/, "", $1); gsub(/^ +| +$/, "", $3); printf "{\"cidr\":\"%s\",\"provider\":\"linode\",\"service\":null,\"region\":\"%s\"}\n", $1, $3}' .linode.csv >> $out
    jq -c 'to_entries[] | select(.value | type == "array") | .key as $svc | .value[] | select(type == "string" and contains("/")) | {cidr: ., provider: "github", service: $svc, region: null}' .github.json >> $out
    jq -c '.prefixes[] | if .ipv4Prefix then {cidr: .ipv4Prefix, provider: "google-services", service: null, region: null} elif .ipv6Prefix then {cidr: .ipv6Prefix, provider: "google-services", service: null, region: null} else empty end' .google-services.json >> $out
    while IFS= read -r cidr; do printf '{"cidr":"%s","provider":"azure","service":null,"region":null}\n' "$cidr"; done < .azure.txt >> $out
    mv $out cloud_provider_ranges.jsonl
    rm -f .aws.json .gcp.json .cloudflare-v4.txt .cloudflare-v6.txt .oracle.json .fastly.json .digitalocean.csv .linode.csv .github.json .google-services.json .azure.txt
}

datacenter_ranges() {
    [ -e datacenter_ranges.txt ] ||
        get https://raw.githubusercontent.com/X4BNet/lists_vpn/main/output/datacenter/ipv4.txt datacenter_ranges.txt
}

bot_ranges() {
    [ -e bot_ranges.jsonl ] && return 0
    [ -e .googlebot.json ] || get https://developers.google.com/search/apis/ipranges/googlebot.json .googlebot.json
    [ -e .bingbot.json ] || get https://www.bing.com/toolbox/bingbot.json .bingbot.json
    [ -e .applebot.json ] || get https://search.developer.apple.com/applebot.json .applebot.json
    [ -e .gptbot.txt ] || get https://openai.com/gptbot-ranges.txt .gptbot.txt

    local out=bot_ranges.jsonl.tmp.$$ cidr
    jq -c '.prefixes[] | if .ipv4Prefix then {cidr: .ipv4Prefix, provider: "googlebot"} elif .ipv6Prefix then {cidr: .ipv6Prefix, provider: "googlebot"} else empty end' .googlebot.json > $out
    jq -c '.prefixes[] | if .ipv4Prefix then {cidr: .ipv4Prefix, provider: "bingbot"} elif .ipv6Prefix then {cidr: .ipv6Prefix, provider: "bingbot"} else empty end' .bingbot.json >> $out
    jq -c '.prefixes[] | if .ipv4Prefix then {cidr: .ipv4Prefix, provider: "applebot"} elif .ipv6Prefix then {cidr: .ipv6Prefix, provider: "applebot"} else empty end' .applebot.json >> $out
    while IFS= read -r cidr; do case "$cidr" in \#*|"") continue;; esac; printf '{"cidr":"%s","provider":"gptbot"}\n' "$cidr"; done < .gptbot.txt >> $out
    mv $out bot_ranges.jsonl
    rm -f .googlebot.json .bingbot.json .applebot.json .gptbot.txt
}

spamhaus_drop() {
    [ -e spamhaus_drop.txt ] && return 0
    local out=spamhaus_drop.txt.tmp.$$
    { curl -fsS https://www.spamhaus.org/drop/drop.txt;
      curl -fsS https://www.spamhaus.org/drop/edrop.txt;
      curl -fsS https://www.spamhaus.org/drop/dropv6.txt; } \
    | sed -e 's/ ;.*//' -e '/^;/d' -e '/^$/d' > $out
    mv $out spamhaus_drop.txt
}

cins_army_ips() {
    [ -e cins_army_ips.txt ] && return 0
    local out=cins_army_ips.txt.tmp.$$
    curl -fsS https://cinsscore.com/list/ci-badguys.txt | grep -v '^#' | grep -v '^$' > $out
    mv $out cins_army_ips.txt
}

as_metadata() {
    [ -e as_metadata.jsonl ] && return 0
    [ -e .as_metadata.json ] || get https://raw.githubusercontent.com/ipverse/as-metadata/master/as.json .as_metadata.json
    jq -c 'to_entries[] | select(.value.asn != null) | {asn: .value.asn, category: (.value.metadata.category // ""), network_role: (.value.metadata.networkRole // ""), registered: ((.value.metadata.registered // "")[:10])}' .as_metadata.json > as_metadata.jsonl.tmp.$$
    mv as_metadata.jsonl.tmp.$$ as_metadata.jsonl
    rm -f .as_metadata.json
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
