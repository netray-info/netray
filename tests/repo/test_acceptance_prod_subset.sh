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

allowed="smoke/health.spec.ts smoke/ready.spec.ts smoke/security-headers.spec.ts smoke/tls-certs.spec.ts static-site/assets.spec.ts api-contract/meta-shape.spec.ts api-contract/openapi.spec.ts"
forbidden="smoke/redirects.spec.ts api-contract/error-format.spec.ts static-site/links.spec.ts static-site/pages.spec.ts services/ integration/"

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

# `just acceptance` (production) runs only the prod project.
recipe=$(just -n acceptance 2>&1) || fail "just -n acceptance failed"
case "$recipe" in *"--project prod"*) ;; *) fail "just acceptance does not run --project prod" ;; esac

[ "$fails" -eq 0 ] || exit 1
echo "PASS: test_acceptance_prod_subset"
