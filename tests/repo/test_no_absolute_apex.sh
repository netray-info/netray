#!/usr/bin/env bash
# No tracked file of netray-web's pages names the apex or a netray.info subdomain as a literal
# URL (spec v2-site-nav, requirement 3): in-page links are apex-relative and every absolute URL is
# built from the one origin, `pages::ORIGIN`, whose definition is the only exempt line.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fail() { echo "FAIL: $1"; exit 1; }

origin_line='pub(crate) const ORIGIN: &str = "https://netray.info";'
hits=$(git grep -nE 'https://([a-z0-9-]+\.)?netray\.info' -- \
  crates/web/templates crates/web/src crates/web/islands/src 2>/dev/null \
  | grep -vxF "crates/web/src/pages/mod.rs:$(git grep -nF "$origin_line" -- crates/web/src/pages/mod.rs | cut -d: -f2):$origin_line")

[ -z "$hits" ] || fail "absolute netray.info URLs (build them from pages::ORIGIN or make them relative):
$hits"

echo "PASS: no absolute netray.info URLs in netray-web's pages"
