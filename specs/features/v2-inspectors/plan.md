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

## Phase 3 — Explore-live links

## Groups

G1: C1–C7 — one new file, `crates/web/src/explore.rs`.

## Plan

### G1

1. `crates/web/Cargo.toml`: `percent-encoding = { workspace = true }` (Cargo.lock follows).
2. `crates/web/src/explore.rs` (new): `ExploreLink`; encode sets `COMPONENT` (non-alphanumeric minus `-._~`, for `url`) and `QUERY` (`COMPONENT` minus `:`, for `q`, `h`, `ip`); `view_link` per protocol; `block_links` matching module/name and shape (`dns.*` records, `tls.addresses`/`email.mx`/`ip.addresses` table rows by column name, `email.dmarc`/`email.mta_sts` fixed TXT names, else none); `view_links` = view link then the protocol's blocks. IPv6 bracketed for `h` only; `email.mx` trailing dot dropped. Values through `evidence::format_value`.
3. `crates/web/src/lib.rs`: `pub mod explore;`.

Gotcha: no `Router` word in explore.rs (`test_web_crate.sh`).

## Phase 4 — Evidence view routes

## Groups

G1: C2, C3, C4, C5, C10 (C6's netray-web half goes green with it) · G2: C1, C6, C7, C8 · C9 already_implemented: G2 leaves lens's snapshot arm untouched. G2 depends on G1 (`evidence_router`).

## Plan

### G1

1. `crates/web/src/pages/errors.rs`: `normalise` `pub(super)`, reused for the domain segment (trim, lowercase, one trailing dot dropped, must contain a dot).
2. `crates/web/templates/pages/evidence.html` (new): extends `base.html`; entry branch renders the pre-rendered section (`render_section(&entry_section(..))`) and one `<a>` per `explore::view_links` link (class `evidence-view__explore`, not a gate component root); missing branch links `/<domain>`. No codename, no "lens".
3. `crates/web/src/pages/evidence.rs` (new): `EvidenceDeps { history, v1 }`, `evidence_router`; five literal routes `/{domain}/{dns,tls,http,email,ip}` and `/r/{id}/{segment}`, no fallback. Latest: normalise → `HistoryStore::latest` → `Err` 500, `Some` 200 (`no-cache`), `None` 404 (`no-store`) linking `/<domain>`. Entry: segment one of five else `not_found()`; `well_formed` else `not_found()`; `get` → `Err` 500, `Some` 200 (`public, max-age=3600`), `None` → v1 `Some(_)` → `not_found()`, else 410 `entries::render::gone()`. No store write, no run.
4. `crates/web/src/pages/mod.rs`: `mod evidence; pub use evidence::{EvidenceDeps, evidence_router};`.

### G2

1. `crates/web/src/entries/routes.rs` `router`: `/r/{id}/{*rest}` → `/r/{id}/{segment}/{*rest}`; doc comment updated.
2. `crates/lens/src/lib.rs` `app`: in the `[history] enabled` arm compute the V1 lookup once and merge `netray_web::pages::evidence_router(EvidenceDeps { history, v1 })` beside `entries::router`; snapshot and empty arms unchanged.

Protected tests moved to three segments under `ADLC-Test-Change` in the test commit: `crates/web/tests/entries_router.rs` (c16), `crates/web/tests/entries_og.rs` (c7), `crates/lens/tests/entries_mount.rs` (`history_enabled_mounts_entries_router`).

Risk noted: with lens mounted, a dotless first segment (`/api/dns`) now matches `/{domain}/dns` and answers the 404 page instead of the SPA shell; requirement 6 gives these five literal second segments to this router.

### Review fix

`crates/common/src/domain.rs` (`validate`, lens's `validate_domain` rule; `crates/lens/src/input.rs` wraps it) replaces `normalise` in the latest handler; both handlers answer the 404 page on a `PathRejection`; a 5xx V1 answer is `500`; the no-entry href is percent-encoded with `errors::LOCATION_SET`; `entries::router` adds `/r/{id}/{segment}/`.
