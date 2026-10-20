# Spec: v2-http-ui

Status: Ready for Implementation
Created: 2026-10-20

## Goal

The HTTP inspector and the stored HTTP evidence views render the parsed views that v2-http-checks
put into the evidence (inventory theme F, UI half): the Content-Security-Policy as a directive
table with its issues attached and the Report-Only marker, the parsed HSTS fields, the caching
view (Cache-Control directives, ETag with its weak marker, Last-Modified, Vary, Age), the detected
CDN, COOP as the graded check it now is and COEP and CORP as present or missing, and the server
facts with "Inspection took <n>". Repeated `Set-Cookie` rows read as `set-cookie`. To get there
this feature adds the seam every inspector UI half builds on: **protocol views**, renderings of
named evidence blocks that sit in place of the generic block while the generic evidence renderer
stays free of module names, the **facts grid** component, people-readable values in the generic
renderer, and the **slim inspector band** of board G that DNS, TLS and IP reuse.

Closes inventory rows 4.12, 4.23, 4.25, 4.26, 4.29, 4.30, 4.32 (finding "set-cookie joined").

## Non-goals

- The shell (header, tabs, form, live check list, export, links, badges): `v2-inspector-shell`
  (merged, #46). This feature renders inside the shell's result region and the evidence views.
- New checks, weights or catalogue texts. COOP is graded since v2-tls-versions-coop (#43, profile
  v4); COEP and CORP stay outside the grade.
- Speed colouring of the inspection duration (V1 fast, ok, slow; row 4.12): operator decision,
  the duration is shown neutrally.
- The cookie table's column split (4.28), the port-80 message (4.20), the empty-redirects text
  (4.19), leakage header names (4.31), XFO/XCTO/Referrer-Policy rows (4.24): the shell's live check
  list carries their findings; not rebuilt here.
- The result page's compact tables (`compact.rs`, v2-result-polish): unchanged; they keep raw
  values.
- An "HTTP inspector" board on the canvas: a proposal for the operator (Decision log).

## Context and constraints

- Inventory (private, `netray.info/specs/inventory/sections/04-http.md`): rows above. Operator
  decision 2026-10-19 (`v2-parity.md`): theme F "rebuild".
- Backend half, merged: `specs/features/v2-http-checks/`, and `v2-tls-versions-coop` (COOP check
  `http.coop`, referencing `http.headers` and `http.isolation`). Blocks, each only when its fact
  exists (`crates/http/src/module.rs` `REGISTER`):
  - `http.csp` (tree): root `report_only` (bool), `issue_count` (int), `issue` (text, repeated, for
    directives the policy lacks); one child per directive in policy order with `directive`,
    `source` (repeated) and `issue` (repeated).
  - `http.hsts` (fields): `max_age` (int, seconds, when parsed), `include_subdomains`, `preload`.
  - `http.caching` (fields, possibly empty): `directive` (repeated), `max_age`, `etag`,
    `etag_weak`, `last_modified` (time), `vary` (repeated), `age`.
  - `http.cdn` (fields): `provider`, `cache_status`, `indicator` (repeated).
  - `http.isolation` (fields): `coop`, `coep`, `corp` (text, when received and readable) and
    `missing` (repeated, the lowercase header name).
  - `http.server` (fields): `final_url`, `status_code`, `http_version`, `inspection_ms`
    (duration), `address`, `org`, `network_type`, `error_kind`; older stored entries hold
    `response_ms` instead of `inspection_ms`.
  - `http.headers` rows `set-cookie`, `set-cookie#2`, …; the repeat rule `<key>#<n>` is shared
    (`crates/model/src/canonical.rs`, SDD §3.7).
  - Checks `http.csp`, `http.hsts`, `http.caching`, `http.coop` reference their block beside
    `http.headers`; `http.cdn` feeds no check.
- V1 reference: `crates/http/frontend/src/components/` (`CspAnalysis.tsx`, the Security Headers,
  Caching and Server cards).
- Generic renderer today: `crates/web/src/evidence.rs` (`block_view_with`, `build_section`,
  `panel`) and `templates/components/evidence_block.html` print keys and values as stored
  (`true`, `77 ms`, `set-cookie#2`, a time as RFC 3339), the tree as indented `key: value` lines.
  `format_value` also feeds the exports (`export.rs`), the compact tables (`compact.rs`), links
  and explore; those keep raw values.
- Where blocks render today: the inspector parts (`pages/inspect.rs` `part_html`, live and shared
  runs; the checks arrive in the band after the parts), the evidence views `/<domain>/<protocol>`
  and `/r/<id>/<protocol>` (`evidence::entry_section`, `section`), and the result page's live
  "All evidence" details (`evidence::panel`, only without an entry id).
- The inspector band (`partials/inspect_band.html`, `inspect.rs` `band_html`) renders the full
  spectrum component: label band with flag row above the plate and the axis with the region word
  under it. Board G draws only a 96 px plate with the line names and codes under it
  (`v2-inspector-shell` Decision log, phase 9: "a band-only variant is a proposal").
- SDD `specs/sdd/v2.md`: §3.7 (six shapes, data never presentation, no module ships templates),
  §6.1 (evidence view order, anchors `ev-<block id>`, feeds line), §8.3 (one macro and one
  stylesheet per component), §8.4 (islands compute no layout, hold no verdict), R5 (views compute
  no verdict).
- Board G (private canvas `G.dc.html`): frame, the band, h2 sections (20 px, 650, -0.01em) over
  cards (`surface`, 1 px `line` border, radius 2, padding 18), the certificate facts grid (label
  12 px muted over a mono 13 px value of weight 700; `repeat(auto-fill, minmax(240px, 1fr))`, gap
  14 px 28 px) and the "Protocols and features" cells (a status bar beside the name). No board
  for the HTTP inspector: its parts follow G's section style.
- Gate rules (CLAUDE.md, `frontend-rules.md`, `design-rules.md`): page tests assert components by
  markup; islands write no style; identifier-valued CSS properties go into
  `IDENTIFIER_PROPERTIES`; no `Router` word in `crates/web/src` outside `entries/`, `pages/`,
  `api/`; a component's class is defined only in its own stylesheet.
- Test data: `example.com` and its subdomains, `cdn.example.com` sources, documentation addresses.

## Requirements

### Protocol views (shared seam)

1. **Protocol views.** `crates/web/src/pages/views/` holds one module per protocol (HTTP filled
   here; DNS, TLS, email, IP present and empty) and one entry function that, given a block, the
   run's check statuses, the value decoration and the block's feeds, returns the rendered view
   fragment when a view claims the block and nothing otherwise. Every place that renders blocks
   (the inspector parts, the evidence views, the result page's live "All evidence" details) asks
   it per block, in the run's block order, and renders an unclaimed block through the generic
   evidence block. `crates/web/src/evidence.rs` still names no module, block or key. SDD §6.1 is
   amended in the same commit. (theme F; reused by v2-dns-ui, v2-tls-ui, v2-ip-ui)
2. **Anchors and feeds kept.** A view fragment is a `section` carrying `id="ev-<block id>"` and the
   block's "Feeds lines" line when it feeds any, so check-row links and the result page's evidence
   links land on it. (§6.1)
3. **Nothing lost.** A view claims a block only when every key the block holds is one the view
   renders; a block with another key (an older stored entry) is not claimed and renders through
   the generic block. A test per view feeds a fixture block holding every registered key and
   asserts each value's display text appears in the fragment. (inventory rule "lose nothing")
4. **Checks reach the inspector's views.** In the inspector the checks arrive with the band, after
   the parts; when the band ends with checks, every part that holds a claimed block is streamed
   again, so its views carry the check tones. A shared run renders with its stored checks. The
   same run renders the same view markup in the inspector, the evidence view and the live
   "All evidence" details.
5. **Facts grid component.** A new component `facts_grid` (macro `templates/components/
   facts_grid.html`, stylesheet `static/facts_grid.css`, root class `facts-grid`; the component
   lists in `frontend-rules.md`, `component_files.rs` and `components.rs` amended): cells of a
   label (12 px sans, muted) over a value (mono 13 px, weight 700, wrapping anywhere), laid out
   `repeat(auto-fill, minmax(240px, 1fr))` with gap 14 px 28 px as board G's certificate facts;
   a cell may carry a tone (`data-tone` pass, warn, fail) shown as a 3 px bar in that status
   colour at the cell's left, and a value that is a list of chips (mono, 1 px line border, radius
   2). Values render through the evidence cell (links and badges kept).

### Generic value display

6. **Display values.** The evidence block (and every view) shows values through a display
   formatter, while `format_value` stays raw for exports, compact tables, links and explore:
   a `duration` below 1 000 ms reads `<n> ms`, below 60 s `<s.s> s`, else `<m> min <s> s`; a `bool`
   reads `yes` or `no`; a `time` reads `YYYY-MM-DD HH:MM UTC` inside a `time` element whose
   `datetime` holds the stored RFC 3339 value. A fields key ending `_ms` whose value is a duration
   is shown without the suffix. (4.12, 4.29 "localised")
7. **Repeat suffix.** A table row's key cell or a fields key of the form `<key>#<n>` (n decimal
   digits, SDD §3.7) displays as `<key>`; the full key stays in a `data-key` attribute and in the
   exports. (4.32, finding "set-cookie")

