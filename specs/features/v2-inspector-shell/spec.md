# Spec: v2-inspector-shell

Status: Ready for Implementation
Created: 2026-10-19

## Goal

Every inspector page runs in one shared shell that gives back the workflow V1's inspectors had
(inventory theme A): a header naming the inspector, tabs between the inspectors, a query form with
clear button and example chips, a URL that follows the query, a share link that shows a finished
run again for an hour without re-running it, a query history, an autocomplete hook, a help dialog,
keyboard shortcuts, and copy as Markdown and CSV and JSON downloads of the result. A live inspector
run shows the same check list (status, finding, fix hint, guide) as the stored
`/<domain>/<protocol>` view (theme B), and inspectors link to each other and show IP enrichment
badges where V1 had them (theme C). The shell is one component used by the DNS, TLS, HTTP, email
and IP inspectors; email gets its inspector page `/inspect/email`. The result page and the
evidence views reuse the shell's help, shortcuts and export parts. Board G (17/18, TLS inspector)
is binding for the shell parts: header, inspector tabs, query form, run title and host heading,
back link, band position and section headings; `just design-check` exits 0 for those elements on
G and GDark.

## Non-goals

- Inspector-specific depth, each a later UI half: DNS servers tab, view toggles, explain, record
  details and tab badges (theme D, and row 2.43); the TLS certificate view, addresses table,
  protocols grid, port chooser, multi-port and per-field copy buttons (theme E, canvas rows
  9.50, 9.55–9.57, row 3.18); HTTP CSP and cross-origin views (theme F); the IP page, its copy
  buttons and CLI (theme G, row 6.2). The shell's copy primitive is what those halves reuse.
- The result page's look (boards F, FFirst, FTrace, M): `v2-result-fidelity` (merged) and
  `v2-result-polish`. This feature adds only help, shortcuts and export to it, in Phase 8.
- Subdomain hosts and the redirect map (`dns.`, `tls.`, `http.`, `ip.`, `email.` by `Host`), and
  where prism's legacy `dns.netray.info/?r=<key>` lands: `v2-cutover`. The shell works on
  `/inspect/<tool>` and keeps working when the hosts route there.
- Site navigation outside the inspector header (theme J): the Inspectors menu (canvas v43 board
  `D3 · Inspectors menu (open)`, the chevron on G's nav entry) and the full footer of v43 (link
  groups, version line). The header keeps the nav of `v2-result-fidelity` requirement 2.
- Recording inspector runs in the history: a shared run lives in memory for an hour, never in the
  store (§6.2 as amended here).
- The design gate tooling: `v2-design-gate` (merged); this feature only adds manifest boards and,
  where the check cannot reach a finished run, the wait it needs (requirement 28).

## Context and constraints

- Inventory (private, `netray-ops/specs/netray/inventory/`), operator decisions of 2026-10-19
  (`v2-parity.md`): theme A "rebuild, as one shared component for every inspector", theme B
  "rebuild: a live run shows the same check list as `/<domain>/<module>`", theme C "rebuild".
  Rows by section: `02-dns.md` 2.4, 2.22, 2.23, 2.35–2.43; `03-tls.md` 3.11, 3.12, 3.18, 3.32,
  3.33, 3.35–3.38, 3.41, 3.42; `04-http.md` 4.2, 4.4, 4.7–4.10, 4.14, 4.15, 4.17, 4.34, 4.35;
  `05-email.md` 5.18, 5.20, 5.27, 5.28; `06-ip.md` 6.2, 6.4, 6.7, 6.22; `01-lens.md` 1.11, 1.20,
  1.21; `07-site-api.md` 7.64. Canvas rows of board G: `09-canvas.md` 9.48–9.59, 9.66.
- Board G (private, `design/canvas/G.dc.html`, PNGs 17/18, export v43): header "wordmark · TLS
  inspector" (21 px weight 650) with the tagline "TLS, illuminated" (13 px muted); right nav
  Inspectors (current), Guides, API, Status, Theme; tabs DNS TLS HTTP EMAIL IP (mono 12 px
  uppercase, marker on current, rule below); a surface card with label "Host", a 44 px input in
  mono 16 px on `bg`, and "Inspect" (44 px, 14 px weight 600); run title "LIVE · 2026-10-08 14:41
  UTC · NOT RECORDED" (mono 11 px uppercase), h1 "example.com:443" 32 px weight 650,
  right-aligned back link with the grade letter; the TLS band directly under the h1, each line
  named and coded under it (mono 11 px and 10 px) and the caption "The TLS band of the spectrum,
  zoomed: the same 9 lines as positions TLS·10–18 on the result page."; sections with h2 20 px
  weight 650, each its own card; a dashed box "Related records live in the DNS inspector: …" with
  "Open in the DNS inspector →".
