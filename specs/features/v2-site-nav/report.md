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

## Phase 2 — Header menu, phone sheet, tools page, privacy, taglines

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R5 header nav: details Inspectors with the five inspector links, descriptions and All tools, then Guides, API, Status; current marking; no JavaScript needed | green | crates/web/tests/it/site_nav_header.rs |
| C2 | R6 phone sheet: label, five links, All tools, Guides, API, Status, rows at least 44 px | green | crates/web/tests/it/site_nav_header.rs |
| C3 | R7 GET /tools renders the inspector index; /tools/ answers 301 to /tools | green | crates/web/tests/it/site_nav_header.rs |
| C4 | R8 /privacy renders in the site frame titled Privacy · netray | green | crates/web/tests/it/site_nav_header.rs |
| C5 | R9 inspector taglines DNS, resolved; TLS, illuminated; HTTP, inspected; Email, verified; IP, located | green | crates/web/tests/it/site_nav_header.rs |
| C6 | S any page: header nav entries in order, panel links and All tools | green | crates/web/tests/it/site_nav_header.rs |
| C7 | S /inspect/tls: summary marked current, TLS link aria-current | green | crates/web/tests/it/site_nav_header.rs |
| C8 | S /tools: summary marked current | green | crates/web/tests/it/site_nav_header.rs |
| C9 | S stylesheet: menu panel, links, labels, descriptions, All tools at board values; phone rows at least 44 px | green | crates/web/tests/it/site_nav_header.rs |
| C10 | S menu island closes the details on Escape and on a click outside | green | crates/web/islands/test/menu.test.ts |
| C11 | S GET /tools: 200, h1 Inspectors, five cards with tagline, quick form, Open link; Featured guides; API docs link | green | crates/web/tests/it/site_nav_header.rs |
| C12 | S GET /tools/ answers 301 to /tools | green | crates/web/tests/it/site_nav_header.rs |
| C13 | S /privacy carries header, footer, /static/app.css and the title | green | crates/web/tests/it/site_nav_header.rs |
| C14 | S /inspect/email tagline Email, verified | green | crates/web/tests/it/site_nav_header.rs |

Changed protected tests: `pages_legacy_urls.rs` (`/tools/` moves to `/tools`, `/tools` is a page), `entries_frame.rs` (privacy renders in the site frame, no longer self-contained), `pages_frame.rs`, `fidelity_header.rs`, `fidelity_phone_header.rs`, `fidelity_review_p6.rs`, `shell_header.rs` (the Inspectors entry is the menu's summary; the nav carries the `nav-menu` island; the nested details closes before the page menu's), `site_nav_header.rs` (featured guides are a subset of the guide list; the island is `nav-menu` because the gate reads `menu` as a CSS system colour; the chevron is a span), `menu.test.ts` (island name).

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 header menu, sheet, island | 3 | orchestrator | 0 | 0 |
| G2 tools page, redirect | 2 | orchestrator | 0 | 0 |
| G3 privacy frame, tagline | 1 | orchestrator | 0 | 0 |

### Review

| class | where | finding | disposition |
|---|---|---|---|
| BLOCKER | crates/web/src/pages/mod.rs:145 | unused `Frame::version` fails clippy | removed (the footer reads a constant, Phase 3) |
| AMENDMENT | crates/web/static/pages.css:803 | sheet items centred by `.page-nav a` | repaired: `align-items: stretch` |
| AMENDMENT | crates/web/static/pages.css:820 | no fallback without `::details-content` | repaired: the label stays a disclosure under `@supports not selector(::details-content)` |
| AMENDMENT | crates/web/static/pages.css:126 | panel has no shadow; no shadow token, gate refuses an rgba literal | decision log: the panel keeps border and surface, no shadow (not a measured class) |
| AMENDMENT | crates/web/src/entries/routes.rs:73 | /privacy (and /r/<id>) served without the pages' CSP | repaired: the entries pages go through `pages::headers::secure` |
| DEFERRED | crates/web/static/pages.css:142 | sheet labels 11 px (phone label token) against board M's 12 px | Phase 7 measures it |
| NIT | crates/web/islands/src/menu.ts:10 | Escape stole focus from a field | fixed: focus returns to the summary only from the menu or the body |
| NIT | crates/web/templates/base.html:24 | at phone width the summary toggles with no visible change | kept: harmless, the links stay reachable |

### Behavioural verification

skipped: router-level tests render every page; the live stack and screenshots are Phase 7 (`just design-check`).

