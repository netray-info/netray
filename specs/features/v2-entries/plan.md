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
