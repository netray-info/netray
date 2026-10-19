# Report: v2-dkim-legacy-urls

## Phase 1 — Old site URLs answer 301

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R9: `/guide/<slug>.html` 301 to `/guide/<slug>`, index.html to `/guide/`, lens and lens.html to `/guide/scoring`, unknown stem 404 | green | crates/web/tests/it/pages_legacy_urls.rs |
| C2 | R10: `/compare/` to `/compare`, `/status/` to `/status`, `/tools` and `/tools/` to `/` | green | crates/web/tests/it/pages_legacy_urls.rs |
| C3 | GET /guide/asn.html: 301 Location /guide/asn | green | crates/web/tests/it/pages_legacy_urls.rs |
| C4 | every GUIDES slug: /guide/<slug>.html 301 to /guide/<slug> | green | crates/web/tests/it/pages_legacy_urls.rs |
| C5 | GET /guide/index.html: 301 to /guide/ | green | crates/web/tests/it/pages_legacy_urls.rs |
| C6 | GET /guide/lens.html and /guide/lens: 301 to /guide/scoring | green | crates/web/tests/it/pages_legacy_urls.rs |
| C7 | GET /guide/nonesuch.html: 404 | already_implemented | crates/web/tests/it/pages_legacy_urls.rs |
| C8 | GET /compare/ 301 /compare; GET /status/ 301 /status | green | crates/web/tests/it/pages_legacy_urls.rs |
| C9 | GET /tools and /tools/: 301 / | green | crates/web/tests/it/pages_legacy_urls.rs |
| C10 | GET /guide/asn: 200 as before | already_implemented | crates/web/tests/it/pages_legacy_urls.rs |

C7 and C10 passed before the change: a 404 for an unknown stem and 200 for a V2 slug already held; the tests stay to protect them.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 guides.rs, pages/mod.rs | 1 | implementer (no coder spawned: two-file change) | 0 | 0 |

### Review

Reader: no findings on the code; one NIT in the SDD row (the V1 sitemap lists `/tools/`, not `/tools`), fixed in the row's wording.

### Behavioural verification

`netray lens lens.dev.toml` on 127.0.0.1:18791, `curl -s -o /dev/null -w '%{http_code} %header{location}'`:

```
/guide/asn.html 301 /guide/asn
/guide/lens.html 301 /guide/scoring
/guide/index.html 301 /guide/
/guide/nonesuch.html 404
/guide/asn 200
/compare/ 301 /compare
/status/ 301 /status
/tools 301 /
/tools/ 301 /
```
