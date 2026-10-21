# Spec: v2-site-nav

Status: Ready for Implementation
Created: 2026-10-20

## Goal

The site frame and its navigation carry what V1's had (inventory theme J, UI half, plus the UI
halves of theme I's DKIM selectors and of v2-input-edges' IDN handling) in the shape the canvas
boards draw since v43: the header's "Inspectors" entry is a menu of the five inspectors with
"All tools →" to the inspector index `/tools`; the phone menu is board M's sheet; the footer holds
the tagline, the link groups Inspectors, Site and About and the version line; the home page
explains the grades; every page that can run DKIM offers a visible selector input; an
internationalised domain shows its Unicode form beside the A-label; and no page links to a
hard-coded `https://netray.info` or `https://<tool>.netray.info`: in-page links are apex-relative
and the few absolute URLs (canonical, Open Graph, sitemap, robots, JSON-LD, share and command
snippets) come from one origin value.

## Non-goals

- The result page's header row and container (`v2-result-fidelity`): this feature changes only the
  header's nav and the footer on that frame.
- The inspector header, tabs and email inspector page (`v2-inspector-shell`); only the email
  tagline changes (requirement 9).
- Old site URLs (`/guide/<slug>.html`, `/compare/`, `/status/`): merged in v2-dkim-legacy-urls;
  only `/tools` changes here (requirement 7).
- Host routing, the redirect map's absolute `Location` headers and a configured apex:
  `feature/v2-cutover`. This feature gives the cutover one value to change (requirement 1).
- The OG card and badge renderers (IDN display there needs glyph coverage), the static
  `/og/<name>.png` row 7.55, the status page's uptime history (7.43), the API overview prose (7.45,
  7.46), MCP tools (1.35).
- The HTTP inspector's protocol views (`v2-http-ui`, built in parallel).

## Context and constraints

- Inventory (private, `netray.info/specs/inventory/`): `sections/07-site-api.md` rows 7.2, 7.3,
  7.38, 7.39, 7.44, 7.47, 7.50, 7.58, 7.59, 7.65, 7.66, 7.71 and the finding "Absolute links";
  the inspector footer rows 2.46, 4.37, 4.38; `sections/01-lens.md` 1.1 (grade legend) and 1.31;
  `sections/05-email.md` 5.2; cross-cutting findings in `v2-parity.md` ("Absolute
  https://netray.info links (nav, canonical, og, share, guides)", "`/r/{id}` and `/privacy` render
  without the site frame"). Operator decisions 2026-10-19: theme J "rebuild (301 for old URLs,
  Email/Tools in the header, full footer, grade legend)"; theme I "DKIM selectors rebuilt
  everywhere: UI, `email.` redirect, API"; canvas 9.4 and 9.40 "adjust canvas: boards get the theme
  toggle and the footer with the privacy link". Operator decisions 2026-10-20: see Decision log.
- Boards (private, canvas v45, `netray.info/specs/design/canvas/`): F, FFirst, FTrace, G, M and
  their dark imports draw the header nav "Inspectors ⌄", Guides, API, Status, Theme and the full
  footer: tagline "Every check of a domain, in one spectrum." over the version line `netray
  <version>` (mono 11 px), a nav `aria-label="Footer"` with three groups, each a mono 11 px
  uppercase heading (letter spacing .12em) over links in ink: Inspectors (DNS, TLS, HTTP, Email,
  IP), Site (Guides, Compare, Status, API docs), About (Privacy, GitHub). Desktop: the footer box
  1120 px wide, padding 0 32 px 40 px, an inner row with a hairline top border, padding-top 24 px,
  wrap, space-between, gap 24 px 48 px; the nav gap 20 px 48 px; a group column gap 6 px; links gap
  4 px 18 px. Phone (M): footer padding 0 16 px 24 px, one column, gap 20 px, padding-top 20 px,
  the nav a column with gap 16 px. `D3 · Inspectors menu (open)` (MenuOpen, 1280 light): the
  "Inspectors" button in ink, weight 600, chevron turned; under the header, right-aligned, a panel
  400 px wide, padding 6 px, hairline border, radius 2 px, surface background, shadow; five links
  (min-height 44 px, padding 8 px 12 px, column gap 2 px) each a mono 12 px bold uppercase label
  (letter spacing .08em) over a 13 px muted description: "DNS records and resolvers", "TLS
  certificates and handshake", "HTTP headers and redirects", "Email MX, SPF, DKIM, DMARC", "IP
  location and network"; then "All tools →" (13 px, min-height 44 px, padding 0 12 px, margin-top
  4 px, hairline top border). Board M's menu button opens a sheet (`nav#mobile-menu`): a mono
  11 px "Inspectors" label, the five links (padding 6 px 14 px), "All tools →" (14 px), then a
  hairline-separated group Guides, API, Status, every row at least 44 px tall. The board hrefs
  (`/dns`, `/guides`, `/api/docs`) are placeholders; the page keeps its real paths.
