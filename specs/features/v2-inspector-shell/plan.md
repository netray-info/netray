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

## Phase 3 — URL, sharing, history and autocomplete

### Groups

G1 (server): C3, C4, C9, C10, C12, C13, C14, C15, C16, C17 and the server halves of C2, C6, C21 ·
G2 (island, `shell.ts` only): C1, C2, C5, C6, C7, C8, C11, C18, C19, C20, C21. G1 first.

### Plan

G1:
1. `crates/dns/src/parser.rs`: `pub const FLAGS` (udp tcp tls https dnssec short norecurse check
   trace compare auth); `parse_flag` warns for a name outside it.
2. `crates/dns/src/api/meta.rs`: `ServerInfo::name()`.
3. `crates/dns/src/jobs/results.rs`: `DnsCachedJob::blocks()` over `blocks::*_blocks`.
4. `crates/web/Cargo.toml`: `moka` (workspace, `future`).
5. `crates/web/src/pages/inspect/shared.rs` (new submodule): `SharedRuns` (moka, 1 000 entries,
   1 h, LRU; `with_ttl` for tests) of `SharedRun` data (tool, input, target, start, link, grade,
   parts, band).
6. `crates/web/src/evidence.rs`: `shared_run_title(Option<i64>)`.
7. `templates/partials/inspect_share.html` (new): the share slot with `data-share-key`/`-url`.
8. `templates/partials/inspect_start.html`: share slot in the head; band slot `Pending`/`Done`;
   parts pending or rendered, so one template renders the live start and the finished run.
9. `components/query_input.html`: the card box is a `div`; input gets the listbox ARIA; empty
   `ul#inspector-suggest`.
10. `components/inspector_shell.html`: `data-suggest*` on the root for DNS; a server-rendered result
    or the idle region with an optional notice.
11. `src/pages/inspect.rs`: `JobRequest::share_key`; `InspectDeps` holds `SharedRuns` and an optional
    `DnsResultCache` (`with_shared_runs`, `with_dns_results`); `suggest(tool)`; renderers factored
    (`part_html`, `band_region_html`, `finished_html`, `shared_html`); `show` serves `?r=` (shared
    run, DNS fallback, 404 notice); `run` picks the key (none for the caller's own address);
    `stream_job` stores the finished run and sends the share fragment before `end`.
12. `src/pages/mod.rs`: re-export `SharedRuns`.
13. `src/api/jobs.rs`: `dns_part` stores the finished collected job in `DnsResultCache` under the
    share key.
14. `static/inspector_shell.css`: share, suggest list, options, notice.
15. `templates/pages/privacy.html`: the one-hour sharing paragraph.
16. `crates/lens/src/lib.rs`: `.with_dns_results(api_state.dns_results().clone())`.
17. `specs/sdd/v2.md`: §6.2 sharing bullet, §7.3 `?r=` row, §4.7 privacy sentence.

G2: `crates/web/islands/src/shell.ts`: pushState on submit, popstate restore and re-run (a
`restoring` flag), replaceState to the share URL on stream end, copy link (clipboard or share sheet
on a coarse pointer), history in `localStorage` (20, distinct, newest first, try/catch), the
listbox with keys, and the DNS autocomplete over the root's `data-suggest*` lists.

## Phase 4 — Help and keyboard

### Groups

G1 (server): C4 and the server halves of C1, C3 · G2 (island): C1, C2, C3, C5–C9.

### Plan

G1: `src/pages/inspect.rs` — `Tool::what`, `Tool::syntax`, `Tool::guide`; `HelpView` on
`ShellView` (DNS lists split from the `SuggestView` strings, so the tables are read once);
`components/inspector_shell.html` — the closed `dialog#inspector-help` after the result region
(examples, static key table, DNS lists behind data checks, guide link, close form);
`partials/inspect_tabs.html` — the hidden "?" button inside the nav (the tabs fragment replaces the
nav); `static/inspector_shell.css` — dialog, backdrop, keys, lists (tokens only).

G2: `islands/src/shell.ts` — unhide and open/close through the delegated click listener, focus
back to "?" on `close`; one document keydown listener per mount (guarded by `isConnected`,
ignoring text fields): `/`, Ctrl/Meta+L, `r`, `j`/`k` over check rows and evidence blocks, Enter on
a check row to its guide, `h`/`l` over the tabs, `?`, Esc.
