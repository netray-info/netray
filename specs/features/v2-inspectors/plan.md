# Plan: v2-inspectors

## Phase 1 — One renderer over the six shapes

## Groups

G1: C1, C2, C3, C4, C5, C6, C7 — every criterion changes `templates/components/evidence_block.html` (key mark, tree depth), so they form one group.

## Plan

### G1

1. `crates/web/src/components.rs`: `TableColumn { name, key }` with `From<&str>` (`key: false`); `EvidenceShape::Table.columns: Vec<TableColumn>`; `MAX_TREE_DEPTH = 7` and `TreeItem::depth_attr()` clamping in the macro for every caller; `RecordRow.ttl: String` (a formatted `Value`, lossless; the protected `tests/components.rs` row changes under `ADLC-Test-Change`).
2. `crates/web/src/evidence.rs` (new): `format_value(&Value)` (text/time verbatim, int/bool Display, duration `<n> ms`, addr Display) and `block_view(&Block) -> EvidenceBlockView`, one `match` on `Shape`; id `ev-<block id>`, feeds empty (Phase 2); tree flattened depth-first, fields `k: v` joined by `, `. No identifier or literal containing `dns.`, `tls.`, `http.`, `email.`, `ip.` (bind addresses as `addr`).
3. `crates/web/src/lib.rs`: `pub mod evidence;`.
4. `templates/components/evidence_block.html`: `<th data-key="true">` on the key column; tree `data-depth="{{ item.depth_attr() }}"`, no `style`.
5. `static/evidence_block.css`: one `.evidence-block__tree-item[data-depth="N"]` rule per depth 0–7 replacing `--evidence-block-depth`.

## Phase 2 — Evidence view model

## Groups

G1: C1–C12 — every criterion goes through `crates/web/src/evidence.rs`.

## Plan

### G1

1. `crates/web/src/entries/view.rs`: `epoch_days_to_ymd` and `status_tone` become `pub(crate)` (one date and tone mapping; `status_tone(None)` is Untested for a check absent from the run).
2. `crates/web/src/evidence.rs`: `entry_run_line`, `incomplete_run_line`, `href`, `EvidenceView`/`EvidenceRow`/`BlockLink`/`SharedLine`, `section` (rows per `catalogue()` entry of `p` in catalogue order, body findings or catalogue `finding` for warn/fail, fix for warn/fail, guide href, links `#ev-<id>` own or `<owner>#ev-<id>` foreign; blocks of `p` in run order with feeds in row order; shared foreign lines deduplicated; reason of a not_applicable section; `run.complete` never read), `render_section` over a private askama template.
3. `crates/web/templates/partials/evidence_section.html`: run line, reason, rows with `<a class="evidence-section__link" href>` links, shared lines `<p class="evidence-section__shared"><a href>`, blocks; macros imported as `templates/pages/entry.html` does; every `{% call %}` closed.
4. Test fix: `evidence_section_rows.rs`/`evidence_section_href.rs` `created_at` constant `1_778_262_240` (2026-05-08 17:44) → `1_791_469_920` (2026-10-08 14:32), the time the test asserts.

Gotchas: no new stylesheet (`entries_gate.rs` pins the CSS set); no `ip.`/`http.`-style substrings in evidence.rs; row link markup puts `class=` before `href=`.