- SDD `specs/sdd/v2.md`: §6.2 (inspectors: jobs without a domain verdict, never stored, "live · not
  recorded", back link to the registrable domain), S3 (`email.` keeps no inspector), §7.3
  (inspectors keep `?q=`, `?h=`, `?url=`, `?ip=`; prism's `?r=` ignored), §8.3 (`query_input`,
  `check_row`, `evidence_block` components), §8.4 (islands compute no layout and hold no verdict;
  bundle capped at 30 KB gzipped by a test), S7 (no SPA), S25 (no inline script, no
  `unsafe-eval`). This feature amends S3, §6.2 and §7.3 (requirements 13, 23).
- Code on main (e3d455fe): `crates/web/src/pages/inspect.rs` (`Tool` with `Dns`, `Tls`, `Http`,
  `Ip` and fields `q`, `h`, `url`, `ip`, headings, placeholders; `inspect_router` with `GET
  /inspect/{tool}` and `POST /inspect/{tool}/run`; `Job` streams parts as `Vec<Block>` and the
  band as `Vec<CheckResult>`; `InspectorJobs` seam implemented by `api::jobs::InspectJobs`;
  `backlink`, `target_host`), `templates/pages/inspector.html` (h1, form, the DNS builder
  fieldset behind a branch on the stream URL, `#inspect-backlink`, `#inspect-result`), partials
  `inspect_start.html`, `inspect_band.html`, `inspect_part.html`, `inspect_backlink.html`,
  `inspect_notice.html`; `templates/components/query_input.html`; islands `stream.ts`, `dns.ts`,
  `share.ts`, `tabs.ts` and others under `crates/web/islands/src/`.
- Merged since the draft:
  - `v2-result-fidelity` (#42): the site header (its requirement 2: wordmark, tagline partial,
    nav Inspectors/Guides/API/Status, Theme button, phone menu on `<details>`), the check-row item
    (requirement 31, `components/check_row.html` with `line`, `meta`, `guide_href`), line codes
    (requirement 29, `pages::result::views::line_code`: "TLS·10"), the primary and secondary
    button styles (requirement 8), the share dialog (`share.ts`, requirement 41) and the result
    header partial `partials/result_header.html` with its actions column.
  - `v2-dns-api` (#38): API v1's DNS jobs store each finished job's collected result in
    `netray_dns::DnsResultCache` (moka, 1 000 entries, one hour, LRU) under a random 12-hex key
    (`DnsResultCache::fresh_key`, `netray_dns::is_key`), replayed by `GET
    /api/v1/dns/results/{key}`; `netray_dns::blocks::{query,trace,dnssec,compare,authcompare}
    _blocks` map collected results to blocks. The inspector's DNS job collects its `DnsJob`
    directly and stores nothing today. `GET /api/v1/dns/servers` and `/dns/record-types` serve
    the parser's resolver and record-type tables (`netray_dns::server_catalogue`,
    `netray_dns::record_types`).
  - `v2-tls-backend` (#34), `v2-http-checks` (#36), `v2-ip-cli` (#37): deeper blocks and checks
    the inspectors stream; nothing in the shell depends on their shapes beyond `Block` and
    `CheckResult`.
  - `v2-dkim-legacy-urls` (#39): `POST /api/v1/email/inspect` takes `{domain, dkim_selectors?}`
    (at most `dkim.max_user_selectors`, default 5), V1 beacon's UI read `?q=<domain> <sel>…`.
  - `v2-input-edges` (#41): `netray_common::domain::validate` maps input with UTS #46 (IDNA), so
    `bücher.example.com` is the domain `xn--bcher-kva.example.com`; `target_host` still refuses
    non-ASCII hosts.
- Email: `POST /api/v1/email/inspect` runs the email module live and unstored through
  `ApiState::email_inspect()` (`netray_email::EmailInspect`), the `[api.email]` limiter and
  inspection slots; `netray_email::module::translate(&[SseEvent])` maps its events to a
  `SectionOutcome`. There is no `/inspect/email` page today (`GET /inspect/email` answers 404).
- IP data: `ApiState::ip_lookup()` (`Arc<dyn netray_ip::lookup::IpLookup>`, absent without
  `[modules.ip]`) answers org, network type (cloud, hosting, residential) and threat flags (Tor,
  VPN, Spamhaus, C2) per address from local databases. V1 showed them as badges in prism
  (`ResultsTable.tsx` `ip-badges`), tlsight (`IpBadges.tsx`), spectra (overview role and threat)
  and beacon (MX "Hosted by …").
- Blocks carrying addresses as typed `addr` values: `dns.a`, `dns.aaaa` records, `tls.addresses`,
  `http.server` (server address), `email.mx_addresses`, `email.fcrdns`, `ip.addresses`.
- Gate rules (CLAUDE.md): islands write no style; page tests assert components by markup; test
  invalid input through the router; no `Router` word in `crates/web/src` outside `entries/`,
  `pages/`, `api/`; a new check id needs catalogue texts; integration tests live in
  `crates/web/tests/it`. `specs/rules/frontend-rules.md` and `specs/rules/design-rules.md` apply.
- Test data: `example.com`, its subdomains, `netray.info`, and documentation addresses
  (192.0.2.0/24, 2001:db8::/32) only.

## Requirements

Each requirement names the inventory rows or board rows it closes.

### Shell frame and form

1. **One shell.** One askama macro `inspector_shell` (with its stylesheet) and one island `shell`
   build every inspector page: header, tabs, form, result regions, help dialog, export controls.
   A per-tool view struct supplies the tool's name, tagline, field name, label, placeholder,
   examples, help content and autocomplete provider name; no template branches on a tool name or
   stream URL (the DNS builder fieldset moves into the DNS view's slot). (theme A; 9.48)
2. **Inspector header.** In the site header, after the wordmark: "·", the inspector's name ("DNS
   inspector", "TLS inspector", "HTTP inspector", "Email inspector", "IP inspector") at 21 px
   weight 650 and its tagline at 13 px muted ("TLS, illuminated" and the same pattern for the
   others) in place of the site tagline; the nav marks Inspectors as current
   (`aria-current="page"`, ink colour, weight 600). The in-page 40 px h1 tool title goes. (9.48;
   boards 17/18)
3. **Inspector tabs.** Under the header a tab row DNS, TLS, HTTP, EMAIL, IP (mono 12 px uppercase,
   the marker on the current inspector, a rule below), each a link. A tab keeps the current
   target where the other inspector takes it: the host of a TLS `host:port`, of an HTTP URL, of a
   DNS query name or of an email domain becomes `?q=<host>` for DNS, `?h=<host>:443` for TLS,
   `?url=https://<host>/` for HTTP, `?d=<registrable domain>` for email; IP takes an address
   target only. A non-ASCII host is carried in its UTS #46 form (`xn--…`), as
   `netray_common::domain::validate` maps it; TLS and HTTP get only a host name they take (letters,
   digits and hyphens in dotted labels), so `_dmarc.example.com` leaves their tabs bare. A run's
   stream re-renders the tabs for the posted target, so they follow a run started in the page.
   (9.49, 3.33, 6.7; boards 17/18)
4. **Query form.** A surface card with padding 18 px: the field label (12 px sans: "Query",
   "Host", "URL", "Domain", "Address"), the input 44 px tall in mono 16 px on the `bg` fill, a
   clear button "×" inside the input's end that empties and focuses it (shown by the island only
   when the input holds text; the server renders it hidden, since it needs JavaScript), and the submit button "Inspect" (44 px, 14 px weight 600, primary style). (9.51,
   3.42, 4.4; boards 17/18)
5. **Example chips.** Under the form while no run is shown (in the result region a run or a notice
   replaces; after a notice the form stays for the next query), two to four example queries per tool
   as links to the prefilled page; with JavaScript activating one fills the input and submits.
   (3.41, 4.2)
6. **Run title, heading and back link.** A run's result starts with the run title "LIVE ·
   <YYYY-MM-DD HH:MM> UTC · NOT RECORDED" (mono 11 px uppercase, tracking .12em) and an h1 naming
   the target as the job read it (`example.com:443`, the URL, the query, the domain, the address)
   at 32 px weight 650. On the same line as the run title, right-aligned, the back link "Back to
   the result of <domain> →" to `/<registrable domain>`, preceded by that domain's latest stored
   grade letter when its latest entry has a letter grade (an incomplete entry shows none); the route reads the grade with the history store's latest
   entry, and no job reads or writes the store (v2-inspectors requirement 11 holds). (9.52, 9.53;
   boards 17/18)
7. **Section headings.** Each result section is its own card with an h2 at 20 px weight 650; the
   page's heading order is h1 (target) then h2 sections, with no skipped level; the idle page's h1
   is visually hidden and names the inspector. (9.59, 9.66; boards 17/18)

### Band and live check list

8. **Band position.** The band renders directly under the h1, before every evidence part. Each line
   is named under the plate with the catalogue short (mono 11 px) and its line code (mono 10 px,
   `v2-result-fidelity` requirement 29: "TLS·10"); under the band the caption "The <P> band of the
   spectrum, zoomed: the same <n> lines as positions <P>·<a>–<b> on the result page." (9.54;
   boards 17/18)
9. **Live check list.** After the band, a section "Checks" holds one check row per catalogue check
   of the band's protocol, in catalogue order, built by the same function and partial as the stored
   evidence view's check list (v2-inspectors requirement 3, `evidence::build_section`): status as
   tone, title, the job's findings as body (the catalogue `finding` for a warn or fail without
   findings), the catalogue `fix_hint` for warn and fail, the guide link, and links to the blocks
   it references on this page (`#ev-<block id>`). Its heading reads "<n> lines need attention"
   with "<f> fail · <w> warn", or "All <n> lines pass". A DNS job's list is the DNS checks of the
   queried name's registrable domain (S29); the IP list is the reputation check. (4.17, 3.11,
   3.12)
10. **Same rows, same words.** For one input, the live list and the stored `/<domain>/<protocol>`
    view of a run with the same statuses and findings render the same rows with the same text; a
    test asserts it for TLS and HTTP fixtures. (theme B)

### Input workflow and sharing

11. **URL follows the query.** On submit the shell writes `?<field>=<value>` into the address bar
    with `history.pushState`; back and forward restore the input and re-run that query; loading a
    prefilled URL runs it (v2-inspectors requirement 9's autostart). (2.35, 4.7)
12. **Share link.** When a shareable run ends, its final fragment carries the run's share key and
    the shell replaces the address bar's URL with `/inspect/<tool>?r=<key>` (the query stays in the
    form). A "Copy link" button beside the run title copies that URL; where `navigator.share`
    exists on a touch device it opens the share sheet instead. A run that is not shareable
    (requirement 14) or not finished offers "Copy link" with the query URL. (2.36, 4.8, 6.4)
13. **Shared runs.** A finished run (every part and the band ended, errors included; a refusal
    notice is no run) is kept in memory by one keyed cache in the shell for one hour: tool, input,
    target heading, start time, each part's label and blocks or error text, and the band's
    protocol and checks or error text. Keys are 12 lowercase hex characters of random bits
    (`netray_dns::DnsResultCache::fresh_key`'s scheme); the cache holds at most 1 000 runs and
    drops the oldest first. `GET /inspect/<tool>?r=<key>` renders the stored run server-side in
    the same order and markup as a live run (run title "SHARED · <start time> UTC · NOT
    RECORDED"), with the form prefilled and no autostart, so it works without JavaScript and
    re-runs nothing. A key of the wrong shape, an unknown or expired key, or a key of another tool
    answers 404 with the idle shell and the notice "This shared result has expired or never
    existed; results are kept for one hour." DNS reuses the API's cache: the DNS job stores its
    collected result in `DnsResultCache` under the run's key, so `GET
    /api/v1/dns/results/<key>` answers it, and `/inspect/dns?r=<key>` for a key only
    `DnsResultCache` holds (an API job's key) renders that result's blocks through
    `netray_dns::blocks` with the band region's text "The band is not part of an API result."
    Nothing is written to the history store; the privacy page says what is kept, and for how
    long. SDD §6.2 and §7.3 are amended in the same commit. (2.36, 4.8, 6.4; operator decision
    2026-10-20)
14. **The caller's own address is never shared.** An IP inspector run of the caller's own address
    (empty input) gets no key, is not cached, and its final fragment carries no share link; its
    "Copy link" copies `/inspect/ip`. A looked-up address is shared like any run. (operator
    decision 2026-10-20)
15. **Query history.** The last 20 distinct queries per inspector live in browser storage
    (`netray.history.<tool>`). On focus of an empty input a list of them opens; Up and Down move
    through it, Enter submits the selection, Esc closes it; a "Clear history" entry empties it.
    Storage failures leave the shell working without history. (2.37, 3.36)
16. **Autocomplete hook.** The shell exposes one hook: a provider named by the tool's view returns
    suggestions for the input and caret position; the list shows under the input, Tab or Enter
    accepts the selection, Esc dismisses. The DNS inspector registers the one provider of this
    feature, suggesting record types, `+` flags and `@` resolvers from lists the server writes into
    a data attribute from the DNS crate's own tables (`netray_dns::server_catalogue`,
    `netray_dns::record_types`; `@cl` suggests `@cloudflare`). (2.4)

### Help and keyboard

17. **Help dialog.** A "?" button in the shell's tab row opens a `<dialog>` with: what the
    inspector does, its input syntax with examples, the shortcut table of requirement 18, and a
    link to the inspector's guide; the DNS help adds the resolver list, flags and record types from
    the same server tables as requirement 16. Esc and the close button close it; focus returns.
    (2.39, 3.38, 4.35, 6.22)
18. **Keyboard shortcuts.** Outside text fields: `/` focuses the input, Ctrl+L or Cmd+L focuses and
    selects it, `r` re-runs the current query, `j` and `k` move focus between check rows and
    evidence blocks, Enter on a focused check row follows its guide link, `h` and `l` move to the
    previous and next inspector tab, `?` opens the help, Esc closes the open dialog or list. Inside
    the input only Esc and the list keys of requirements 15 and 16 act. (2.38, 3.37, 4.34, 6.22)

### Export

19. **Export data.** When a run ends, its final fragment carries the run's exports as inert
    `<template>` elements the server rendered from the parts and band it streamed: Markdown (the
    run title, the check list as a table of code, check, status, finding, fix, and each evidence
    block as a table or list), CSV (long format, columns `section,item,field,value`, one row per
    check field and per block value) and JSON (`{target, checks, blocks}` in netray-model's
    serialisation, the shapes `/api/v1` uses). A shared run's page carries the same exports. No
    export re-runs the job. (2.40, 2.41, 2.42, 3.35, 4.9, 4.10)
20. **Export controls.** Beside "Copy link": "Copy Markdown" (to the clipboard), "CSV" and "JSON"
    (downloads named `netray-<tool>-<target slug>.csv` and `.json`); the controls are disabled until
    the exports exist and announce completion. (2.40, 2.41, 2.42, 3.35, 4.9, 4.10)

### Cross-links and IP enrichment

21. **Address and host links.** In inspector runs, every `addr` value in a block links to
    `/inspect/ip?ip=<address>`; host names in `tls.addresses`, `http.server`, `email.mx` and DNS
    record data (NS, MX, CNAME targets) link to the DNS inspector `?q=<host>`. The links come from
    one function over the block's typed values, not per view. (2.23, 3.33, 4.15, 6.7)
22. **IP badges.** In inspector runs, each `addr` value carries badges from the process's IP
    lookup: organisation, network type (cloud, hosting, residential) and threat flags (Tor, VPN,
    Spamhaus, C2), each a short labelled chip in the status tones (threat in fail, VPN in warn,
    others muted). The evidence renderer takes an optional enrichment lookup and stays free of
    module names; stored evidence views pass none (their evidence holds only the run's facts). A
    shared run shows the badges it was rendered with. Without `[modules.ip]` no badge renders and
    nothing fails. (2.22, 3.32, 4.14, 5.18)
23. **Related records.** The TLS inspector ends with the dashed box "Related records live in the
    DNS inspector: CAA · TLSA · HTTPS" and "Open in the DNS inspector →" to `?q=<host> CAA` (the
    record summaries inside the box belong to the TLS depth feature). (3.33, 9.58 link part;
    boards 17/18)

### Email inspector

24. **Email inspector page.** `GET /inspect/email` renders the shell with field `d`: a domain
    followed by up to `dkim.max_user_selectors` DKIM selectors separated by spaces
    (`example.com s1 s2`, V1 beacon's `?q=` syntax). `POST /inspect/email/run` streams the email
    module's live run (the inspection behind `POST /api/v1/email/inspect`, its limiter and slots
    shared with the API) as blocks, band and check list, never recorded; refused selectors answer
    the API's validation text as the notice. `[inspect.email]` gets the same `deadline_ms` and
    `max_concurrent` keys as the other inspectors (default deadline 30 000 ms). SDD S3 and §6.2 are
    amended in the same commit. (board G tabs; 5.18 via requirement 22; operator decision
    2026-10-19)

### Result page and evidence views

25. **Shared parts beyond inspectors.** The help dialog (with content for the page), the keyboard
    shortcuts (`/` focuses the idle page's or not-found page's domain input, `r` re-checks, `j` and
    `k` move between open-issue items, `?`, Esc) and the export controls (Markdown, CSV and JSON of
    the shown entry, rendered by the server from the stored run; JSON is the entry as
    `/api/v1/entries/<id>` serves it) mount on the result page `/<domain>` and on every evidence view
    `/<domain>/<protocol>` and `/r/<id>/<protocol>`. The result page's "?" button sits in its header
    actions. (1.11, 1.20, 1.21, 7.64, 5.20, 5.27, 5.28)

### Constraints and acceptance

26. **No JavaScript.** Without JavaScript the shell still renders header, tabs, the form (a GET to
    the prefilled page), example links, a shared run, and the notice that a live run needs
    JavaScript; help, history, shortcuts and export controls are absent, never broken. (S7)
27. **Bundle cap.** The `shell` island and the changes to existing islands keep the bundle under
    30 KB gzipped; the cap test passes. Islands write no style and compute no verdict. (§8.4)
28. **Design gate.** The private manifest names the shell elements of boards G and GDark (header
    title and tagline, tabs, form card, label, input, submit, run title, h1, back link, band and its
    labels and caption, section h2, related-records box) on a page path the check can render with a
    finished run (a shared run seeded by the check, or a wait for the live run's end); `just
    design-check` exits 0 for them, every row fixed or accepted by an operator decision. Elements
    of the TLS depth feature (addresses, certificate, protocols, port chooser), the Inspectors menu
    and the footer are not in this feature's manifest. (9.48, 9.49, 9.51–9.54, 9.58, 9.59)
29. **Visual reading.** Screenshots of all inspectors (idle, a finished run, a refused input) at
    1280 px and 390 px, light and dark, are read side by side with board G by a reader with no part
    in the build; every finding becomes a `report.md` row and is closed. At 390 px no inspector
    scrolls sideways and every target is at least 44 px.

## Phases

## Phase 1 — Shell frame and form

**Depends on:** `v2-design-gate` merged; `v2-result-fidelity` merged (site header and button
styles)
**Requirements:** 1, 2, 3, 4, 5, 6, 7, 26

The `inspector_shell` macro and stylesheet, per-tool view structs in `pages/inspect.rs`, the header
variant, tabs with target carry-over, the form card with clear button, example chips, run title,
h1, back link with grade. The `shell` island starts with the clear button and chips.

### Test Scenarios

GIVEN `/inspect/tls` WHEN rendered THEN the header holds "TLS inspector" and "TLS, illuminated", the nav marks Inspectors current, and no 40 px h1 tool title exists.
GIVEN `/inspect/tls?h=example.com:8443` WHEN rendered THEN the tabs link DNS to `?q=example.com`, HTTP to `?url=https://example.com/`, EMAIL to `?d=example.com` and TLS carries the marker.
GIVEN `/inspect/http?url=https://www.example.com/path` WHEN rendered THEN the TLS tab links to `?h=www.example.com:443` and the EMAIL tab to `?d=example.com`.
GIVEN `/inspect/dns?q=bücher.example.com` WHEN rendered THEN the TLS tab links to `?h=xn--bcher-kva.example.com:443`.
GIVEN `/inspect/ip?ip=192.0.2.24` WHEN rendered THEN the DNS, TLS and HTTP tabs link to their bare inspectors and the EMAIL tab to `/inspect/email`.
GIVEN `/inspect/dns` WHEN rendered THEN the template holds no branch on the tool name and the DNS builder sits in the view slot.
GIVEN an input with text WHEN the clear button is activated THEN the input is empty and focused.
GIVEN the idle TLS page without JavaScript WHEN an example chip is followed THEN it leads to `/inspect/tls?h=<example>`.
GIVEN a finished TLS run of `example.com:443` WHEN rendered THEN the run title reads "LIVE · <date> UTC · NOT RECORDED", the h1 "example.com:443", and the back link "Back to the result of example.com →" to `/example.com`.
GIVEN example.com has a stored entry graded B WHEN the back link renders THEN it starts with "B"; GIVEN none THEN it has no letter.
GIVEN a TLS run against a store WHEN it ends THEN the store's entry count and every `last_seen_at` are unchanged.
GIVEN a finished run WHEN headings are listed THEN there is one h1 and the sections are h2.
GIVEN the idle page without JavaScript WHEN rendered THEN header, tabs, form, example links and the JavaScript notice are present.

## Phase 2 — Band position and live check list

**Depends on:** Phase 1
**Requirements:** 8, 9, 10

Stream order (band first after the run title), band labels and caption, the check list from the
band's `CheckResult`s through the evidence view's check-list function.

### Test Scenarios

GIVEN a TLS run WHEN the stream ends THEN the band element precedes every part element in the result region.
GIVEN the TLS band of nine lines at positions 10 to 18 WHEN rendered THEN each line has its short and code and the caption reads "The TLS band of the spectrum, zoomed: the same 9 lines as positions TLS·10–18 on the result page.".
GIVEN an HTTP run where `http.csp` warns with finding "style-src contains 'unsafe-inline'" WHEN the list renders THEN its row shows warn, the title, that finding, the fix hint and the guide link.
GIVEN a TLS fixture with fixed statuses and findings WHEN rendered live and as the stored view THEN the check rows are identical.
GIVEN a run with one fail and two warns WHEN the list heading renders THEN it reads "3 lines need attention" with "1 fail · 2 warn"; GIVEN all pass THEN "All <n> lines pass".
GIVEN a DNS query for `www.example.com` WHEN the list renders THEN it holds the DNS checks of `example.com`.

## Phase 3 — URL, sharing, history and autocomplete

**Depends on:** Phase 2
**Requirements:** 11, 12, 13, 14, 15, 16

The shell's keyed run cache and the `?r=` page, the DNS job's write into `DnsResultCache`, the
privacy line and SDD amendment; the `shell` island's address-bar, share link, history list and
autocomplete hook; the DNS provider and its server-written tables.

### Test Scenarios

GIVEN the TLS page WHEN `example.com:443` is submitted THEN the address bar reads `/inspect/tls?h=example.com%3A443` and a history state was pushed.
GIVEN two submitted queries WHEN back is used THEN the input holds the first query and its run starts.
GIVEN a finished HTTP run WHEN the stream ends THEN the final fragment carries a 12-hex key and `GET /inspect/http?r=<key>` renders the same run title time, h1, band, check rows and blocks without starting a job.
GIVEN a shared run page WHEN rendered without JavaScript THEN its run title starts "SHARED" and its blocks are present.
GIVEN a finished run with key K WHEN "Copy link" is pressed THEN the clipboard holds the page URL `/inspect/<tool>?r=K`.
GIVEN `?r=` with an unknown key, a malformed key, or a key of another tool WHEN requested THEN the answer is 404 with the idle shell and the expiry notice.
GIVEN a run older than one hour WHEN its key is requested THEN the answer is 404.
GIVEN a finished DNS inspector run with key K WHEN `GET /api/v1/dns/results/K` is requested THEN it answers 200 with the run's mode and query.
GIVEN a key an API DNS job stored WHEN `/inspect/dns?r=<key>` is requested THEN its blocks render and the band region reads "The band is not part of an API result.".
GIVEN an IP inspector run with empty input WHEN it ends THEN no key is in the final fragment and the cache holds no new run; GIVEN `ip=192.0.2.24` THEN a key is.
GIVEN a shareable run WHEN it ends THEN the history store is unchanged.
GIVEN 21 distinct submitted queries WHEN the history list opens THEN it holds the newest 20, newest first, without duplicates.
GIVEN storage that throws WHEN a query is submitted THEN the run starts and no error escapes.
GIVEN the DNS input `example.com @cl` with the caret at the end WHEN the provider runs THEN it suggests `@cloudflare`; WHEN Tab is pressed THEN the input reads `example.com @cloudflare`.
GIVEN the TLS page WHEN it renders THEN no provider is registered and no suggestion list exists.

## Phase 4 — Help and keyboard

**Depends on:** Phase 1
**Requirements:** 17, 18

The help dialog in the shell with per-tool content, the shortcut handler in the `shell` island.

### Test Scenarios

GIVEN any inspector WHEN `?` is pressed outside the input THEN the help dialog opens with the syntax, the shortcut table and the guide link.
GIVEN the DNS help WHEN rendered THEN it lists the resolvers, flags and record types from the DNS crate's tables.
GIVEN the open dialog WHEN Esc is pressed THEN it closes and focus returns to the "?" button.
GIVEN focus outside the input WHEN `/` is pressed THEN the input has focus; WHEN Ctrl+L is pressed THEN its text is selected.
GIVEN a finished run WHEN `j` is pressed twice THEN the second check row has focus; WHEN Enter is pressed THEN its guide link is followed.
GIVEN the TLS inspector WHEN `l` is pressed THEN the HTTP tab's link is followed.
GIVEN focus in the input WHEN `r` is typed THEN it is inserted as text and nothing re-runs.

## Phase 5 — Export

**Depends on:** Phase 3
**Requirements:** 19, 20

Server-side Markdown, CSV and JSON from the streamed parts and band, the final fragment's
templates and the shared page's, the island's copy and download controls.

### Test Scenarios

GIVEN a finished HTTP run WHEN the final fragment arrives THEN it holds three templates for Markdown, CSV and JSON, and no further request is made.
GIVEN that run WHEN "Copy Markdown" is pressed THEN the clipboard holds the run title, a check table with a row per check, and a table per evidence block.
GIVEN a CSV export WHEN parsed THEN its header is `section,item,field,value` and every check and block value appears once.
GIVEN a JSON export WHEN parsed THEN it has `target`, `checks` and `blocks`, and each block deserialises as a `netray_model::evidence::Block`.
GIVEN a run in progress WHEN the controls render THEN they are disabled.
GIVEN a TLS run of `example.com:443` WHEN JSON is downloaded THEN the file is named `netray-tls-example.com-443.json`.
GIVEN a shared run page WHEN rendered THEN it holds the same three templates as the live run's final fragment.

## Phase 6 — Cross-links and IP enrichment

**Depends on:** Phase 1
**Requirements:** 21, 22, 23

One link function over typed block values, the optional enrichment lookup in the evidence renderer,
the badge chips, the related-records box.

### Test Scenarios

GIVEN a DNS run whose `dns.a` block holds 192.0.2.24 WHEN rendered THEN the value links to `/inspect/ip?ip=192.0.2.24`.
GIVEN a TLS run whose `tls.addresses` holds 2001:db8::24 WHEN rendered THEN the link percent-encodes the address.
GIVEN an IP lookup stub that answers org "Example Org", network type cloud and flag Tor for 192.0.2.24 WHEN an HTTP run renders its server address THEN three chips show, Tor in the fail tone.
GIVEN an email inspector run with `email.mx_addresses` WHEN rendered live THEN each address carries its badges.
GIVEN no `[modules.ip]` WHEN any inspector renders THEN no badge exists and the run is complete.
GIVEN the stored view `/example.com/tls` WHEN rendered THEN its addresses link to the IP inspector and carry no badges.
GIVEN a TLS run of `example.com:443` WHEN it ends THEN the related-records box links to `/inspect/dns?q=example.com%20CAA`.
GIVEN the renderer's source WHEN scanned THEN it names no module or block.

## Phase 7 — Email inspector

**Depends on:** Phase 2, Phase 3, Phase 6
**Requirements:** 24

The `/inspect/email` page on the shell, its job over `EmailInspect` and `translate`, the
`[inspect.email]` limits, and the SDD amendment.

### Test Scenarios

GIVEN `/inspect/email?d=example.com` WHEN the run is posted THEN the email module runs live and the band, check list and blocks render, and the store is unchanged.
GIVEN `d=example.com s1 s2` WHEN the run is posted THEN the inspection receives the selectors `s1` and `s2`; GIVEN six selectors with the default limit THEN the notice carries the API's refusal text and no inspection starts.
GIVEN the per-client limit of `[api.email]` is spent through `POST /api/v1/email/inspect` WHEN `/inspect/email/run` is posted THEN it answers 429.
GIVEN no email part WHEN `/inspect/email/run` is posted THEN the notice names the inspector as off.
GIVEN `[inspect.email] deadline_ms = 0` WHEN the config is checked THEN it is refused naming `inspect.email.deadline_ms`.

## Phase 8 — Result page and evidence views

**Depends on:** Phase 4, Phase 5
**Requirements:** 25

Help, shortcuts and export mounted on `/<domain>`, `/<domain>/<protocol>` and `/r/<id>/<protocol>`,
with server-rendered exports of the stored entry.

### Test Scenarios

GIVEN `/example.com` with a stored entry WHEN `?` is pressed THEN the help dialog opens with the page's help and the shortcut table.
GIVEN `/example.com` WHEN "Copy Markdown" is pressed THEN the clipboard holds the grade, score, open issues and a check table.
GIVEN `/example.com/email` WHEN JSON is downloaded THEN it equals the body of `/api/v1/entries/<id>` for the shown entry.
GIVEN the idle page WHEN `/` is pressed THEN the domain input has focus.
GIVEN `/example.com` WHEN `r` is pressed THEN the re-check form submits.

## Phase 9 — Design gate and visual reading

**Depends on:** Phases 1–8
**Requirements:** 27, 28, 29

The shell elements of G and GDark in the private manifest, a `just design-check` run on a finished
TLS run, screenshots of every inspector, the reader's pass; results under
`### Behavioural verification` in `report.md`.

### Test Scenarios

GIVEN the manifest's G and GDark shell elements WHEN `just design-check` runs THEN it exits 0.
GIVEN the islands bundle WHEN built THEN the cap test passes.
GIVEN every inspector at 390 px WHEN measured THEN no page is wider than the viewport and no target is below 44 px.
GIVEN the screenshots WHEN read beside board G THEN every finding is a `report.md` row and none stays open.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate for features 0–7
  (operator, 2026-10-19).
- Share by key `?r=` for every inspector, built once in the shell, one hour like DNS; the IP
  inspector never shares a lookup of the caller's own address (request headers and user agent),
  only looked-up addresses (operator, 2026-10-20).
- Email gets an inspector page: `/inspect/email` on the shell over the live job of `POST
  /api/v1/email/inspect`; SDD S3 and §6.2 are amended (operator, 2026-10-19).
- One `inspector_shell` macro plus one `shell` island, with per-tool view structs, over five copies
  or a client-side app: the operator asked for one shared component, and S7 rules out an SPA
  (drafting agent, 2026-10-19).
- The shared run is stored as the parts and band the run streamed (blocks, checks, error texts),
  not as rendered HTML, and rendered again on `?r=` by the same functions: one renderer, and the
  exports of requirement 19 come from the same data. The stored run keeps the IP enrichment it
  was rendered with, never looked up again (implementer under the operator's autonomy mandate, 2026-10-20).
- A shared run's title reads "SHARED · <time> UTC · NOT RECORDED" rather than "LIVE": the page
  does not run anything, and the time is the run's (implementer under the operator's autonomy
  mandate, 2026-10-20).
- An unknown, expired or malformed key answers 404 with the idle shell: the permalink no longer
  resolves, and the form lets the visitor run the query again (implementer under the operator's
  autonomy mandate, 2026-10-20).
- DNS keeps both caches under one key: the shell cache holds the whole inspector run (band
  included), `DnsResultCache` holds the collected job so API v1's replay and prism-style keys keep
  working; `?r=` reads the shell cache first and falls back to `DnsResultCache` for DNS only
  (implementer under the operator's autonomy mandate, 2026-10-20).
- The email inspector's field `d` takes V1 beacon's `<domain> <selector>…` syntax in one input, so
  the shell keeps one field per tool (implementer under the operator's autonomy mandate,
  2026-10-20).
- Exports are rendered by the server from the parts and band it just streamed and delivered as
  inert `<template>` elements in the final fragment: islands hold no verdict and compute no
  layout (§8.4), the stream island strips scripts, and a second request would re-run the job
  (drafting agent, 2026-10-19).
- CSV is one long table (`section,item,field,value`) because the result mixes a check list and
  blocks of six shapes; JSON uses netray-model's types, the shapes `/api/v1` serves (drafting
  agent, 2026-10-19).
- IP badges render only in inspector runs: a stored evidence view shows the facts its run
  recorded, and a badge looked up at view time would mix today's data into an old run (CLAUDE.md:
  an evidence block holds the fact that decided its check). Address links render everywhere
  (drafting agent, 2026-10-19).
- The check list in an inspector reuses the evidence view's function and partial, so theme B's
  "same check list" holds by construction and a test pins it (drafting agent, 2026-10-19).
- The Checks section is not on board G; the operator's theme-B decision places it, after the band
  and before the evidence parts. A reader's "extra element" finding on it is answered by that
  decision (drafting agent, 2026-10-19).
- Taglines for the other inspectors follow G's "TLS, illuminated" pattern; the operator approves
  the copy from the screenshots (drafting agent, 2026-10-19).
- Rows deferred to the inspector-depth features though listed under theme A: 2.43 (DNS tab badges,
  tied to the DNS tabs of theme D), 3.18 (copy per certificate field, part of the certificate
  view of theme E), 6.2 (copy IP and host name on the IP page of theme G). Each reuses this
  feature's copy primitive (drafting agent, 2026-10-19).
- The back link reads the domain's latest grade in the route, read-only; jobs stay store-free as
  v2-inspectors requirement 11 demands (drafting agent, 2026-10-19).
- Email rows 5.20, 5.27, 5.28 are closed on the email evidence view through requirement 25; 5.18
  is closed by the badges on `email.mx_addresses` in every live email run (drafting agent,
  2026-10-19).
- Result-page help, shortcuts and copy as Markdown (1.11, 1.20, 1.21, 7.64) are built here, on the
  shared component, after `v2-result-fidelity` (drafting agent, 2026-10-19).
- Phase 1 review: a notice keeps a visually hidden h1 naming the inspector; the grade read is
  bounded by 500 ms (and the run's deadline) and a store error is logged and read as no entry; a modified click
  on an example chip stays a link (implementer under the operator's autonomy mandate,
  2026-10-20).
- The Inspectors menu and the full footer of canvas v43 belong to the site-navigation feature
  (theme J); a reader's finding on G's chevron or footer is answered by this non-goal
  (implementer under the operator's autonomy mandate, 2026-10-20).

## Open decisions

None.

## Out of scope

- Inspector depth (themes D, E, F, G) and rows 2.43, 3.18, 6.2.
- The result page's look: `v2-result-fidelity`, `v2-result-polish`.
- Host routing and redirects, including where legacy `?r=` links land: `v2-cutover`.
- Site navigation, the Inspectors menu and the footer: the site-navigation feature.
