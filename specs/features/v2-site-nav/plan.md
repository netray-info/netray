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
