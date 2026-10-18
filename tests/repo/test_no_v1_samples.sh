#!/usr/bin/env bash
# No template under crates/web/templates/ names a V1 API sample: a V1 subdomain API path,
# a cut ip. route (requirement 14), the apex V1 API, or ?d= with dkim_selectors
# (v2-cutover C19, C20). `curl ip.netray.info` (the root) stays valid.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fail() { echo "FAIL: $1"; exit 1; }

cut='json|yaml|toml|csv|all|ip|ipv4|ipv6|range|diff|batch|asn|meta|tcp|location|user_agent|network|host|isp|country|city|timezone|latitude|longitude|region|headers'
sub='(dns|tls|http|email|ip|lens)'
pattern="${sub}\\.netray\\.info/(api/|api-docs/)"
pattern+="|email\\.netray\\.info/inspect"
pattern+="|ip\\.netray\\.info/(${cut})([^[:alnum:]_-]|\$)"
pattern+="|(^|[^.[:alnum:]-])netray\\.info/api/(check|meta)"
pattern+="|\\?d=.*dkim_selectors|dkim_selectors.*\\?d="

# matcher: stdin lines -> matching lines with numbers
v1_sample_lines() { grep -nE -- "$pattern"; }

# self-test on scratch input
good=$'curl -s https://ip.netray.info\ncurl ip.netray.info/\ncurl https://netray.info/api/v1/dns/example.com\nhttps://netray.info/?d=example.com'
[ -z "$(printf '%s\n' "$good" | v1_sample_lines)" ] || fail "matcher flags a good line"
bad=(
  'https://dns.netray.info/api/lookup'
  'https://lens.netray.info/api-docs/openapi.json'
  'https://email.netray.info/inspect/example.com'
  'curl https://ip.netray.info/json'
  'curl ip.netray.info/ipv4'
  'curl ip.netray.info/user_agent'
  'https://netray.info/api/check?d=example.com'
  'https://netray.info/api/meta'
  'https://netray.info/?d=example.com&dkim_selectors=a,b'
)
for b in "${bad[@]}"; do
  [ -n "$(printf '%s\n' "$b" | v1_sample_lines)" ] || fail "matcher misses: $b"
done

files=$(git ls-files crates/web/templates/)
[ -n "$files" ] || fail "no tracked files under crates/web/templates/"

hits=""
while IFS= read -r f; do
  [ -f "$f" ] || continue
  out=$(v1_sample_lines < "$f" | sed "s|^|$f:|")
  [ -z "$out" ] || hits+="$out"$'\n'
done <<< "$files"

if [ -n "$hits" ]; then
  printf '%s' "$hits"
  fail "V1 API samples in templates (rewrite to /api/v1/... or drop dkim_selectors)"
fi

echo "PASS: no V1 samples in templates"
