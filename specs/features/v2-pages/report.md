# Report: v2-pages

## Phase 1 — 4a deferrals

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 `[data-theme="light"]` rule after the dark media block: every light value, `color-scheme: light`, `none` per `status.glow.*`; existing rules kept | green | crates/web/tests/tokens.rs |
| C2 | R2 the three SVGs carry root ids `netray-logo`/`netray-favicon`/`netray-badge-mark`, every `<style>` selector begins with that id; colours/geometry unchanged | green | crates/web/tests/identity_scope.rs |
| C3 | R3 gate reads an unquoted askama attribute value as one opaque value; the next attribute is its own | green | crates/web/tests/gate.rs |
| C4 | S: `tokens::CSS` light rule after dark block with `color-scheme: light`, `--color-bg: #f7f8fa`, `--status-glow-fail: none` | green | crates/web/tests/tokens.rs |
| C5 | S: every SVG selector begins with the root id; colour and geometry tests still pass | green | crates/web/tests/identity_scope.rs |
| C6 | S: planted `<button class={{ c }} onclick="go()">` fails rule 3 naming `onclick` | green | crates/web/tests/gate.rs |
| C7 | S: planted `<script src={{ asset }} onload="init()"></script>` fails rule 3 naming `onload` | green | crates/web/tests/gate.rs |
| C8 | S: `<script src={{ asset }} defer></script>` passes | already_implemented | crates/web/tests/gate.rs |

C8's row passed before the change (the gate already accepted `src={{ asset }} defer`); it is kept as the passing twin of C7.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 27367 | 19 |
| G2 | 1 | sonnet | 25568 | 13 |
| G3 | 1 + 1 + 1 + 1 + 2 + 1 (five reader passes) | sonnet, then opus from the third fix | 28007 + 31175 + 40281 + 44111 + 47284 + ~30000 | 34 + 38 + 76 + 95 + 114 + ~60 |

### Reader

Five passes over `crates/web/src/gate.rs`; G1 and G2 were sound from the first pass.

| pass | COUNTS | outcome |
|---|---|---|
| 1 | blocker=2 | glued handler after `{{ … }}`; stray quote after a value span. Rows added (1c0ffd1), fixed |
| 2 | blocker=2 amendment=1 | the fix stopped a quote after `{% … %}`/`{# … #}` opening the value. Rows added (e412d47), fixed |
| 3 | blocker=2 amendment=1 nit=1 | `{% else %}`/`{% when %}` branches and unquoted text after a span read unlike the browser. Requirement 3 amended: control flow and comments go inside a quoted value (ef87b70) |
| 4 | blocker=1 amendment=4 deferred=5 nit=2 | value span inside an unquoted value, glued `/`, empty-rendering span hiding the next token, quoted-branch `javascript:`, `type` behind a span. Rows added, fixed with a second reading as HEAD parsed |
| 5 | blocker=4 amendment=1 deferred=5 | no template HEAD refuses passes the new gate; real tree 0 violations. Over-refusal of unquoted `javascript:` fixed (rows added). The other three are gaps HEAD shares: DEFERRED (Decision log) |

