# Plan: v2-inspector-shell

## Phase 1 — Shell frame and form

### Groups

G1: C15 · G2: C1–C14, C16–C21 (everything hangs on inspect.rs and the inspector templates).

### Plan

G1 (islands): `crates/web/islands/src/shell.ts` new, `export function shell(el)`: clear button
visibility on `input`, clear click empties and focuses, delegated chip click fills `data-example` and
`requestSubmit()`s, attributes only. `main.ts` registers `"shell"`.

G2:
1. `crates/web/tokens.json`: font sizes `inspector` 21px, `input` 16px, `target` 32px (the gate only
   accepts `var(--font-size-*)`).
2. `templates/base.html`: nested blocks `header_inspector` (empty), `header_tagline`, `nav_inspectors`
   whose defaults render today's markup.
3. `components/query_input.html`: macro argument `card = false` adds `query-input--card` and the clear
   button (`hidden` without a value); no new `QueryInputView` field.
4. `static/query_input.css`: card padding 18px on surface, field 44px on bg at `--font-size-input`,
   clear button, card submit 44px weight 600.
5. `components/inspector_shell.html` (new): root `.inspector-shell[data-island=shell]`, tabs, card form,
   view slot, examples, noscript, `#inspect-result` with the hidden h1. No branch on a tool.
6. `partials/inspect_dns_builder.html` (new): the DNS builder moved out of `pages/inspector.html`.
7. `pages/inspector.html`: overrides the three header blocks, calls the shell macro.
8. `partials/inspect_start.html`: run head (run title, back link with grade), h1 target, part sections
   with h2, band div as today.
9. `partials/inspect_part.html`: `section.inspector-section` with its h2.
10. `partials/inspect_backlink.html`: deleted.
11. `static/inspector_shell.css` (new): header name, tabs, examples, run head, target, back link,
    section cards and h2.
12. `static/base.css`: `.visually-hidden`.
13. `static/pages.css`: `.page-nav a[aria-current="page"]` ink, weight 600.
14. `src/pages/assets.rs`: bundle `inspector_shell.css`.
15. `src/evidence.rs`: `live_run_title(now)` parts live / `<date> UTC` / not recorded.
16. `src/pages/inspect.rs`: `ShellView` per tool (name, tagline, label, examples, tabs, slot),
    `tabs()` carry-over via `target_host` and `registrable_domain`, `target_host` maps with
    `netray_common::domain::validate` (IDNA), `InspectDeps::with_history`, grade read with
    `HistoryStore::head` in `run`, `StartFragment` with title, target, link, grade, part labels,
    `target_heading()` (TLS gets `:443`).
17. `crates/lens/src/lib.rs`: `.with_history(state.history_store.clone())`.
18. `specs/sdd/v2.md`: the live run title and the back link's grade.

Existing tests expected to change (orchestrator, `ADLC-Test-Change`): `inspect_page.rs` (idle region
holds the hidden h1), `inspect_review.rs` and `inspect_review_second.rs` and `inspect_backlink.rs`
(the back link moves from the page into the start fragment; tabs add carried links),
`shell_headings.rs` (20 px is a token, the gate refuses a font-size literal).

## Phase 2 — Band position and live check list

### Groups

G1: C3, C7 · G2: C1, C2, C4, C5, C6, C8, C9 (G2 depends on G1).

### Plan

G1: `crates/web/src/evidence.rs` — `build_section` sets the row's `evidence_href` to its first block
link (the link sits inside the check-row article in both views); new
`live_rows(checks, blocks, protocol)` wraps the live checks and blocks in a transient `Run` and
returns `build_section(..).rows` with same-page links.

G2: `partials/inspect_start.html` — the band placeholder moves directly under the h1.
`src/pages/inspect.rs` — the stream state collects every part's blocks; `band_event` builds a band
view: spectrum, lines (catalogue short and `line_code`), caption (tag and positions from the first
and last code), rows from `evidence::live_rows`, heading counts. `partials/inspect_band.html` —
`figure.inspector-band` with lines and caption, then `section.inspector-checks` with the check-row
macro and the block-link loop. `static/inspector_shell.css` — band line and checks rules (11 px and
10 px tokens).

Heading wording: "<n> lines need attention" ("1 line needs attention") with "<f> fail · <w> warn";
"All <n> lines pass" when every line passes; "No line needs attention" with "<k> not measured"
when nothing warns or fails but some lines are unmeasured, not applicable or not tested.
