# Plan: v2-pages

## Phase 1 — 4a deferrals

### Groups

G1: C1, C4 · G2: C2, C5 · G3: C3, C6, C7, C8. No shared production file, no dependency between them.

### Plan

#### G1
1. `crates/web/build.rs` — in `main`, after the `[data-theme="dark"]` rule and before the phone media block, `block(&mut css, "", "[data-theme=\"light\"]", Some("light"), &light);` (`light` already holds every `:root` value, `none` per `status.glow.*`).
2. `crates/web/src/tokens.rs` — doc comment on `CSS` names the light rule.
3. `specs/sdd/v2.md` — §8.2's build.rs sentence adds the light values under `[data-theme="light"]` after the dark media block (same commit as build.rs).

#### G2
1. `crates/web/static/logo.svg` — root `id="netray-logo"`, selectors `#netray-logo text`, `#netray-logo text, #netray-logo rect` (both outside and inside the dark media block).
2. `crates/web/static/favicon.svg` — root `id="netray-favicon"`, `#netray-favicon path`.
3. `crates/web/static/badge-mark.svg` — root `id="netray-badge-mark"`, `#netray-badge-mark .tile`, `#netray-badge-mark .mark`.
Colours and geometry byte-identical.

#### G3
1. `crates/web/src/gate.rs`:
   - `parse_attrs` takes `askama: &[(usize, usize)]`; in the unquoted-value branch an askama span starting at the value position (only whitespace between `=` and the span start) is the value, and parsing resumes after it.
   - `tags` takes and forwards the spans; `scan_template` passes its `askama` vec; `data_uri_colours` passes `&[]`.
   - `has_src`'s second disjunct (askama src parsed as the next token) becomes dead and goes.
   - The handler message already names the attribute.

## Phase 2 — Islands scaffold and the `stream` island

### Groups