### HTTP views

8. **CSP view.** `http.csp` renders as an h2-style section "Content-Security-Policy": a mode line
   "Enforced" or "Report-Only" (Report-Only in the warn tone with the sentence "Browsers report
   violations but enforce nothing."), "<n> issues" from `issue_count` ("1 issue" for one), the
   root issues as a list, and a table with one row per directive in policy order: the directive
   (mono, weight 600), its sources each as its own mono element wrapping between sources, and its
   issues under the sources in the tone of the `http.csp` check (warn when the check is unknown).
   A wide table scrolls inside its section. (4.26)
9. **HSTS view.** `http.hsts` renders as a facts grid "Strict-Transport-Security": max-age as the
   largest whole unit with the seconds after it ("1 year (31536000 s)", "1 day (86400 s)",
   "1 h (3600 s)", "42 s"; units year of 365 days, day, h, min), "not parsed" without `max_age`;
   includeSubDomains and preload `yes` or `no`. Each cell's tone is the `http.hsts` check's status;
   no tone when the check is unknown. (4.23)
10. **Server view.** `http.server` renders as a facts grid "Server": final URL, status, HTTP
    version, address (with the shell's links and badges), organisation, network type and error
    kind, each only when present, and under the grid the muted line "Inspection took <d>", <d> the
    display duration of `inspection_ms`, no colour. A stored `response_ms` is not claimed (generic
    block, labelled "response"). (4.12, operator decision)
11. **Caching view.** `http.caching` renders as a facts grid "Caching": Cache-Control directives
    (chips in received order), max-age (as requirement 9), ETag (the value; a weak ETag adds the
    word "weak"), Last-Modified (requirement 6), Vary (chips), Age (as requirement 9). An absent
    fact reads "not set"; an empty block reads the one line "No caching headers". (4.29)
12. **CDN view.** `http.cdn` renders as a facts grid "CDN": provider, cache status (when
    received), indicators (header names as chips). Without the block nothing renders. (4.30)
13. **Isolation view.** `http.isolation` renders as a facts grid "Cross-origin isolation" with
    three cells: "Cross-Origin-Opener-Policy" shows its value or "missing" with the tone of the
    `http.coop` check (no tone when unknown), as any graded fact; "Cross-Origin-Embedder-Policy"
    and "Cross-Origin-Resource-Policy" show their value and the word "present", or "missing",
    with no tone, followed by the muted note "COEP and CORP are not part of the grade." (4.25,
    operator decision)

### Inspector band

14. **Slim band.** The inspector band renders the spectrum component's plate only: no label band
    (flag row) above it and no axis or region word under it; the plate is 96 px tall as board G
    draws it; the line names and codes under it and the caption stay. The result page's spectrum
    is unchanged. Built here for every inspector. (operator decision)

### Acceptance

15. **No JavaScript.** Every view is server-rendered and complete without JavaScript; no island is
    added or changed. (S7)
16. **Exports.** The exports (Markdown, CSV, JSON) keep the stored keys and raw values, repeat
    suffixes included; the views change only the page. (shell requirement 17)
17. **Manifest rows.** The private manifest gains the rows that waited for the canvas v45 export
    (v2-result-polish `report.md`): the grade descriptor (typography font-size and font-family,
    colour) on the F family and M, MDark, and the "All evidence →" link (typography font-size) on
    the F family and M, MDark.
18. **Design gate.** `just design-check` exits 0, and the HTTP inspector at 1280 px and 390 px,
    light and dark, is read beside board G by a reader with no part in the build: frame, band,
    section headings, facts grid and tone bars follow G; every finding is a `report.md` row and is
    closed. At 390 px no HTTP page scrolls sideways. (design-rules; "no board; follows G's frame")

## Phase 1 — Protocol views and value display

**Depends on:** `v2-inspector-shell` merged
**Requirements:** 1, 2, 3, 4, 5, 6, 7

The `pages/views` module with the entry function and empty per-protocol views, its use in the
inspector parts (with the re-stream at band end), the evidence views and the live details, the
`facts_grid` component, the display formatter and repeat suffix in the evidence block; SDD §6.1
and §8.3 and `frontend-rules.md` amended. A test-only view (or the HSTS view, if built here) proves
the seam.

### Test Scenarios

GIVEN a run whose `http.hsts` a view claims WHEN `/r/<id>/http` renders THEN the block appears once, as the view fragment, carrying `id="ev-http.hsts"` and its feeds line.
GIVEN the same run WHEN the inspector's shared run, the evidence view and the live "All evidence" details render THEN the claimed fragment's markup is the same in all three.
GIVEN a live HTTP inspector run WHEN the band ends with checks THEN the stream carries the part again after the band event and the re-sent part holds the view fragment with the check tone.
GIVEN `crates/web/src/evidence.rs` WHEN scanned THEN it names no block prefix.
GIVEN a stored `http.hsts` with a key the view does not know WHEN rendered THEN the block renders through the generic evidence block.
GIVEN a duration 77 ms WHEN shown THEN it reads "77 ms"; GIVEN 1234 ms THEN "1.2 s"; GIVEN 125000 ms THEN "2 min 5 s".
GIVEN a fields key `response_ms` with duration 77 WHEN the generic block renders THEN the key reads "response" and the value "77 ms".
GIVEN a `time` 2024-01-01T00:00:00Z WHEN rendered THEN it reads "2024-01-01 00:00 UTC" inside a `time` element with `datetime="2024-01-01T00:00:00Z"`.
GIVEN a `bool` true WHEN the generic block renders THEN it reads "yes".
GIVEN `http.headers` rows `set-cookie` and `set-cookie#2` WHEN rendered THEN both key cells read `set-cookie` and the second carries `data-key="set-cookie#2"`.
GIVEN the same run WHEN JSON and CSV are exported THEN the row key `set-cookie#2` and the raw values stay.
GIVEN the `facts_grid` macro WHEN rendered with three cells, one with tone warn THEN its root class is `facts-grid`, it holds three labels and values, the warn cell carries `data-tone="warn"`, and `static/facts_grid.css` defines `.facts-grid`.

## Phase 2 — CSP, HSTS and server views

**Depends on:** Phase 1
**Requirements:** 8, 9, 10, 15

### Test Scenarios

GIVEN the enforced CSP `default-src 'self'; script-src 'self' 'unsafe-inline' https://cdn.example.com` WHEN the view renders THEN the mode reads "Enforced", it shows "2 issues", the root issue about the missing base-uri, and rows default-src then script-src, script-src with three source elements and its 'unsafe-inline' issue.
GIVEN a report-only policy WHEN rendered THEN the mode reads "Report-Only" with `data-tone="warn"` and "Browsers report violations but enforce nothing."
GIVEN a policy of 4 kB WHEN rendered THEN each source is its own element and the table sits in a container that scrolls horizontally.
GIVEN no CSP header WHEN the HTTP evidence renders THEN no CSP view exists.
GIVEN `max-age=31536000; includeSubDomains` and the `http.hsts` check passing WHEN rendered THEN the grid reads "1 year (31536000 s)", includeSubDomains "yes", preload "no", each cell with `data-tone="pass"`.
GIVEN `http.hsts` without `max_age` WHEN rendered THEN max-age reads "not parsed".
GIVEN `http.server` with `inspection_ms` 1234 WHEN rendered THEN the grid shows the final URL, status and HTTP version and the line reads "Inspection took 1.2 s".
GIVEN `http.server` with an address WHEN rendered in the inspector THEN the address keeps the shell's link.
GIVEN the CSP, HSTS and server fixtures with every registered key WHEN the nothing-lost test runs THEN every value's display text appears in the fragment.

## Phase 3 — Caching, CDN and isolation views

**Depends on:** Phase 1
**Requirements:** 11, 12, 13, 16

### Test Scenarios

GIVEN Cache-Control `public, max-age=3600, must-revalidate`, ETag `W/"abc"`, Last-Modified 2024-01-01, Vary `Accept-Encoding, Accept-Language`, Age 42 WHEN the caching view renders THEN it shows chips public and must-revalidate, max-age "1 h (3600 s)", ETag `W/"abc"` with "weak", "2024-01-01 00:00 UTC", two Vary chips and Age "42 s".
GIVEN `http.caching` with only an ETag WHEN rendered THEN Last-Modified, Vary and Age read "not set".
GIVEN an empty `http.caching` WHEN rendered THEN one line reads "No caching headers".
GIVEN `http.cdn` with provider Cloudflare, cache status HIT and two indicators WHEN rendered THEN the grid shows all three facts; GIVEN no block THEN no CDN view.
GIVEN `http.isolation` with coop `same-origin`, two missing and the `http.coop` check passing WHEN rendered THEN COOP shows "same-origin" with `data-tone="pass"`, COEP and CORP show "missing" without a tone, and the note "COEP and CORP are not part of the grade." follows.
GIVEN `http.isolation` with corp `same-site` WHEN rendered THEN CORP shows "same-site" and "present".
GIVEN the caching, CDN and isolation fixtures with every registered key WHEN the nothing-lost test runs THEN every value's display text appears in the fragment.
GIVEN an HTTP run WHEN JSON is exported THEN `http.caching` keeps `max_age` 3600 and `etag_weak` true.

## Phase 4 — Slim band, manifest rows and design gate

**Depends on:** Phases 1–3
**Requirements:** 14, 17, 18

The slim band; the manifest rows; `just design-check` on the local stack; screenshots of
`/inspect/http` (a finished run of a fixture with every block) and `/r/<id>/http` at 1280 px and
390 px, both themes; the reader's pass beside board G; results under `### Behavioural
verification` in `report.md`.

### Test Scenarios

GIVEN a finished inspector run WHEN the band renders THEN it holds the spectrum plate and the line names, and no `spectrum__label-band`, `spectrum__axis` or `spectrum__region-label`.
GIVEN the result page WHEN it renders THEN its spectrum still holds the label band and the axis.
GIVEN the band stylesheet WHEN read THEN the slim plate is 96 px tall.
GIVEN the manifest WHEN `just design-check` runs THEN it exits 0 with the descriptor and "All evidence →" rows measured.
GIVEN the screenshots WHEN read beside board G THEN every finding is a `report.md` row and none stays open.
GIVEN `/inspect/http` at 390 px with the 4 kB policy WHEN measured THEN the document is not wider than the viewport.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate for features 0–7
  (operator, 2026-10-19).
- Inspector band is a slim variant as board G draws it: one protocol's lines with their names
  below, no flag row and no region word; built here (first inspector UI half) and reused by DNS,
  TLS, IP (operator, 2026-10-20).
- Inspection duration shown neutrally as "Inspection took <n>", no colour, no verdict (operator,
  2026-10-20). COOP is a graded check since profile v4 (#43); its cell renders as a graded fact
  with the check's tone. CORP and COEP stay evidence with present and missing words (operator,
  2026-10-20).
- "Inspection took <d>" uses the generic display duration (`77 ms` below a second, `1.2 s` above),
  not always seconds: a sub-second "0.1 s" would round a measured value away (implementer under
  the operator's autonomy mandate, 2026-10-20).
- COEP and CORP carry no tone bar, only the words, and the note names them as outside the grade:
  a pass or warn bar on an ungraded fact reads as a verdict (R5), and the operator's decision
  names words only; V1's pass and warn ticks are not rebuilt (implementer under the operator's
  autonomy mandate, 2026-10-20).
- Protocol views are page-level functions in `crates/web/src/pages/views/` that read published
  blocks by their registered keys (the API's public contract), asked per block by every renderer:
  board G and the inventory need readings per protocol, SDD §3.7 forbids module templates and the
  source scan keeps the generic renderer module-free; `explore.rs` and `compact.rs` already read
  block names the same way (drafting agent, 2026-10-20).
- A view claims a block only when it renders all of it, pinned by a test, so a reading never hides
  a stored fact; unknown key sets fall back to the generic block (drafting agent, 2026-10-20).
- Value display is generic (durations, times in UTC, booleans, repeat suffix) and separate from
  `format_value`, which stays raw: exports must keep raw values and the compact tables and their
  fixtures are v2-result-polish's. Units of seconds (`max_age`, `age`) are a view's reading, since
  the generic renderer may name no key. Times are shown in UTC as the run titles are (implementer
  under the operator's autonomy mandate, 2026-10-20).
- The inspector re-streams a part when the band brings the checks, rather than holding the parts
  back until the band ends: parts keep appearing as they finish, and the shell's stream order is
  unchanged (implementer under the operator's autonomy mandate, 2026-10-20).
- The HSTS and COOP tones follow their check, not per-field rules: a per-field threshold (max-age
  under a year) would be a second verdict beside the check (drafting agent, 2026-10-20).
- `facts_grid` is a component (users: HTTP views here, TLS certificate and IP card next), the
  rule of three; view sections otherwise use `pages.css` (drafting agent, 2026-10-20).
- Re-check against main 2026-10-20: the shell (#46) renders parts through `part_html` and streams
  the band after them, so requirement 4 was added; the result page's tabs hold compact tables
  since #44, so a view renders in the live "All evidence" details there, not in the tabs; COOP's
  graded check (#43) changed the isolation view (implementer under the operator's autonomy
  mandate, 2026-10-20).
- Board: none for the HTTP inspector; it follows board G's frame. Proposal for the operator: add an
  "HTTP inspector" board (CSP table, caching and isolation cells) to the canvas so later runs of
  `just design-check` measure it (drafting agent, 2026-10-20).

## Open decisions

None.

## Out of scope

- Shell parts, IP badges, export and sharing: `v2-inspector-shell`.
- Rows 4.19, 4.20, 4.24, 4.28, 4.31 (changed, not missing) and the speed colouring of 4.12.
- New HTTP checks or a profile change.
