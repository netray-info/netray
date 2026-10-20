# Plan: v2-http-ui

## Phase 1 — Protocol views and value display

### Groups

G1: C5, C19 · G2: C1, C2, C3, C4, C6, C7, C8, C9, C10, C11, C12, C13, C14, C15, C16, C17, C18.
G2 depends on G1 (the HSTS view renders `facts_grid`; the evidence cell reads the new `Cell`
fields). Both groups compile into the one `it` test binary, so they are coded in one pass. C11 is
already green (`evidence_render_source.rs`).

### Plan

G1
1. `components.rs`: `Cell` gains `datetime: Option<String>` (raw RFC 3339 of a time) and
   `key: Option<String>` (full `<key>#<n>`); `FactCell { label, values: Vec<Cell>, chips, tone:
   Option<Tone> }`, `FactsGridView { cells }`; `component!(facts_grid, …)`; doc "eleven".
2. `templates/components/facts_grid.html`: `{% macro facts_grid(view) %}`, `facts-grid`,
   `facts-grid__cell` with `data-tone` when set, `__label`, `__value`, `__chip`; values through
   `evidence_cell` imported from `evidence_block.html`.
3. `static/facts_grid.css`: `repeat(auto-fill, minmax(240px, 1fr))`, gap 14px 28px; label 12 px
   muted; value mono 13 px 700 `overflow-wrap: anywhere`; 3 px tone bar; chip hair border, radius.
4. `pages/assets.rs` `APP_CSS` includes `facts_grid.css`.
5. `gate.rs` `COMPONENTS` gains `facts_grid`.
6. Tests changed (ADLC-Test-Change): `component_files.rs` NAMES, `component_gate.rs` COMPONENTS,
   `facts_grid.rs` Cell literal (`datetime: None, key: None`).
7. `frontend-rules.md` component list; 8. SDD §8.3 row for `facts_grid`.

G2
1. `evidence.rs`: `display_value` beside raw `format_value`; `cell()` uses it and sets
   `datetime` for a time; `fields()` drops `_ms` on durations and splits `<k>#<digits>` into
   `key`; the key column of a table does the same; `flatten` uses `display_value`; the split is a
   `pub(crate)` helper. No block prefix in the source.
2. `evidence_block.html`: `<time datetime>` in `evidence_cell`; `data-key` on `<td>`/`<dt>`.
3. `evidence_render_values.rs` `fields_block_renders_typed_values_in_dd` expects display values
   (ADLC-Test-Change).
4. `pages/views/mod.rs`: `ViewContext { statuses, decorate, feeds }`, `render(block, ctx)` asks
   each protocol module; claim helper (Fields, every key known); `tone(ctx, check)`; frame
   template `partials/protocol_view.html`.
5. `pages/views/{http,dns,tls,email,ip}.rs`: empty except HTTP's HSTS view (title
   "Strict-Transport-Security", max-age in the largest whole unit plus " (<n> s)", "not parsed",
   yes/no, tone from `http.hsts`).
6. `partials/protocol_view.html`: `section.protocol-view#ev-<id>[data-block]`, `h3`, body,
   `protocol-view__feeds` "Feeds lines:" when any.
7. `pages/mod.rs` `pub mod views;`.
8. `evidence.rs` seam: `BlockSlot { block, view }` with `Deref`; `block_feeds` pulled out of
   `build_section`; `EvidenceView.blocks`, `PanelView.blocks` become slots; both ask
   `views::render` with statuses and feeds.
9. `evidence_section.html`, `result_evidence.html` render a slot's view or the generic block.
10. `inspect.rs`: `part_html`/`part_event` take checks; `shared_html` passes the stored band's
    checks; at band end with checks, every successful part holding a claimed block is re-sent.
11. `pages.css`: `.protocol-view` card per board G, `__title`, `__feeds`.
12. SDD §6.1 amended for protocol views.
13. `islands/src/shell.ts` j/k navigation selects `.protocol-view` beside `.evidence-block`, so a
    claimed block stays reachable (one selector, logged in the Decision log).
