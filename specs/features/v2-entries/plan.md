# Plan: v2-entries

## Phase 1 — Frame and page templates in netray-web

## Groups

G1: C1–C20. One group: every criterion goes through `crates/web/src/entries/{mod,view,render}.rs` or the page templates; C6 and C19 share the lens re-export.

## Plan

### G1

1. `Cargo.toml` — `netray-web = { path = "crates/web" }` in `[workspace.dependencies]`.
2. `tests/repo/test_spectrum_deps.sh` — (orchestrator, under `ADLC-Test-Change`) `netray-store` leaves the refused set (SDD §3.1 gives `netray-web` `store`); lens and history stay refused; the self-test row "store" becomes a passing case.
3. `crates/web/src/lib.rs` — `pub mod entries;`.
4. `crates/web/src/entries/mod.rs` — `pub mod render; pub mod view;` and re-exports of `EntryPageView`, `StatusCounts`.
5. `crates/web/src/entries/view.rs` — `format_utc_timestamp` (moved verbatim from `crates/lens/src/og/render.rs`), `StatusCounts`, `EntryPageView`, `entry_view(&Entry)` (grade letter and tone, run line, fail-then-warn issues in catalogue order, unknown IDs at the end of their group, counts), `diff_view(&Diff)` (title, summary forms, class words, tones, evidence items, engine note).
6. `crates/web/src/entries/render.rs` — one inline stylesheet (tokens, base.css, ten component sheets) in a `OnceLock`; askama templates `EntryPage`, `GonePage`, `PrivacyPage`; `entry`, `gone`, `privacy`.
7. `crates/web/templates/base.html` — doctype, `lang="en"`, metas, `title`/`meta`/`styles` blocks in the head; `header`, `main`, footer (tagline partial, `/privacy` link), `scripts` in the body.
8. `crates/web/templates/pages/entry.html` — h1, grade, run line, optional diff block, check rows, counts; title and og metas.
9. `crates/web/templates/pages/gone.html` — heading, sentence, query input; no og metas.
10. `crates/web/templates/pages/privacy.html` — §4.7's three statements in prose.
11. `crates/lens/Cargo.toml` — `netray-web = { workspace = true }`.
12. `crates/lens/src/og/render.rs` — `pub use netray_web::entries::view::format_utc_timestamp;` in place of the function and its helpers.

## Phase 2 — The routers in netray-web, `/r/<id>` mounted in lens

## Groups

G1: C3, C4, C5, C8, C10, C11, C12, C14, C16, C17 · G2: C1, C6, C7, C13, C15, C18, C19, C20 · none: C2, C21 (already in place after the test commit), C9 (v2-pages has not landed; the frame merge falls to its rebase).

## Plan

### G1

1. `crates/web/src/entries/routes.rs` (new) — `NOT_FOUND_HTML`, `V1Lookup`, `EntryCard`, `CardRenderer`, `EntriesDeps`; `router(deps)` routes `/r/{id}` → `entry_page`, `/r/{id}/og.png`, `/r/`, `/r/{id}/`, `/r/{id}/{*rest}` → `unmatched` (a static segment beside a catch-all does not conflict in matchit 0.8; Phase 3 swaps only `og.png`'s handler). `entry_page`: rejection or malformed → 404; store error → warn with the ID, 500 JSON, V1 never called; entry → 200 page, `public, max-age=3600`; else V1 `Some(resp)` → resp; else 410 gone page.
2. `crates/web/src/entries/mod.rs` — `mod routes;` and its re-exports.
3. `crates/web/src/lib.rs` — crate doc no longer says no route (and does not name the router type: `test_web_crate.sh` greps it).
4. `specs/sdd/v2.md` §4.6 — requirement 17's paragraph.

### G2

1. `crates/lens/src/routes.rs` — `not_found_html` body from `NOT_FOUND_HTML`; `snapshot_answer(state, id) -> Option<Response>` extracted from `snapshot_handler`; `v1_lookup(state) -> V1Lookup`.
2. `crates/lens/src/og/handler.rs` — `entry_card_renderer(state) -> CardRenderer`, a render-failure stub until Phase 3.
3. `crates/lens/src/lib.rs` — `pub fn app(state) -> Router` from `run_with` steps 4–5; the `/r/` router (entries, else snapshot, else none) merged after `.with_state(state)`, before the layers; `run_with` calls `app`.

## Phase 3 — The entry's own OG card

## Groups

G1: C2 · G2: C5, C6, C7, C8 · G3: C1, C3, C4, C9, C10 (G2 before G3: lens's tests go through the web route; C9 holds already, history off mounts no entries router).

## Plan

### G1

1. `specs/sdd/v2.md` §4.6 — requirement 18's sentence: grade, domain and recording time in the V1 layout, no score, until v2-result-page switches it to `spectrum::layout` (V4.4).

### G2

1. `crates/web/src/entries/view.rs` — `grade_letter` becomes `pub(crate)`.
2. `crates/web/src/entries/routes.rs` — `entry_card` on `/r/{id}/og.png`: malformed 404 `NOT_FOUND_HTML`; store error warn + 500; entry → etag `"<id>-<letter>"`, 304 on `If-None-Match`, else `deps.card` → 200 `image/png`, `public, max-age=3600`, or the renderer's error response; else 410 empty, V1 never called.

### G3

1. `crates/lens/src/og/render.rs` — private `card_svg(.., score_pct: Option<f64>, ..)`; `svg_for_grade` byte-identical through it; `svg_for_entry` with no score text.
2. `crates/lens/src/og/handler.rs` — `entry_card_renderer`: `svg_for_entry(.., DEFAULT_LABEL, ..)` → `svg_to_png`, errors through `render_error_from`, boxed.
