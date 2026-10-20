# Report: v2-site-nav

## Phase 1 — Origin and relative links

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 every absolute URL a page or route writes is built from pages::ORIGIN | green | crates/web/tests/it/site_nav_links.rs |
| C2 | R2 links and form actions between pages are apex-relative; inspectors at /inspect/tool, email CTAs to /inspect/email with d | green | crates/web/tests/it/site_nav_links.rs |
| C3 | R3 tests/repo/test_no_absolute_apex.sh fails on a netray.info literal in web templates, src or islands/src except the ORIGIN line | green | tests/repo/test_no_absolute_apex_selftest.sh |
| C4 | R4 the sitemap lists home, guides, compare, status, privacy, tools, API docs and the five inspector pages, no subdomain URL | green | crates/web/tests/it/site_nav_links.rs |
| C5 | S /guide/csp canonical is the origin path, CTA action /inspect/http, no subdomain link | green | crates/web/tests/it/site_nav_links.rs |
| C6 | S sitemap and robots locs start with the origin, sitemap lists /tools, /api/v1/docs, /inspect/email | green | crates/web/tests/it/site_nav_links.rs |
| C7 | S /guide/scoring table links /inspect/ip, /inspect/dns, /inspect/tls, /inspect/http | green | crates/web/tests/it/site_nav_links.rs |
| C8 | S /guide/dkim CTA action /inspect/email with the field d | green | crates/web/tests/it/site_nav_links.rs |
| C9 | S /guide/ links each guide as /guide/slug | green | crates/web/tests/it/site_nav_links.rs |
| C10 | S the repo check fails on a tracked template with https://dns.netray.info and passes on the tree | green | tests/repo/test_no_absolute_apex_selftest.sh |

Changed protected tests: `pages_sitemap.rs` (the sitemap now lists `/tools`, `/api/v1/docs` and the inspector pages instead of the subdomains), `pages_guides.rs` (guide index links are relative), `guide_content.rs` (V1 subdomain CTAs map to `/inspect/<tool>`, email field `d`), `site_nav_links.rs` (the apex-href scans skip the canonical link, which is absolute by requirement 1).

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 2 | orchestrator | 0 | 0 |
| G2 | 1 | orchestrator | 0 | 0 |

### Review

| class | where | finding | disposition |
|---|---|---|---|
| BLOCKER | crates/web/templates/pages/entry.html:11 | `/r/<id>` og:image and og:url relative, no canonical | fixed: canonical, og:image and og:url from the origin; `entries_page.rs`, `fidelity_entry_frame.rs` assert the absolute URLs |
| AMENDMENT | crates/lens/src/lib.rs:423 | the binary serves lens's own robots without the Sitemap line | repaired in phase: lens serves `pages::robots_body()`; `pages_mount.rs` pins the new body |
| AMENDMENT | crates/web/templates/pages/guides/vpn-leak.html:51 | a blank IP CTA lands on an idle inspector | repaired in phase: `/inspect/ip?ip=` (key present, blank) autostarts the own-address lookup |
| DEFERRED | domain-security-audit.html:15 | runs snippets read top-level `grade`; the answer nests `run` | fixed here: `.run | {…}` and `.run.grade`; `guide_content.rs` asserts `.run.grade` |
| DEFERRED | crates/web/src/pages/mod.rs:75 | sitemap lists `/tools` while it still answers 301 | Phase 2 requirement 7 |
| NIT | tests/repo/test_no_absolute_apex.sh:13 | the exemption matched the ORIGIN text in any file | fixed: exempt only that line of `crates/web/src/pages/mod.rs` |

### Behavioural verification

skipped: the routes are exercised through the routers in `site_nav_links.rs`; the live stack runs in Phase 7 (`just design-check`), whose screenshots cover `/guide/csp`.
