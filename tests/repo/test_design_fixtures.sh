#!/usr/bin/env bash
# v2-result-polish R15: every acceptance history fixture uses example.com or a subdomain, and the
# long-domain fixture (checkout.eu-central.storefront.example.com) holds one run at age 0 with four
# TLS addresses (two IPv4, two IPv6, all ok), a chain of three per address ending in a self-signed
# certificate, a handshake per address, and evidence references that all resolve to a block.
set -u
source "$(dirname "$0")/lib/netray.sh"

trap '_netray_cleanup' EXIT

dir="$REPO_ROOT/tests/acceptance/fixtures/history"
long="checkout.eu-central.storefront.example.com"

failures=0
fail() { echo "FAIL: $*" >&2; failures=$((failures + 1)); }

out=$(python3 -I - "$dir" "$long" <<'PY'
import glob, ipaddress, json, sys

d, long = sys.argv[1], sys.argv[2]
errs = []
fixtures = {}
for path in sorted(glob.glob(f"{d}/*.json")):
    fx = json.load(open(path))
    fixtures[path] = fx
    dom = fx.get("domain", "")
    if dom != "example.com" and not dom.endswith(".example.com"):
        errs.append(f"{path}: domain {dom!r} is not example.com or a subdomain")

mine = [(p, f) for p, f in fixtures.items() if f.get("domain") == long]
if len(mine) != 1:
    errs.append(f"want exactly one fixture for {long}, found {len(mine)}")
else:
    path, fx = mine[0]
    if fx.get("age_seconds") != 0:
        errs.append(f"{long}: age_seconds {fx.get('age_seconds')!r}, want 0")
    run = fx["run"]
    blocks = {b["id"]: b for b in run["blocks"]}
    t = blocks.get("tls.addresses")
    rows = t["shape"]["data"]["rows"] if t else []
    if len(rows) != 4:
        errs.append(f"tls.addresses has {len(rows)} rows, want 4")
    v4 = [ipaddress.ip_address(r[0]["v"]) for r in rows]
    n4 = [a for a in v4 if a.version == 4]
    n6 = [a for a in v4 if a.version == 6]
    if len(n4) != 2 or len(n6) != 2:
        errs.append(f"want two IPv4 and two IPv6, got {len(n4)} and {len(n6)}")
    for a in n4:
        if not (a in ipaddress.ip_network("192.0.2.0/24") or a in ipaddress.ip_network("198.51.100.0/24")):
            errs.append(f"IPv4 {a} outside the documentation ranges")
    for a in n6:
        if a not in ipaddress.ip_network("2001:db8::/32"):
            errs.append(f"IPv6 {a} outside 2001:db8::/32")
    for r in rows:
        if r[2]["v"] != "ok":
            errs.append(f"address {r[0]['v']} outcome {r[2]['v']!r}, want ok")
        # Block qualifiers bracket an IPv6 address (SDD 3.7: `<module>.<name>@[<v6>]:<port>`).
        addr = r[0]['v']
        key = f"[{addr}]:443" if ":" in addr else f"{addr}:443"
        ch = blocks.get(f"tls.chain@{key}")
        if ch is None:
            errs.append(f"missing block tls.chain@{key}")
        else:
            items = ch["shape"]["data"]
            if len(items) != 3:
                errs.append(f"tls.chain@{key} has {len(items)} items, want 3")
            else:
                last = [v for k, v in items[-1] if k == "self_signed"]
                if not last or last[0].get("v") is not True:
                    errs.append(f"tls.chain@{key}: last item is not self_signed true")
        if f"tls.handshake@{key}" not in blocks:
            errs.append(f"missing block tls.handshake@{key}")
    for c in run["checks"]:
        for ref in c.get("evidence", []):
            if ref not in blocks:
                errs.append(f"check {c['id']}: evidence {ref!r} has no block")
print("\n".join(errs))
PY
) || fail "fixture reader failed: $out"

while IFS= read -r line; do
    [ -n "$line" ] && fail "$line"
done <<<"$out"

[ "$failures" -eq 0 ] || { echo "$failures failure(s)" >&2; exit 1; }
echo "ok: design fixtures"
