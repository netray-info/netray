# Plan: v2-site-nav

## Phase 1 — Origin and relative links

## Groups

- G1 (C1, C2, C4, C5–C9): pages::ORIGIN for robots, sitemap, frame, guides, JSON-LD; relative guide links and CTAs.
- G2 (C3, C10): the repository check.

## Plan

- G1: `crates/web/src/pages/mod.rs` (`robots`, `sitemap`, `Frame::new`; `Tool::ALL` crate-visible in `inspect.rs`), `src/pages/guides.rs` (`show`, `index`), `src/pages/jsonld.rs` (`Organization.url`), guide templates (`tool(...)` actions to `/inspect/<tool>`, email field `d`, body links, command snippets via `frame.origin()`), `templates/pages/guide_index.html`.
- G2: `tests/repo/test_no_absolute_apex.sh` (git grep over web templates, src, islands/src; the ORIGIN line exempt).

## Phase 2 — Header menu, phone sheet, tools page, privacy, taglines

## Groups

- G1 (C1, C2, C6, C7, C9, C10): header menu markup, current marking, sheet CSS, the island.
- G2 (C3, C8, C11, C12): `/tools` page and redirect.
- G3 (C4, C5, C13, C14): privacy in the frame, the email tagline.

## Plan

- G1: `templates/base.html` (nav), `src/pages/mod.rs` (`Frame.nav`, `INSPECTOR_MENU`), `src/pages/inspect.rs` (frame nav), `templates/pages/inspector.html`, `static/pages.css`, `islands/src/menu.ts`, `islands/src/main.ts`.
- G2: `src/pages/tools.rs`, `templates/pages/tools.html`, `src/pages/mod.rs` (`router`), SDD §7.3 row.
- G3: `src/entries/render.rs` (`privacy`), `templates/pages/privacy.html`, `src/entries/routes.rs` (secure headers), `src/pages/inspect.rs` (`tagline`).

## Phase 3 — Footer

## Groups

- G1 (C1–C6): footer markup and styles.

## Plan

- G1: `templates/base.html` (footer), `src/pages/mod.rs` (`VERSION`), `static/pages.css` (desktop and phone rules), `src/entries/render.rs` (the gone page's inline sheet).

## Phase 4 — Home grade legend

## Groups

- G1 (C1–C3): legend rows and the inspector line.

## Plan

- G1: `src/grade_legend.rs` (`legend`), `src/pages/result/mod.rs` (`IdlePage`), `templates/pages/idle.html`, `static/pages.css`.

## Phase 5 — DKIM selector inputs and copy

## Groups

- G1 (C1–C7): one group; the files overlap.

## Plan

- G1: `templates/components/query_input.html` (`selectors` parameter), `static/query_input.css`, `templates/pages/idle.html`, `templates/partials/selector_form.html`, `templates/partials/result_evidence.html` (sibling `selector-panel`), `src/pages/result/mod.rs` (`selector_form`, `with_selectors`, `email_fragment`), `src/pages/result/run.rs` (live selectors), `src/pages/errors.rs` (`selectors_refused_page`), `src/pages/evidence.rs` and `templates/pages/evidence.html`, `static/pages.css`.

## Phase 6 — IDN display

## Groups

- G1 (C1–C6): one group.

## Plan

- G1: `src/idn.rs` (`unicode`, `title`), `src/lib.rs`, `Cargo.toml` (`idna`), `templates/partials/result_header.html`, `templates/pages/result.html` (`data-display`), `templates/pages/history.html`, `templates/pages/evidence.html`, `templates/pages/entry.html`, titles in `src/pages/result/mod.rs`, `src/pages/evidence.rs`, `src/entries/render.rs`, `static/pages.css`, `islands/src/recent.ts`.