G1: C4, C9 · G2: C1, C10 · G3: C2, C3, C5, C6, C7, C8, C11–C17. Order G1 → G2 → G3 (G3's real-tree gate run scans the new sources; G2 brings esbuild, the build script and the vitest config).

### Plan

#### G1 — `cargo test -p netray-web --test gate`
1. `crates/web/src/gate.rs` `walk`: skip directories named `node_modules`.
2. `crates/web/src/gate.rs` `scan_island`: a `style_writes(code)` helper over the comment-blanked code (`lex(text, true).0`) pushes rule 1 for `.style` not followed by an identifier char, whole-word `cssText`, `setProperty(`, `insertRule(`, `setAttribute(` + optional whitespace + `"style"`/`'style'`. Module and `check` docs name style writes.

#### G2 — stage, then `bash tests/repo/test_islands_package.sh && bash tests/repo/test_frontend_toolchain_majors.sh && bash tests/repo/test_npm_workspaces.sh`
1. `crates/web/islands/package.json`: `@netray-info/islands`, private, `type: module`; `build` = `esbuild src/main.ts --bundle --minify --format=esm --outfile=dist/islands.js`; `test` = `tsc --noEmit && vitest run`; devDependencies esbuild `0.28.2` (exact), typescript `^6.0.0`, vitest `^4.1.11`, happy-dom `^20.14.5`.
2. `crates/web/islands/tsconfig.json`: strict, ESNext, bundler resolution, lib ES2022/DOM/DOM.Iterable, noEmit, isolatedModules, skipLibCheck, `include: ["src"]`; self-contained.
3. `crates/web/islands/vitest.config.ts`: happy-dom environment, `test/**/*.test.ts`.
4. `crates/web/.gitignore`: `/islands/dist/`.
5. Root `package.json` workspaces add `crates/web/islands`; `package-lock.json` via `npm install`.
6. `tests/repo/test_frontend_toolchain_majors.sh` dirs add `crates/web/islands`.
7. `justfile` `check-frontend-dist`: require `crates/web/islands/dist/islands.js`, message keeps `just adlc-setup`.

#### G3 — `npm test -w crates/web/islands && bash tests/repo/test_islands_typecheck.sh && cargo test -p netray-web --test gate real_tree_passes_the_gate`
1. `crates/web/islands/src/main.ts`: `register` (Map), `mount` (root if it matches plus `querySelectorAll("[data-island]")`, a `WeakSet` of mounted elements, unknown names skipped); registers `stream` passing `mount` in (no import cycle); ends with `mount(document)`.
2. `crates/web/islands/src/stream.ts`: `stream(form, mount)` per requirement 28 — URL from `data-stream-url` else the `action` attribute; URL-encoded `FormData` string entries plus `trigger` (submitter named `trigger`, else `data-stream-autostart`); POST with `Accept: text/event-stream`, `credentials: "same-origin"`, an `AbortController` aborting the previous run; submit intercepted; autostart waits out `document.prerendering` via `prerenderingchange`; SSE parse over a streaming `TextDecoder` (CRLF/LF/CR lines, multi-line `data:`, `:` comments); `fragment` → `<template>`, scripts removed, `replaceWith`, `mount` over inserted elements, bubbling `netray:stream-fragment` on the inserted element; `end` → `done`, bubbling `netray:stream-end` on the form; `running`/`error` states, `[data-stream-error]` unhidden on error; superseded runs ignored and their readers cancelled on abort. No style writes, no colour strings.

## Phase 3 — Router, frame, CSP

### Groups

G1: C10, C22 · G2: C6, C18 · G3: C1–C5, C7–C9, C11, C12, C14–C17, C19–C21, C23, C24, C26 · G4: C13, C25. G1, G2, G3 independent; G4 after G3.

### Plan

#### G1 — `npm test -w crates/web/islands && npm run build -w crates/web/islands`
1. `crates/web/islands/src/theme.ts`: `theme(el)`; reads `localStorage.theme` in try/catch, applies it in `requestAnimationFrame`; click cycles auto → light → dark, sets/removes `data-theme` on `<html>`, stores/removes the key in try/catch; attributes only.
2. `crates/web/islands/src/main.ts`: `register("theme", theme)`.

#### G2 — `cargo test -p netray-web --test view_transition --test gate`
1. `crates/web/static/base.css`: `@media (prefers-reduced-motion: no-preference) { @view-transition { navigation: auto; } }`.

#### G3 — build islands, stage, `cargo test -p netray-web --test pages_router --test pages_frame --test pages_static --test pages_csp --test pages_errors --test pages_codenames --test stream_wire --test gate --test tagline --test components && cargo check -p netray-email && bash tests/repo/test_web_crate.sh && bash tests/repo/test_no_codenames.sh`
1. Root `Cargo.toml`: `base64 = "0.22"` in `[workspace.dependencies]`; `crates/email/Cargo.toml` uses it with `workspace = true`.
2. `crates/web/Cargo.toml`: axum, sha2, base64, percent-encoding, serde (derive); dev: tokio (macros, rt), tower (util), http-body-util, futures.
3. `crates/web/src/lib.rs`: `pub mod pages; pub mod stream;` (doc without the word `Router`).
4. `crates/web/src/stream.rs`: `fragment(id, html)` (serde struct, key order id, html), `end()` = `Event::default().event("end")`.
5. `crates/web/src/pages/mod.rs`: `SPECULATION_RULES`, `router()` = one `/static/{*path}` route, `robots()`, `Frame`, `page()` render helper (render error → `server_error()`), re-exports.
6. `crates/web/src/pages/headers.rs`: CSP with the generated hash and the status origin, `secure(response)` adds CSP plus the seven `site.rs` headers on every response.
7. `crates/web/src/pages/assets.rs`: `APP_CSS` concatenated at compile time (tokens, base, components in R44 order, pages.css); embedded islands bundle, logo, favicon, og table; anything else → 404 page; `public, max-age=3600`.
8. `crates/web/src/pages/errors.rs`: `not_found`, `server_error`, `query_redirect` (R15 normalisation, percent-encoded `Location`, 400 with refilled input and the line).
9. Templates `base.html`, `pages/not_found.html`, `pages/server_error.html`; `static/pages.css` (tokens only, no R44 root selectors); `static/og/landing.png` copied from `site/og/` for the default `og:image`.

#### G4 — `cargo test -p netray-common --lib security_headers && cargo test -p lens --lib spa && cargo test -p lens --test pages_mount && cargo build -p netray`
1. `crates/common/src/security_headers.rs`: `keep_inner_csp` (default false) leaves an inner CSP in place; unit row.
2. `crates/lens/Cargo.toml`: `netray-web` path dependency.
3. `crates/lens/src/spa.rs`: `Shell` state and `render_shell(site)`; `handler` takes `State<Shell>`; `state.rs` uses `render_shell`.
4. `crates/lens/src/lib.rs`: `frontend_router_with(shell)` = robots + `pages::router()` + SPA fallback under `security_headers_mw` (with `keep_inner_csp: true`); `pub fn frontend_router()` with the default site config; `run_with` merges it in place of its robots route and fallback.
5. `Dockerfile`: copy `crates/web/islands/dist` from the frontend stage (lens now embeds the bundle).