- On main: `templates/base.html` holds the frame (`<details class="page-menu">` whose summary is the
  phone menu button and whose `nav.page-nav` holds Inspectors (`/inspect/dns`), Guides, API,
  Status; the theme button beside it), a footer with the tagline and Privacy, Status, Compare.
  `src/pages/mod.rs` holds `ORIGIN` (used by the canonical and the share snippets) beside literal
  `https://netray.info` and `https://<tool>.netray.info` URLs in robots, the sitemap and the
  landing OG image; `src/pages/guides.rs` (canonical, breadcrumbs, OG image), `src/pages/jsonld.rs`,
  the guide index and the guide CTAs (`tool("https://dns.netray.info/", …)`) and the scoring
  guide's command snippets hold more. `/tools` and `/tools/` answer 301 to `/` (v2-dkim-legacy-urls)
  "until a /tools page takes the redirect over". `/privacy` renders without header, footer and
  shared stylesheet. The email inspector's tagline reads "Email, authenticated". The home page's
  inspector line names DNS, TLS, HTTP, IP. `src/grade_legend.rs` holds the grade descriptors once
  ("excellent, ahead of most domains" … "critical, fix today").
- Backend halves merged on main: `specs/features/v2-dkim-legacy-urls/` (`GET /<domain>?
  dkim_selectors=` runs on load, `no-store`; `GET /?d=…&dkim_selectors=` 301; the selector rule
  1–5 tokens of letters, digits, hyphens, 1–63 each, in `src/selectors.rs`; the hidden field in
  `partials/result_header.html`); `specs/features/v2-input-edges/` (`netray_common::domain::
  validate` maps UTS #46 to the A-label; `idna` is a dependency of netray-common);
  `specs/features/v2-inspector-shell/` (the email inspector page `/inspect/email` takes a domain
  then DKIM selectors in its one field, `email_input`).
- The design gate (`specs/rules/design-rules.md`, `tests/acceptance/design/`) measures a board and
  its page as rendered after load; it cannot open a menu, so neither the MenuOpen panel on the
  page nor board M's sheet (closed until its button is pressed) is measurable today.
- SDD: §6.1 (explore links apex-relative), §7.3 (redirect map, `/tools` row), §8.1 (`base.html`
  the one frame), §8.7 (taglines in one partial).
- Gate rules: page tests assert by markup; a repo check over files reads `git ls-files`; no
  `Router` word in `crates/web/src` outside `entries/`, `pages/`, `api/`.
- Test data: `example.com`, `bücher.example` (A-label `xn--bcher-kva.example`), selectors `s1`,
  `s2`.

## Requirements

### Origin and links

1. **One origin.** netray-web holds one origin value, `pages::ORIGIN` (`https://netray.info`), and
   every absolute URL a page or route writes is built from it: canonical, `og:image`, the sitemap's
   locs, robots' `Sitemap:` line, JSON-LD, guide breadcrumbs, the guide command snippets and the
   share snippets. (finding "Absolute links", 7.50)
2. **Relative in-page links.** Every link and form action between pages is apex-relative: the
   inspectors are `/inspect/<tool>` (the guide CTAs, the scoring guide's table, guide body links
   and the guide index), the email CTAs target `/inspect/email` with the field `d`. (7.38, 7.39,
   4.37)
3. **No literal.** A repository check `tests/repo/test_no_absolute_apex.sh` fails when a tracked
   file under `crates/web/templates`, `crates/web/src` or `crates/web/islands/src` contains
   `https://netray.info` or `https://<label>.netray.info`, except the one line defining `ORIGIN`.
4. **Sitemap.** The sitemap lists the origin's `/`, `/guide/` and each guide, `/compare`,
   `/status`, `/privacy`, `/tools`, `/api/v1/docs` and `/inspect/<tool>` for DNS, TLS, HTTP, Email
   and IP, and no subdomain URL. (7.50)

### Header and frame

5. **Inspectors menu.** The header nav holds, in order: a disclosure "Inspectors" (a `<details>`
   whose summary reads "Inspectors" with a chevron), Guides (`/guide/`), API (`/api/v1/docs`),
   Status (`/status`). The disclosure's panel lists DNS, TLS, HTTP, Email, IP, each a link to
   `/inspect/<tool>` with the label and the board's description, then "All tools →" to `/tools`,
   in the MenuOpen board's styles. The panel opens without JavaScript; with the `menu` island an
   open panel closes on Escape and on a click outside it. On `/tools` and every inspector page the
   summary is marked current (ink, weight 600) and on an inspector page its own link carries
   `aria-current="page"`. (7.58, 7.59, 9.4)
6. **Phone sheet.** At the phone width the menu button opens board M's sheet: a mono "Inspectors"
   label, the five inspector links with their descriptions, "All tools →", then Guides, API and
   Status under a hairline, every row at least 44 px tall; the disclosure's summary is the label,
   its panel always shown inside the sheet. (9.67)
7. **Tools page.** `GET /tools` answers 200 with the inspector index in the frame: h1
   "Inspectors", one card per inspector (DNS, TLS, HTTP, Email, IP) with its name, its tagline, the
   shell's description of what it shows, what it takes ("a name and record types", "a host and
   ports", "a URL", "a domain and DKIM selectors", "an address, or none for yours"), a quick form
   (`query_input` to `/inspect/<tool>` with the tool's field) and "Open the <name> →"; then
   "Featured guides" (two per inspector) and a line linking `/api/v1/docs`. `GET /tools/` answers
   301 to `/tools`. SDD §7.3's row and v2-dkim-legacy-urls' test change in the same commit
   (`ADLC-Test-Change`). (7.44, 7.3, 7.42)
8. **Privacy in the frame.** `/privacy` renders inside the site frame (header, footer, shared
   stylesheet) with the title "Privacy · netray". (7.47, cross-cutting finding)
9. **Inspector taglines.** The inspector taglines read "DNS, resolved", "TLS, illuminated", "HTTP,
   inspected", "Email, verified", "IP, located".

### Footer

10. **Full footer.** The footer on every page, as boards F, G and M draw it: the tagline "Every
    check of a domain, in one spectrum." (the one tagline partial) over the version line `netray
    <workspace version>`; a nav `aria-label="Footer"` with the groups Inspectors (DNS, TLS, HTTP,
    Email, IP to `/inspect/<tool>`), Site (Guides `/guide/`, Compare `/compare`, Status `/status`,
    API docs `/api/v1/docs`) and About (Privacy `/privacy`, GitHub
    `https://github.com/netray-info/netray` with `rel="noopener"`), in the boards' desktop styles.
    Compare is not in the header. (7.65, 7.66, 2.46, 4.38, 9.40)
11. **Footer on phones.** At the phone width the footer is board M's column: tagline, the three
    groups stacked, the version line last; every footer link at least 44 px tall. (9.67)

### Home

12. **Grade legend.** Under the home page's domain input, a section "What the grades mean": six
    rows A+, A, B, C, D, F, each the letter in its grade tone, the descriptor in weight 600 and the
    meaning in muted text, read from `grade_legend::descriptor`; then a link "How scoring works" to
    `/guide/scoring`. The home page's inspector line adds Email (`/inspect/email`). (7.2, 1.1)

### DKIM selectors

13. **Home input.** Under the home page's domain input, a `<details>` "Custom DKIM selectors"
    holds an input `dkim_selectors` (placeholder "selector1, selector2") with the hint "Up to 5,
    comma-separated. A run with custom selectors is not recorded."; the idle form's GET carries it
    to `/?d=…&dkim_selectors=…`, which redirects to the selector run. Empty means none. (1.31, 5.2,
    7.71)
14. **Result and email views.** The email panel of the result page and the email evidence views
    `/<domain>/email` and `/r/<id>/email` end with a form "Check custom DKIM selectors": the input
    `dkim_selectors` and "Run with selectors", a GET to `/<domain>`. On a selector run's page the
    input is prefilled with the normalised list, and the hidden field in the recheck form keeps
    carrying it. (1.31, 5.2)
15. **Selector run copy.** A selector run's run title names the selectors and that the run is not
    recorded ("DKIM selectors s1, s2 · not recorded"); a list that breaks the rule answers the 400
    page titled "Not a list of DKIM selectors" with the rule in one sentence, instead of "Not a
    domain name". (v2-dkim-legacy-urls report)

### IDN display

16. **Unicode beside the A-label.** One function in netray-web returns a name's Unicode form when
    its A-label decodes (UTS #46 to_unicode) without error and differs from it, else nothing. Where
    a domain is the heading of the result page, `/<domain>/history`, the evidence views and
    `/r/<id>`, a decoded name shows the Unicode form with the A-label beside it in mono muted text;
    never the Unicode form alone. The page `<title>` starts "<unicode> (<a-label>)". URLs,
    canonical, exports and the API keep the A-label. (v2-input-edges non-goal)
17. **Recent list.** The recent-domains island shows the display form the server wrote in a data
    attribute of the result page, keyed by the A-label. (v2-input-edges non-goal)

### Acceptance

18. **No JavaScript.** Header menu, phone sheet, footer, tools page, legend, selector inputs and
    IDN display work without JavaScript. (S7)
19. **Gate opens menus.** A manifest board may name `click`: a board selector and a page selector,
    each optional, clicked in that order before the board and the page are measured; the loader
    rejects an unknown key in it. `specs/rules/design-rules.md` describes it.
20. **Design gate.** The private manifest measures the header menu on the MenuOpen board (page `/`
    with the menu clicked open), the footer on F, FDark, G, GDark, M and MDark, and the phone sheet
    on M as the state "menu open"; `just design-check` exits 0, every row fixed or accepted by a
    decision. Screenshots of `/`, `/tools`, `/guide/csp`, `/privacy` at 1280 px and 390 px are read
    by a reader with no part in the build beside board F's frame; every finding is a `report.md`
    row and is closed. (9.4, 9.40, 9.67)

## Phase 1 — Origin and relative links

**Depends on:** none
**Requirements:** 1, 2, 3, 4

### Test Scenarios

GIVEN `/guide/csp` WHEN rendered THEN its canonical is `https://netray.info/guide/csp`, its CTA form action is `/inspect/http` and no element links to a `netray.info` subdomain.
GIVEN `/sitemap.xml` and `/robots.txt` WHEN fetched THEN every loc starts with `https://netray.info/`, the sitemap lists `/tools`, `/api/v1/docs` and `/inspect/email` and no subdomain URL, and the `Sitemap:` line names `https://netray.info/sitemap.xml`.
GIVEN `/guide/scoring` WHEN rendered THEN its table links `/inspect/ip`, `/inspect/dns`, `/inspect/tls` and `/inspect/http`.
GIVEN `/guide/dkim` WHEN rendered THEN its CTA form action is `/inspect/email` with the field `d`.
GIVEN `/guide/` WHEN rendered THEN each guide link is `/guide/<slug>`.
GIVEN a tracked template with `https://dns.netray.info` WHEN `tests/repo/test_no_absolute_apex.sh` runs THEN it fails; GIVEN the tree after this phase THEN it passes.

## Phase 2 — Header menu, phone sheet, tools page, privacy, taglines

**Depends on:** Phase 1
**Requirements:** 5, 6, 7, 8, 9

### Test Scenarios

GIVEN any page WHEN rendered THEN the header nav holds a details "Inspectors", then Guides, API and Status, all apex-relative, and the panel links `/inspect/dns`, `/inspect/tls`, `/inspect/http`, `/inspect/email`, `/inspect/ip` with their descriptions and "All tools →" to `/tools`.
GIVEN `/inspect/tls` WHEN rendered THEN the Inspectors summary is marked current and the TLS link carries `aria-current="page"`.
GIVEN `/tools` WHEN rendered THEN the Inspectors summary is marked current.
GIVEN the stylesheet WHEN read THEN the menu panel, its links, labels, descriptions and "All tools →" resolve to the MenuOpen board's values, and at the phone width the sheet's rows are at least 44 px tall.
GIVEN an open menu and the `menu` island WHEN Escape is pressed or a click lands outside THEN the details closes.
GIVEN `GET /tools` WHEN answered THEN status 200, an h1 "Inspectors" and five cards DNS, TLS, HTTP, Email and IP, each with its tagline, a quick form to `/inspect/<tool>` with its field name and "Open the <name> →", then "Featured guides" and a link to `/api/v1/docs`.
GIVEN `GET /tools/` WHEN answered THEN 301 to `/tools`.
GIVEN `/privacy` WHEN rendered THEN it carries the site header, the footer and `/static/app.css`, and the title "Privacy · netray".
GIVEN `/inspect/email` WHEN rendered THEN its tagline reads "Email, verified".

## Phase 3 — Footer

**Depends on:** Phase 2
**Requirements:** 10, 11

### Test Scenarios

GIVEN any page WHEN the footer renders THEN it holds the tagline, a nav labelled Footer with the groups Inspectors, Site and About holding DNS, TLS, HTTP, Email, IP, Guides, Compare, Status, API docs, Privacy and GitHub, and the line `netray <CARGO_PKG_VERSION>`.
GIVEN the footer WHEN rendered THEN GitHub links `https://github.com/netray-info/netray` with `rel="noopener"` and the inspector links are `/inspect/<tool>`.
GIVEN the header WHEN rendered THEN Compare is not in it and appears once, in the footer.
GIVEN the stylesheet WHEN read THEN the footer resolves to the boards' desktop values, and at the phone width to board M's column with footer links at least 44 px tall.

## Phase 4 — Home grade legend

**Depends on:** Phase 1
**Requirements:** 12

### Test Scenarios

GIVEN `/` WHEN rendered THEN a section "What the grades mean" lists A+, A, B, C, D and F with "excellent" and "ahead of most domains" in the A+ row and "critical" and "fix today" in the F row, and links `/guide/scoring`.
GIVEN `/` WHEN rendered THEN the inspector line links `/inspect/email`.

## Phase 5 — DKIM selector inputs and copy

**Depends on:** Phase 1
**Requirements:** 13, 14, 15

### Test Scenarios

GIVEN `/` WHEN rendered THEN the form holds a details "Custom DKIM selectors" with an input `dkim_selectors` and the hint; GIVEN `GET /?d=example.com&dkim_selectors=s1,s2` THEN 301 to the selector run.
GIVEN `/example.com/email` WHEN rendered THEN it ends with the form "Check custom DKIM selectors" whose GET targets `/example.com` with the input `dkim_selectors`.
GIVEN `GET /example.com?dkim_selectors=s1,s2` WHEN rendered THEN the email panel's selector input holds `s1,s2` and the run title names "DKIM selectors s1, s2" and "not recorded".
GIVEN `GET /example.com?dkim_selectors=bad.sel` WHEN answered THEN 400 with the title "Not a list of DKIM selectors".

## Phase 6 — IDN display

**Depends on:** Phase 1
**Requirements:** 16, 17

### Test Scenarios

GIVEN `xn--bcher-kva.example` WHEN the display function runs THEN it returns `bücher.example`; GIVEN `example.com` THEN nothing; GIVEN an `xn--` label that does not decode THEN nothing.
GIVEN a stored entry for `xn--bcher-kva.example` WHEN `/xn--bcher-kva.example` renders THEN the h1 shows `bücher.example` with `xn--bcher-kva.example` beside it, the title starts "bücher.example (xn--bcher-kva.example)" and the canonical carries only the A-label.
GIVEN that entry WHEN `/xn--bcher-kva.example/email` and `/r/<id>` render THEN their headings show both forms.
GIVEN the result page of that entry WHEN rendered THEN it carries the display form in the recent island's data attribute, and the island stores and lists both forms linking `/xn--bcher-kva.example`.

## Phase 7 — Design gate and visual reading

**Depends on:** Phases 1–6
**Requirements:** 18, 19, 20

### Test Scenarios

GIVEN a manifest board with `click` WHEN the loader reads it THEN it is accepted, and an unknown key in `click` is rejected.
GIVEN a board with `click.page` WHEN the page is measured THEN the selector is clicked before the values are read; GIVEN `click.board` THEN the board's frame is clicked before it is measured.
GIVEN the manifest's MenuOpen, footer and "menu open" rows WHEN `just design-check` runs THEN it exits 0.
GIVEN the screenshots WHEN read beside board F's frame THEN every finding is a `report.md` row and none stays open.
GIVEN no JavaScript WHEN `/`, `/tools` and `/example.com/email` render THEN the menu, the legend, the cards and the selector forms are reachable.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate for features 0–7
  (operator, 2026-10-19).
- Inspector taglines: DNS, resolved · HTTP, inspected · Email, verified · IP, located (TLS as board
  G); set in the inspector shell header (operator, 2026-10-20).
- Header keeps four entries; "Inspectors" is a menu listing DNS, TLS, HTTP, Email, IP, with
  `/tools` as the no-JavaScript page; the canvas gets an open-menu board (operator, 2026-10-20).
- Full footer as in V1 by adjusting the canvas (operator, 2026-10-20); the canvas v43 draws it.
- The footer follows the v43 boards, not the draft's "Project" group: About holds Privacy and
  GitHub, Site holds API docs, the version is a plain line `netray <version>`, and V1's Author
  link and a Tools footer link are not added (the boards are the reference; "All tools →" sits in
  the menu) (implementer under the operator's autonomy mandate, 2026-10-19).
- The origin stays one constant on main instead of a router parameter: threading a runtime value
  through every page's frame touches every constructor while nothing on main would set it; the
  cutover, which owns the apex config, turns the one constant into its value (implementer under the
  operator's autonomy mandate, 2026-10-19).
- The menu is a `<details>` in the nav, the same element the phone sheet shows open: no JavaScript
  needed, one DOM for both widths; the island only adds Escape and click-outside closing
  (implementer under the operator's autonomy mandate, 2026-10-19).
- The gate gets an optional `click` per board instead of separate open-state boards in the canvas:
  board M's sheet exists only after its button is pressed, and the page's menu is closed after load
  (implementer under the operator's autonomy mandate, 2026-10-19).
- The draft's requirement "email inspector input" (a separate selectors field on `/inspect/email`)
  is dropped: `v2-inspector-shell` already takes the selectors after the domain in the page's one
  field and passes them to the job (`email_input`) (implementer under the operator's autonomy
  mandate, 2026-10-19).
- The desktop menu panel has no shadow: tokens hold no elevation shadow and the gate refuses a
  colour literal; border and surface carry the panel (implementer under the operator's autonomy
  mandate, 2026-10-19).
- The menu island is registered as `nav-menu`: the gate reads the word `menu` as a CSS system
  colour (implementer under the operator's autonomy mandate, 2026-10-19).
- The result page's selector form sits in its own `selector-panel` beside the email tab panel,
  never in a streamed fragment: a stream that replaced the panel would drop the form, and a plain
  run's stream must not carry selector copy (implementer under the operator's autonomy mandate,
  2026-10-19).
- Phone footer links stay 44 px tall (requirement 11) although board M draws them at text height;
  the stacked footer is looser than the board (visual row V16) (implementer under the operator's
  autonomy mandate, 2026-10-19).
- Board M's footer rows do not measure `max-width`: the board draws no limit at 390 px and the
  page's 1120 px never binds there (implementer under the operator's autonomy mandate,
  2026-10-19).
- Inspector links point at `/inspect/<tool>`, not at `<tool>.<apex>`: valid before and after the
  cutover and on every host (drafting agent, 2026-10-20).
- `/tools` becomes the inspector index again and takes over the redirect v2-dkim-legacy-urls set
  for it, as that spec's decision log foresaw (drafting agent, 2026-10-20).
- The visible selector input on the result page sits in the email panel and email views, not in
  the result header: board F's header row holds Re-check and Share only, and the selectors belong
  to the email check they change (drafting agent, 2026-10-20).
- IDN names always show the A-label beside the Unicode form: a Unicode form alone could hide a
  homograph, and the A-label is what is stored and checked (drafting agent, 2026-10-20).
- Boards: none for the home page, `/tools`, guides and privacy; their screenshots are read beside
  board F's frame. Proposal for the operator: a home board (input, selectors disclosure, legend)
  and a `/tools` board (drafting agent, 2026-10-20).

## Open decisions

None.

## Out of scope

- Result-page header row and container: `v2-result-fidelity`.
- Inspector header, tabs, email page: `v2-inspector-shell`.
- Host routing, a configurable apex and absolute redirect `Location`s: `feature/v2-cutover`.
- OG card and badge IDN rendering, rows 7.43, 7.45, 7.46, 7.55, 1.35.
