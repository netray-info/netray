# Plan: v2-result-polish

## Phase 1 — Compact evidence tabs

### Groups

- G1 (C1–C22): one group; every criterion touches `crates/web/src/evidence.rs` (the panel) and `templates/partials/result_evidence.html`.

### Plan

- `crates/web/src/compact.rs` (new, `pub mod compact` in `lib.rs`): `CompactView { caption, columns, rows: Vec<Vec<CompactCell>>, sources: Vec<BlockId> }`, `CompactCell { text, title: Option<String>, strong }`; `pub fn compact(run: &Run, protocol: Protocol) -> Option<CompactView>` with one builder per protocol (`tls`, `dns`, `http`, `email`, `ip`) over released block ids and keys; `cell()` applies requirement 4's truncation.
- `crates/web/src/evidence.rs` `panel`: `PanelView` gains `compact: Option<CompactView>` and `entry: bool` (whether `all_href` exists); feeds come from the compact's sources when it exists; `blocks` stay for the disclosure of a run without entry; `has_table` true with a compact.
- `templates/partials/result_evidence.html`: caption, table, swipe note, links; blocks only inside `<details class="evidence-tabs__blocks">` when `all_href` is none.
- `static/pages.css`: `.evidence-tabs__caption`, `.evidence-tabs__compact` rules, phone rules in the 480 px block, `.evidence-tabs__blocks`.
- `tokens.json`: `font.weight.medium` 500.
- Protected test `crates/web/tests/it/fidelity_evidence_tabs.rs` c1_c2 asserted every block in a stored panel: changed under `ADLC-Test-Change` to the compact table.