DEFERRED (gaps HEAD's gate shares, not built here):
- two leading value spans in one tag are blanked together in the second reading, never one at a time;
- a control span glued to the token the second reading moves in is not refused;
- `<!--` inside a start tag is blanked as a comment, so text after it is not read as attributes;
- askama comments and `-` whitespace control join text the gate sees split (`java{# c #}script:`, `on{# c #}click`);
- entity-encoded or newline-split `javascript:`;
- duplicate `type` attributes (HTML keeps the first);
- `src` inside attribute-position control flow counts on every branch;
- `on{{ evt }}=` handler names; `{#` inside a `{{ "…" }}` string literal.

### Behavioural verification

skipped: the phase has no entry point; the gate, the token CSS and the SVGs are exercised by `cargo test -p netray-web`.

## Phase 2 — Islands scaffold and the `stream` island

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R4 `crates/web/islands` private workspace package: root workspaces, toolchain-majors list, esbuild exact, typescript workspace major, vitest ^4, happy-dom ^20; build → minified ESM `dist/islands.js` (untracked); test = `tsc --noEmit` then `vitest run` (happy-dom config); `check-frontend-dist` requires the islands bundle | green | tests/repo/test_islands_package.sh |
| C2 | R5 `mount(root)` mounts each registered island on every `[data-island]` under root (root included), once per element, unknown names ignored; `mount(document)` on load | green | crates/web/islands/test/main.test.ts |
| C3 | R6 budget test: gzip (default level) of `dist/islands.js` ≥ 30 720 bytes fails naming the size | green | crates/web/islands/test/budget.test.ts |
| C4 | R7 gate rule 1 refuses island style writes (`.style`, `cssText`, `setProperty(`, `insertRule(`, `setAttribute` with "style"/'style'), comments excluded | green | crates/web/tests/gate.rs |
| C5 | R28 `stream` island: POST URL-encoded fields + `trigger` to `data-stream-url`/action with `Accept: text/event-stream`, `credentials: "same-origin"`; autostart, submit/abort, SSE wire, replay, state and events | green | crates/web/islands/test/stream-post.test.ts, stream-wire.test.ts |
| C6 | S: `probe` mounted once when mount runs twice; unknown name does not throw | green | crates/web/islands/test/main.test.ts |
| C7 | S: built bundle passes the budget; planted 40 KB incompressible string fails naming the size | green | crates/web/islands/test/budget.test.ts |
| C8 | S: type error in `src/main.ts` → `npm test -w crates/web/islands` exits non-zero before vitest | green | tests/repo/test_islands_typecheck.sh |
| C9 | S: planted island `el.style.color = x` fails rule 1; `el.dataset.state = "warn"` passes | green | crates/web/tests/gate.rs |
| C10 | S: `crates/web/islands/dist` missing → `just check-frontend-dist` fails naming `just adlc-setup` | green | tests/repo/test_islands_package.sh |
| C11 | S: `data-stream-autostart="visit"` → exactly one POST to `data-stream-url` with `Accept: text/event-stream` and `trigger=visit`; no attribute → nothing until submit | green | crates/web/islands/test/stream-post.test.ts |
| C12 | S: `document.prerendering` true → nothing until `prerenderingchange`, then one POST | green | crates/web/islands/test/stream-post.test.ts |
| C13 | S: submit button `name="trigger" value="recheck"` → `trigger=recheck`; second submit aborts the first request's signal | green | crates/web/islands/test/stream-post.test.ts |
| C14 | S: fragments for `band-dns`, `band-tls` split across chunks → both replaced in order, `netray:stream-fragment` per element; absent ID dropped, later fragments apply | green | crates/web/islands/test/stream-wire.test.ts |
| C15 | S: replayed then live fragment for one ID → element holds the live HTML | green | crates/web/islands/test/stream-wire.test.ts |
| C16 | S: fragment HTML with `<script>` and `data-island="probe"` → no script in document, probe mounted on the new element | green | crates/web/islands/test/stream-wire.test.ts |
| C17 | S: 429 `text/event-stream` with fragment + `end` → applied, state `done`; 502 HTML or rejecting fetch → state `error`, `[data-stream-error]` visible | green | crates/web/islands/test/stream-wire.test.ts |

The C9 passing rows (comments, `dataset`, `classList`, `data-state`, `styleName`, `.styles`) passed before the change and guard against over-refusal.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 31504 | 36 |
| G2 | 1 | sonnet | 30374 | 26 |
| G3 | 1 + 3 (reader amendment) | sonnet | 44971 + ~35000 | 45 + ~90 |

### Reader

COUNTS blocker=0 amendment=1 deferred=2 nit=4

| class | finding | outcome |
|---|---|---|
| AMENDMENT | a `<template>` parses fragments in an HTML context; an SVG band (`<g id="band-dns">`) would come out in the XHTML namespace (shown in Chromium) | repaired in phase: requirement 28 amended, parse in the target's context (SVG wrapper / `createContextualFragment`); test added |
| DEFERRED | a superseded run kept applying the rest of its chunk | fixed with the amendment; test added |
| NIT | fragments after `end` in the same chunk were applied | fixed with the amendment; test added |
| NIT | `netray:stream-fragment` fires once per top-level element of a fragment | kept; a fragment is one element by contract |
| DEFERRED | style writes R7 does not list (`el["style"]`, `setAttributeNS`, `attributeStyleMap`) pass the gate | not built; R7's list is closed |
| NIT | `setAttribute ("style")` with a space passes; `.style-note` in a selector string is flagged | not acted on |
| NIT | the two pre-existing island colour rows now also pass on the style finding | not acted on |

### Behavioural verification

skipped: no route serves the bundle until Phase 3; the island is exercised end to end by the vitest suite over the built package (27 tests) and the bundle builds to 1.4 KB gzipped.

## Phase 3 — Router, frame, CSP

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R8 deps (axum, netray-engine, sha2, base64 in workspace deps); `pages::router()` without fallback, `/`, `/robots.txt`; 404, `?d=` and robots handlers exported; `test_web_crate.sh` allows `Router` only under `entries/`, `pages/`, `api/`, allows axum, drops the `netray` guard | green | crates/web/tests/pages_router.rs, tests/repo/test_web_crate.sh |
| C2 | R9 `base.html` frame: lang, charset, viewport, title, description, canonical, og tags, `styles`/`header`/`scripts` defaults, nav, `main`, footer with tagline, Privacy, Status; each page carries header, footer, stylesheet, bundle and speculation rules exactly once | green | crates/web/tests/pages_frame.rs |
| C3 | R10 `/static/app.css` (tokens, base, components in R44 order, pages.css), `/static/islands.js`, logo, favicon, `/static/og/<name>.png` with media types; other `/static/` → 404 page; `Cache-Control: public, max-age=3600` | green | crates/web/tests/pages_static.rs |
| C4 | R11 CSP exactly as specified with the status origin, plus the seven other headers of `site.rs`; `script-src` without `'unsafe-inline'`/`'unsafe-eval'` | green | crates/web/tests/pages_csp.rs |
| C5 | R12 speculation rules constant rendered verbatim; CSP hash = base64 SHA-256 of the constant; no pattern matches `/`, `/example.com`, `/example.com/run`, `/example.com/tls`, `/compare` | green | crates/web/tests/pages_csp.rs |
| C6 | R13 `@view-transition` only inside `@media (prefers-reduced-motion: no-preference)` | green | crates/web/tests/view_transition.rs |
| C7 | R14 404 page (heading, `query_input` action `/` name `d`, links `/guide/`, `/compare`), 500 page (heading, link `/status`), both on the frame, `Cache-Control: no-store` | green | crates/web/tests/pages_errors.rs |
| C8 | R15 `?d=` → 301 `/<domain>` (trimmed, lower-cased, one trailing dot dropped, has a dot, no whitespace `/ ? #`, percent-encoded); else 400 with the refilled input and the line; no `d` → 404 page | green | crates/web/tests/pages_errors.rs |
| C9 | R16 `test_no_codenames.sh` over the listed paths with a self-test; a render test over every route finds no codename in bodies or headers | green | tests/repo/test_no_codenames.sh, crates/web/tests/pages_codenames.rs |
| C10 | R27 `theme` island: auto → light → dark cycle, `data-theme` on `<html>`, `localStorage.theme`, storage errors ignored | green | crates/web/islands/test/theme.test.ts |
| C11 | R29 `stream::fragment`/`end` encoders; Rust test writes `islands/test/fixtures/stream.sse` (UPDATE_GOLDEN), island parser test reads it | green | crates/web/tests/stream_wire.rs, crates/web/islands/test/stream-fixture.test.ts |
| C12 | R32 one `base.html` under `crates/web/templates`; frame shared with v2-entries' pages when they land | green | crates/web/tests/pages_frame.rs |
| C13 | R33 lens merges `pages::router()` ahead of `spa::handler`; middleware keeps every inner security header (`keep_inner`); `/`, SPA paths, robots, `/privacy`, `/r/*` unchanged | green | crates/lens/tests/pages_mount.rs |
| C14 | S: composed router `GET /nope` → 404, frame, `query_input` action `/` name `d`, `no-store`; `router()` alone has no `/` route | green | crates/web/tests/pages_errors.rs, pages_router.rs |
| C15 | S: every routed page's CSP `script-src` is `'self' 'sha256-…'`; the seven headers present | green | crates/web/tests/pages_csp.rs |
| C16 | S: hash of the rendered speculation-rules body appears in that response's CSP | green | crates/web/tests/pages_csp.rs |
| C17 | S: rules match none of `/`, `/example.com`, `/example.com/run`, `/example.com/tls`, `/compare`; match `/guide/dnssec`, `/r/abc12345` | green | crates/web/tests/pages_csp.rs |
| C18 | S: view-transition test over `static/base.css` | green | crates/web/tests/view_transition.rs |
| C19 | S: `GET /?d=%20Example.COM.%20` → 301 `/example.com`; `?d=foo`, `?d=a/b.c` → 400 with the input refilled | green | crates/web/tests/pages_errors.rs |
| C20 | S: `/static/app.css` begins with the tokens `:root` rule and holds every component root class; `/static/missing.css` → 404 | green | crates/web/tests/pages_static.rs |
| C21 | S: planted "lens" template fails `test_no_codenames.sh` naming file and line; render test finds no codename | green | tests/repo/test_no_codenames.sh, crates/web/tests/pages_codenames.rs |
| C22 | S: theme button clicked three times → light, dark, absent, `localStorage.theme` follows; throwing storage → attribute still changes | green | crates/web/islands/test/theme.test.ts |
| C23 | S: `fragment("band-dns", "<g id=\"band-dns\"></g>")` + `end()` bytes equal the fixture the island's parser test reads | green | crates/web/tests/stream_wire.rs, crates/web/islands/test/stream-fixture.test.ts |
| C24 | S: `test_web_crate.sh` passes with axum and a `Router` under `src/pages/`, fails on a planted `Router` in `components.rs`, no `netray` deps check, still checks name, edition, lib, token source, askama 0.16 | green | tests/repo/test_web_crate.sh |
| C25 | S: lens `GET /guide/dnssec`-class page route → pages CSP; `GET /`, `/some-client-route`, `/robots.txt` → SPA shell and robots with lens's CSP | green | crates/lens/tests/pages_mount.rs |
| C26 | S: with v2-entries landed, its privacy page and the 404 share frame and footer; one `base.html` | green | crates/web/tests/pages_frame.rs |

Passed before the change: `test_no_codenames.sh` (the tree held no codename yet; its self-test refuses the planted fixture), `stream-fixture.test.ts` (the island already parsed the fixture), the `view_transition.rs` guard and self-test. C26's v2-entries half is inert until v2-entries lands: requirement 32 makes the second lander merge the frame.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 26918 | 19 |
| G2 | 1 | sonnet | 22767 | 14 |
| G3 | 2 | sonnet | 70582 | 155 |
| G4 | 2 | sonnet | 43907 | 95 |
| reader fixes | 1 | sonnet | 41270 | 85 |

### Behavioural verification

`netray lens lens.dev.toml` from this worktree on 127.0.0.1:18731:

```
== /static/app.css
HTTP/1.1 200 OK
content-type: text/css; charset=utf-8
cache-control: public, max-age=3600
content-security-policy: default-src 'self'; script-src 'self' 'sha256-57GqQIelF6haaPGofb/J2Ypud64R6MJSIxW9RTejeO4='; style-src 'self' 'unsafe-inline'; ... connect-src 'self' https://stats.uptimerobot.com; ... form-action 'self' https://*.netray.info
== /            -> 200 SPA shell, lens CSP (script-src 'self' 'unsafe-inline')
== /robots.txt  -> 200 text/plain, lens CSP
== /nope, /?d=  -> 200 SPA shell (lens keeps `/` and its fallback, requirement 33)
```

### Reader

Pass 1: COUNTS blocker=3 amendment=2 deferred=1 nit=0 · Pass 2: COUNTS blocker=0 amendment=0 deferred=1 nit=2

| class | finding | outcome |
|---|---|---|
| BLOCKER | responses axum builds itself (405) carried no CSP or headers | `map_response(secure)` layer on `router()`; test added |
| BLOCKER | `/static/%FF` answered a plain 400 from the `Path` extractor | tolerant extraction answers the 404 page; bare `/static/` too; test added |
| BLOCKER | nav label "Guide" instead of the approved "Guides" | fixed; labels test added |
| AMENDMENT | under lens the shared layer rewrote the pages' CORP to `cross-origin` | requirement 33 amended: lens's layer keeps every inner security header (`keep_inner`); pass 2 compared 40 lens V1 requests against main: identical |
| AMENDMENT | `end` with no `data:` line is dropped by a conforming SSE parser | requirement 29 amended: `end` carries `{}`; fixture regenerated |
| DEFERRED | `/static/` empty tail fell through to the host's fallback | fixed with the 404 route |
| DEFERRED (pass 2) | the exported `query_redirect`/`robots` composed outside `router()` get no layer for axum's own 405 | v2-cutover's apex router applies the layer when it composes them |
| NIT (pass 2) | the island's wire tests still feed `end` with empty data | kept: they pin the island's leniency |
| NIT (pass 2) | report and plan wording named `keep_inner_csp` and data-less `end` | report row corrected; the plan records what was planned |
