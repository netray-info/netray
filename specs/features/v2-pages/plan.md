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

## Phase 4 — Guides

### Groups

G1: C5 (gate rule 3, SDD wording), C11 · G2: C1 (list, layout, index, three pattern guides, stubs), C2–C10, C12 · G3: C1 (Email batch) · G4: C1 (TLS and IP batch) · G5: C1 (HTTP and DNS batch). G1, then G2, then G3 ∥ G4 ∥ G5.

### Plan

Conventions for the port (codename table, link and action mapping, template shape): see the Phase 4 conventions the coders followed, summarised here — prose codenames replaced per R17; `lens.netray.info` form/link/samples → `/` with `d`, `/`, and `POST https://netray.info/api/v1/domains/example.com/runs`; `/api/lens` → `/api/v1/openapi.json` "netray API reference"; each guide template extends `pages/guide.html` and fills `article`; tool entries become `guide-tool` sections with a `query_input` call; no V1 class or `style=` survives.

#### G1 — `cargo test -p netray-web --test gate --test component_gate && bash tests/repo/test_v2_pages_sdd.sh`
1. `crates/web/src/gate.rs` `scan_template`: rule 3 accepts a script without `src` whose quoted, askama-free `type` is `speculationrules` or `application/ld+json`.
2. `specs/sdd/v2.md`: §5.1 segments `guide` and `privacy`; V4.9, S25 and §8.5 wording.

#### G2 — guide routes, list, layout, index, sitemap, og, JSON-LD, three pattern guides
1. `static/og/{dns,email,http,ip,tls}.png` copied; `assets.rs` `OG` table of six.
2. `base.html`: an empty `head` block for the JSON-LD.
3. `pages.css`: guide classes (`guide-lede`, `guide-tool`, `guide-tool__hint`, `guide-related`, `guide-index`) and prose rules, tokens only.
4. `pages/jsonld.rs`: `Article`, `BreadcrumbList` serde structs; `script_json` escapes `<`, `>`, `&`.
5. `pages/guides.rs`: `Guide`, `GUIDES` (32, V1 index order), `find`, breadcrumb set, `tool(...)` builder, one askama template struct per slug via a macro, `show`, `index`, `index_redirect` (301).
6. `pages/mod.rs`: routes `/guide`, `/guide/`, `/guide/{slug}`, `/guide/{slug}/` (404 page), `/sitemap.xml`.
7. Templates `pages/guide.html` (layout: JSON-LD in `head`, h1 from the list, `article` block, related section), `pages/guide_index.html`, `pages/guides/{dnssec,domain-security-audit,scoring}.html`, stubs for the other 29.

#### G3 — Email: email-auth, email-deliverability, mta-sts, dkim, bimi, tls-rpt, dnsbl, dane-tlsa, domain-investigation
#### G4 — TLS and IP: certificate-chain, hsts, certificate-transparency, letsencrypt-vs-commercial, multi-ip-tls, tls-protocol, encrypted-client-hello, certificate-management, ip-enrichment, asn, vpn-leak
#### G5 — HTTP and DNS: security-headers, csp, cors, cookie-security, http-redirects, record-types, dns-propagation, caa-records, lame-delegation
Each replaces its stubs with the port; GROUP_CMD: codename and banned-string greps over its files, `test_no_codenames.sh`, `pages_codenames`, `gate`, and `guide_content` showing no failure line for its slugs.

## Phase 5 — Compare and status

### Groups

G1: C4, C10 · G2: C1, C2, C3, C6–C9 · G3: C5 (prose) · G4: C11 (already in place). G1 ∥ G2 ∥ G3.

### Plan

#### G1 — `npx tsc --noEmit && npx vitest run` in the islands package, `npm run build`, gate and island repo checks
1. `crates/web/islands/src/status.ts`: fetches the two V1 UptimeRobot endpoints with `credentials: "omit"` on mount and every 60 s; clones the server templates, fills `[data-field]` by `textContent`; `success/warning/danger/paused` → `operational/degraded/down/paused` on `data-state`; banner "N of M"; error line unhidden on failure.
2. `src/main.ts`: `register("status", status)`.

#### G2 — compare and status pages
1. `crates/web/Cargo.toml`: `toml` (workspace).
2. `crates/web/compare.toml`: extracted from V1 (both tables, five axes, 41 rows, competitor cells verbatim, legend, honesty section, last reviewed); each row's basis from the catalogue (`checks` + `depth`, `checks = []` for V1 ✗, `product` + `note` where no check covers the row).
3. `pages/compare.rs`: `Row`, `Cell`, `Row::netray()`, `rows()`, `unknown_ids()`, the page handler.
4. `templates/pages/compare.html`: six tables, `<details>` rows, generated netray column, honesty section, last reviewed.
5. `templates/pages/status.html` and `pages/status.rs`: the skeleton with `<noscript>` link, `data-island="status"` and the two templates.
6. `pages/mod.rs`: `/compare`, `/status`.
7. `pages.css`: compare and status rules, `data-state` mapped to the pass, warn, fail and muted tokens.

#### G3 — `bash tests/repo/test_comparison_rules.sh`
1. `specs/rules/comparison-page-rules.md` rewritten for `compare.toml` and the template (`docs(rules)` commit).
2. `CLAUDE.md` Rules row (`docs` commit).

## Phase 6 — Rules and SDD

### Groups

G1: C2, C5 · G2: C1, C3, C4. No shared file.

### Plan

#### G1 — `bash tests/repo/test_v2_pages_sdd.sh && bash tests/repo/test_spectrum_component_sdd.sh && bash tests/repo/test_spectrum_deps.sh`
1. `specs/sdd/v2.md`: V4.8 drops "`site/` is gone", V4.8–V4.12 marked built by `v2-pages`; V5.1 deletes `site/` with `netray site`; §8.4 lists `theme` and `status` and states the `stream` wire (`fragment` `{id, html}`, `end` with `{}`, applied whatever the status, joiners replayed by the server) and the `data-stream-*` attributes; §8.7's name check covers templates, static files, islands and rendered pages, API serializers with Phase 3.

#### G2 — `bash tests/repo/test_frontend_rules.sh && bash tests/repo/test_sdd_in_repo.sh && bash tests/repo/test_comparison_rules.sh && bash tests/repo/test_no_self_host.sh`
1. `specs/rules/frontend-rules.md` rewritten for V2 (templates and components, tokens, islands, marker, streaming, accessibility, testing), each enforced rule once with its test path; the Solid apps frozen until V5.1, old rules cited by commit `45898422`; no codename outside the codename check's word list.
2. `CLAUDE.md`: the frontend Rules row names `crates/web/templates`, `static`, `islands`; a Conventions line: `site/` frozen until v2-cutover, content changes go to the templates.
