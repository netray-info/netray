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