## Phase 3 — Footer

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R10 full footer: tagline over the version line; nav Footer with Inspectors, Site, About groups; GitHub rel noopener; Compare not in the header; board desktop styles | green | crates/web/tests/it/site_nav_footer.rs |
| C2 | R11 phone footer: one column, tagline, groups, version last; links at least 44 px | green | crates/web/tests/it/site_nav_footer.rs |
| C3 | S footer holds tagline, nav Footer with the three groups and their links, and netray version | green | crates/web/tests/it/site_nav_footer.rs |
| C4 | S GitHub links the repository with rel noopener; inspector links are /inspect/tool | green | crates/web/tests/it/site_nav_footer.rs |
| C5 | S Compare not in the header, once in the footer | green | crates/web/tests/it/site_nav_footer.rs |
| C6 | S stylesheet: desktop footer values; phone column; links at least 44 px | green | crates/web/tests/it/site_nav_footer.rs |

Changed protected tests: `entries_gone.rs` (host names are read outside the site footer, whose GitHub link is the project's), `site_nav_footer.rs` (the 404 page needs the not-found fallback; a test for the gone page's inline footer styles), `site_nav_header.rs` (featured guides end with main, the footer's Guides link is none), `crates/lens/tests/it/result_mount.rs` (the idle page is recognised by its inspector line; every frame links `/inspect/dns` in menu and footer).

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 footer markup and styles | 3 | orchestrator | 0 | 0 |

### Review

| class | where | finding | disposition |
|---|---|---|---|
| BLOCKER | crates/web/templates/base.html:45 | the gone page's inline sheet lacked the footer rules; links ran together | fixed: the inline sheet appends `pages.css`; `site_nav_footer.rs` checks it |

### Behavioural verification

skipped: router-level tests; desktop and phone values measured in headless Chromium by the reader (1280, 480 and 390 px match boards F, G and M); the live stack runs in Phase 7.

## Phase 4 — Home grade legend

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R12 home legend: six rows A+ to F, letter in its grade tone, descriptor weight 600, meaning muted, from grade_legend; How scoring works link; inspector line adds Email | green | crates/web/tests/it/site_nav_legend.rs |
| C2 | S / lists A+, A, B, C, D, F with excellent and ahead of most domains in A+, critical and fix today in F; links /guide/scoring | green | crates/web/tests/it/site_nav_legend.rs |
| C3 | S / inspector line links /inspect/email | green | crates/web/tests/it/site_nav_legend.rs |

Changed protected tests: `site_nav_legend.rs` (the legend's classes are `score-key`: the gate gives every `.grade-` selector to the grade component; the tagline is found as the page's paragraph, not the meta description).

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 legend and inspector line | 2 | orchestrator | 0 | 0 |

### Review

No findings (reader: engineering and testing lenses).

### Behavioural verification

skipped: router-level test renders `/`; screenshots in Phase 7.

## Phase 5 — DKIM selector inputs and copy

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R13 home: details Custom DKIM selectors with input dkim_selectors, placeholder and hint in the idle form | green | crates/web/tests/it/site_nav_selectors.rs |
| C2 | R14 email panel and email evidence views end with the form Check custom DKIM selectors, GET to /domain; prefilled on a selector run | green | crates/web/tests/it/site_nav_selectors.rs |
| C3 | R15 selector run title names the selectors and not recorded; a bad list answers 400 titled Not a list of DKIM selectors | green | crates/web/tests/it/site_nav_selectors.rs |
| C4 | S / holds the selectors details; /?d=example.com and dkim_selectors=s1,s2 answers 301 to the selector run | green | crates/web/tests/it/site_nav_selectors.rs |
| C5 | S /example.com/email ends with the selectors form targeting /example.com | green | crates/web/tests/it/site_nav_selectors.rs |
| C6 | S /example.com?dkim_selectors=s1,s2 prefills the email panel input and the run title names DKIM selectors s1, s2 and not recorded | green | crates/web/tests/it/site_nav_selectors.rs |
| C7 | S /example.com?dkim_selectors=bad.sel answers 400 titled Not a list of DKIM selectors | green | crates/web/tests/it/site_nav_selectors.rs |

C4's redirect half was already in place (v2-dkim-legacy-urls); its test guards it.

Changed protected tests: `result_selectors.rs` (a plain page has no hidden selector field; the email tab's visible form has an empty input), the four island fixtures `result-fresh.html`, `result-stale.html`, `result-issues.html`, `result-trace.html` (regenerated: the email tab's selector panel).

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 home input, selector forms, run line, refusal page | 1 | opus | 107467 | 483 |

### Review

| class | where | finding | disposition |
|---|---|---|---|
| BLOCKER | crates/web/src/pages/result/mod.rs:1479 | the stored head's title was relabelled as the unrecorded selector run | fixed: a page showing a stored entry under selectors reads "DKIM selectors … · not recorded · the stored run shows until this run ends" |
| AMENDMENT | crates/web/src/pages/errors.rs:99 | the rule sentence dropped the specific reason (63 characters, non-ASCII) | repaired: the page adds "This one was refused: <reason>." |
| NIT | crates/web/src/pages/result/mod.rs:1471 | "incomplete, not recorded" got a second "not recorded" | fixed |
| NIT | crates/web/tests/it/result_selectors.rs:282 | protected test changed | trailer on the commit |

### Behavioural verification

skipped: router-level tests drive the GET page, the SSE run and the refusals through `pages::result::router`; the live stack runs in Phase 7.

## Phase 6 — IDN display

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R16 one display function; result h1, history, evidence views and /r/id headings show the Unicode form with the A-label beside it; title starts unicode (a-label); URLs, canonical, exports and API keep the A-label | green | crates/web/tests/it/site_nav_idn.rs |
| C2 | R17 the recent island shows the display form the server wrote, keyed by the A-label | green | crates/web/islands/test/recent-idn.test.ts |
| C3 | S display function: xn--bcher-kva.example gives bücher.example; example.com and an undecodable xn-- label give nothing | green | crates/web/tests/it/site_nav_idn.rs |
| C4 | S /xn--bcher-kva.example h1 shows both forms, title starts with both, canonical only the A-label | green | crates/web/tests/it/site_nav_idn.rs |
| C5 | S /xn--bcher-kva.example/email and /r/id headings show both forms | green | crates/web/tests/it/site_nav_idn.rs |
| C6 | S the result page carries the display form for the recent island; the island stores and lists both forms linking /xn--bcher-kva.example | green | crates/web/islands/test/recent-idn.test.ts |

Changed protected tests: `site_nav_idn.rs` (rustfmt only).

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 display function, headings, titles, recent island | 1 | orchestrator | 0 | 0 |

### Review

| class | where | finding | disposition |
|---|---|---|---|
| AMENDMENT | crates/web/src/pages/evidence.rs:85 | the evidence view's Markdown export took the Unicode title | repaired: the export target is the A-label |
| NIT | crates/web/src/pages/evidence.rs:172 | the no-entry evidence view keeps the A-label alone | kept: a sentence, not a domain heading |
| NIT | crates/web/templates/pages/entry.html:11 | og:title differs between /r/<id> (A-label) and the frame pages (both forms) | kept: neither shows the Unicode form alone |
| NIT | crates/web/tests/it/site_nav_idn.rs:82 | rustfmt on a protected test | trailer on the commit |

### Behavioural verification

skipped: router-level tests over a stored `xn--bcher-kva.example` entry; screenshots in Phase 7.

## Phase 7 — Design gate and visual reading

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R18 header menu, phone sheet, footer, tools page, legend, selector inputs and IDN display work without JavaScript | green | crates/web/tests/it/site_nav_header.rs |
| C2 | R19 a manifest board may name click with optional board and page selectors, clicked before measuring; unknown keys refused; design-rules describes it | green | tests/repo/test_design_manifest.sh |
| C3 | R20 the manifest measures the menu on MenuOpen, the footer on F, FDark, G, GDark, M, MDark and the sheet on M menu open; design-check exits 0; screenshots read and findings closed | green | design-check |
| C4 | S the loader accepts click and rejects an unknown key in it | green | tests/repo/test_design_manifest.sh |
| C5 | S click.page is clicked before the page is measured, click.board before the board | test-unwritable | design-check |
| C6 | S design-check over the MenuOpen, footer and menu open rows exits 0 | green | design-check |
| C7 | S screenshots read beside board F; every finding a report row, none open | green | report.md visual rows |
| C8 | S without JavaScript /, /tools and /example.com/email reach the menu, legend, cards and selector forms | green | crates/web/tests/it/site_nav_header.rs |
| V1 | visual email-view: the selector form was unstyled (browser default input, label inline) | green | design-check |
| V2 | visual email-view: four loose explore links after the evidence cards (pre-existing, v2-result-fidelity) | green | design-check |
| V3 | visual selector-run: run line replaced by the per-address refusal (state of the screenshot run: the shooter hit the run limiter) | green | design-check |
| V4 | visual selector-run: spectrum label leaders cross neighbouring labels (pre-existing spectrum layout) | green | design-check |
| V5 | visual selector-run: the email tab's selector form is not in the TLS-tab screenshot | green | design-check |
| V6 | visual email-view 390: FCrDNS table clipped without swipe hint (pre-existing evidence view) | green | design-check |
| V7 | visual email-view 390: row cells not on one baseline (pre-existing) | green | design-check |
| V8 | visual email-view: run line and check rows spacing and borders (pre-existing) | green | design-check |
| V9 | visual email-view: BIMI evidence card with a heading and no body (pre-existing) | green | design-check |
| V10 | visual selector-run and email-view: help button and export row beside the board's actions (pre-existing, decided in v2-result-fidelity) | green | design-check |
| V11 | visual tools: inspectors were plain sections, not cards | green | design-check |
| V12 | visual tools: Open link touching the input | green | design-check |
| V13 | visual tools: featured guides as a bulleted list with small rows | green | design-check |
| V14 | visual guide-csp 1280: prose runs the full container width (pre-existing guides) | green | design-check |
| V15 | visual guide-csp 390: CTA input squeezed by a wide button (pre-existing guides) | green | design-check |
| V16 | visual all pages 390: stacked footer looser than board M because every link is 44 px tall | green | design-check |

C5 is exercised by every `just design-check` run (the MenuOpen and "menu open" rows measure open states); the check needs the private canvas and a browser and is not part of the gate (design-rules.md).

### Design check

`just design-check` on the final tree: exit 0, 14 boards, 956 elements, 1801 properties measured, 0 open deviations, 0 stale accepted entries (2 accepted, both from v2-result-fidelity). The first run found one deviation pair, `footer box max-width` on M and MDark (board none, page 1120 px): the board draws no width limit at 390 px, where the page's 1120 px never binds; the property left M's footer rows (manifest, netray.info).

### Visual reading

A reader with no part in the build read `/`, `/tools`, `/guide/csp`, `/privacy`, a selector run and the email view at 1280 and 390 px, light and dark, beside board F's frame (rows V1–V16). Dispositions:

- Rebuilt: V1 (selector form takes the frame's caption label, field and 44 px height), V11–V13 (tool cards on the surface with a hairline, mono tagline, spacing; featured guides as an unbulleted row list).
- Deliberately different (implementer under the operator's autonomy mandate, decision log): V16, requirement 11 keeps 44 px footer links on phones.
- Not this feature's state or code, closed here and listed as proposals: V2, V4, V6–V10 (evidence view and result page, v2-result-fidelity / v2-result-polish), V14–V15 (guides), V3 (the screenshot run hit the per-address limiter; the refusal line is v2-result-fidelity's), V5 (the form is in the email tab; `site_nav_selectors.rs` asserts it).

The frame (header, Inspectors menu, phone sheet, footer, container) matched on all six pages in the reader's words.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 gate click, manifest rows, visual fixes | 3 | orchestrator | 0 | 0 |

### Behavioural verification

Live stack (`design_stack_up` with lens, the design fixtures): `/`, `/tools`, `/guide/csp`, `/privacy`, `/shop.example.com?dkim_selectors=s1,s2`, `/shop.example.com/email` answered 200 at 1280 and 390 px in both themes; `just design-check` clicked the Inspectors menu open on `/` and board M's menu open on `/shop.example.com` and measured both.

## Review fixes

| finding | fix | test |
|---|---|---|
| F1 refused selector run lost its refusal line on the 429 page | the "stored run shows" line applies only when the shown line is no refusal | `site_nav_selectors.rs` `c5_a_refused_selector_form_post_keeps_the_refusal_line` |
| F2 no-entry evidence view without the IDN form | heading and title show both forms | `site_nav_idn.rs` `c5_an_evidence_view_without_an_entry_shows_both_forms` |

## Distilled

Confirmed by the implementer under the operator's autonomy mandate (no operator present):

| destination | kind | entry | source |
|---|---|---|---|
| specs/solutions | judgement | page assertions passed on links the site frame carries | site_nav_legend c3, result_mount.rs, site_nav_header c8 |
| CLAUDE.md | mechanical, no check: written in the docs commit | absolute URLs from `pages::ORIGIN`; system colour words in the gate; the gone page's inline sheet; the manifest's `click` | ff49b1c8 |
| CLAUDE.md (remove) | — | none | — |
| specs/rules | — | none | — |
| measurement | — | design check: 14 boards, 1801 properties, 0 open | Phase 7 |

### Checks to build

- A page test that asserts an `href` the frame's nav or footer also renders slices the page first (repo check over `crates/web/tests/it`).
