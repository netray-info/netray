# Plan: v2-site-nav

## Phase 1 — Origin and relative links

## Groups

- G1 (C1, C2, C4, C5–C9): pages::ORIGIN for robots, sitemap, frame, guides, JSON-LD; relative guide links and CTAs.
- G2 (C3, C10): the repository check.

## Plan

- G1: `crates/web/src/pages/mod.rs` (`robots`, `sitemap`, `Frame::new`; `Tool::ALL` crate-visible in `inspect.rs`), `src/pages/guides.rs` (`show`, `index`), `src/pages/jsonld.rs` (`Organization.url`), guide templates (`tool(...)` actions to `/inspect/<tool>`, email field `d`, body links, command snippets via `frame.origin()`), `templates/pages/guide_index.html`.
- G2: `tests/repo/test_no_absolute_apex.sh` (git grep over web templates, src, islands/src; the ORIGIN line exempt).
