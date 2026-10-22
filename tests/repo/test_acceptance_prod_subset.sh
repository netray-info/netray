#!/usr/bin/env bash
# Production only ever gets the narrow acceptance subset (operator decision 2026-10-08):
# the full suite probes unknown paths and crawls the sitemap, which fail2ban's botsearch
# jail on the production host bans. `just acceptance` runs the Playwright project `prod`,
# whose specs hit only known, existing paths.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

cfg=tests/acceptance/playwright.config.ts
[ -f "$cfg" ] || { echo "FAIL: $cfg missing"; exit 1; }

allowed="agreement/prod.spec.ts smoke/health.spec.ts smoke/ready.spec.ts smoke/security-headers.spec.ts smoke/tls-certs.spec.ts"
forbidden="smoke/redirects.spec.ts api-contract/error-format.spec.ts static-site/ api-contract/meta-shape.spec.ts api-contract/openapi.spec.ts services/ integration/"

# The prod project block: from `name: 'prod'` to the end of its object literal.
block=$(awk "/name: 'prod'/{p=1} p{print} p && /}/{exit}" "$cfg")
[ -n "$block" ] || fail "playwright.config.ts has no project named 'prod'"
case "$block" in *dependencies*) fail "the prod project has dependencies (they would pull in the full suite)" ;; esac
for s in $allowed; do
    case "$block" in *"$s"*) ;; *) fail "prod project does not include $s" ;; esac
done
for s in $forbidden; do
    case "$block" in *"$s"*) fail "prod project includes $s, which probes unknown paths or crawls" ;; esac
done
# Exactly the allowed set: no more, no fewer.
listed=$(printf '%s\n' "$block" | grep -oE "'[^']+\.spec\.ts'" | tr -d "'" | sort)
want=$(printf '%s\n' $allowed | sort)
[ "$listed" = "$want" ] || fail "prod project lists [$(echo $listed)], want exactly [$(echo $want)]"

# The agreement spec: tracked, request contexts only, known domains only, one runs route.
tracked=$(git ls-files)
spec=tests/acceptance/agreement/prod.spec.ts
if ! printf '%s\n' "$tracked" | grep -qx "$spec"; then
    fail "$spec is not tracked"
else
    if grep -nE 'page\.|browser|newPage|\{ *page\b' "$spec" >/dev/null; then
        fail "$spec uses a browser page; request contexts only"
    fi
    for d in $(grep -oE '[a-z0-9-]+(\.[a-z0-9-]+)*\.(info|com|org|net|io|de|dev)' "$spec" | sort -u); do
        case "$d" in netray.info|dns.netray.info|tls.netray.info|http.netray.info|ip.netray.info|example.com) ;; *) fail "$spec names domain $d" ;; esac
    done
    grep -qE '/api/v1/domains/[^/]+/runs' "$spec" || fail "$spec never posts to /api/v1/domains/{domain}/runs"
    if grep -E '\.post\(' "$spec" | grep -vE '/api/v1/domains/.*/runs' | grep -q .; then
        fail "$spec posts somewhere other than /api/v1/domains/{domain}/runs"
    fi
fi

# Requirement 22: V1 acceptance leftovers are gone.
env=tests/acceptance/fixtures/env.ts
grep -qE '\blens\b' "$env" && fail "$env still has a lens origin"
for gone in tests/acceptance/services/ tests/acceptance/integration/cross-links.spec.ts tests/acceptance/static-site/; do
    printf '%s\n' "$tracked" | grep -q "^$gone" && fail "$gone is still tracked"
done
for f in $(printf '%s\n' "$tracked" | grep -E '^tests/acceptance/.*\.spec\.ts$'); do
    [ -f "$f" ] && grep -q '/api/meta' "$f" && fail "$f requests /api/meta (V1)"
done

# `just acceptance` (production) runs only the prod project.
recipe=$(just -n acceptance 2>&1) || fail "just -n acceptance failed"
case "$recipe" in *"--project prod"*) ;; *) fail "just acceptance does not run --project prod" ;; esac

[ "$fails" -eq 0 ] || exit 1
echo "PASS: test_acceptance_prod_subset"
