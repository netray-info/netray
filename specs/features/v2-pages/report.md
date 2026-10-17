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

## Phase 4 — Guides

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R17 `guides.rs` is the one source of slug, title, description, category, og:image, related; 32 guide templates on `base.html` with V1 `<main>` content and ids; codenames replaced; 31 `tool-entry` forms → `query_input`; link and action mapping; suite links dropped; `lens.html` → `scoring` | green | crates/web/tests/pages_guides.rs, guide_content.rs |
| C2 | R18 `/guide/<slug>` 200, `/guide/<slug>/` and unknown 404, `/guide/` index in V1 categories and order with absolute links, `/guide` → 301 `/guide/` | green | crates/web/tests/pages_guides.rs |
| C3 | R19 a test fails naming the check when a catalogue `guide` path names no listed guide | green | crates/web/tests/guide_links.rs |
| C4 | R20 V1 `og:image` kept; six PNGs copied to `static/og/`, `lens.png` guides use `landing.png` | green | crates/web/tests/guide_og.rs |
| C5 | R21 JSON-LD from serde structs, `<`, `>`, `&` escaped, in `application/ld+json`; gate rule 3 accepts it beside `speculationrules`; SDD V4.9, S25, §8.5 wording; §5.1 `guide` and `privacy` | green | crates/web/tests/guide_jsonld.rs, gate.rs, tests/repo/test_v2_pages_sdd.sh |
| C6 | R22 `/sitemap.xml` generated (guide index, guides, compare, status, privacy, apex, four inspector roots, no `lastmod`); robots handler | green | crates/web/tests/pages_sitemap.rs |
| C7 | S: every slug → 200 on the frame with title, description, canonical `https://netray.info/guide/<slug>`, `og:image` under `/static/og/` | green | crates/web/tests/pages_guides.rs |
| C8 | S: `/guide/dnssec/`, `/guide/nope` → 404; `/guide` → 301 `/guide/` | green | crates/web/tests/pages_guides.rs |
| C9 | S: every catalogue `guide` path names a listed guide; a planted `/guide/missing` fails naming the check | green | crates/web/tests/guide_links.rs |
| C10 | S: `dnssec` keeps its V1 element ids; its form is a `query_input` with action `https://dns.netray.info/` and name `q` | green | crates/web/tests/guide_content.rs |
| C11 | S: JSON-LD body parses as JSON and holds no raw `<`; planted `<script type="text/plain">x</script>` fails rule 3 | green | crates/web/tests/guide_jsonld.rs, gate.rs |
| C12 | S: `/sitemap.xml` lists the entries and parses as XML | green | crates/web/tests/pages_sitemap.rs |

Passed before the change: the gate rows refusing `text/plain`, `application/json` and a handler on an ld+json script (any non-speculation script was refused already); the robots half of C6 (Phase 3).

Content follow-up for the pass before v2-cutover (out of scope here): `dkim_selectors` is not an SDD §7.1 parameter, and `?d=` drops it in the 301; the scoring guide's prose names `summary.not_applicable`; the inspector-host API samples answer 410 after the cutover (SDD §7.3). The API v1 curl samples read the stored run's shape (reader fix).

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 28369 | 34 |
| G2 | 1 | opus | 127593 | 496 |
| G3 | 1 | sonnet | 41465 | 75 |
| G4 | 1 | sonnet | 38358 | 53 |
| G5 | 1 | sonnet | 34120 | 42 |

### Reader

Pass 1: COUNTS blocker=2 amendment=1 deferred=3 nit=0 · Pass 2: COUNTS blocker=0 amendment=0 deferred=2 nit=2

| class | finding | outcome |
|---|---|---|
| BLOCKER | API v1 curl samples kept V1 jq filters (`.summary.grade`, `.email.checks[]`) that fail on the stored run | filters read `Run` (`{grade, complete, sections}`, `.grade`, `.checks[] \| select(.id \| startswith("email.dkim"))`); test added; pass 2 ran jq over a serialised `Run` |
| BLOCKER | rule 3 measured the askama overlap on a trimmed `type`, so `speculationrules{{ x }}` passed | overlap measured on the untrimmed value; three rows added |
| AMENDMENT | requirement 17 said `netray.info` serves the V1 lens paths; the apex sends `/api*` to `netray site` | requirement 17 amended to the Decision log's API v1 rewrite |
| DEFERRED | duplicate `type` exemption now also covers ld+json; askama in attribute position around `type` | gaps HEAD shared for `speculationrules`; Phase 1 DEFERRED list |
| DEFERRED | `dkim_selectors` dropped by `?d=`'s 301 after the cutover; inspector samples 410 after the cutover; `summary.not_applicable` prose | content pass before v2-cutover |
| NIT | "If any section shows a low grade" | reworded |
| NIT | report paragraph called the filters V1's | corrected |

Screenshot review (phase 4, local lens): the header logo collapsed to zero width (an SVG with only a viewBox in a flex row); sized in `pages.css`, test added.

### Behavioural verification

`netray lens lens.dev.toml` from this worktree on 127.0.0.1:18731:

```
/guide/dnssec   -> 200 text/html; form action https://dns.netray.info/
/guide/         -> 200 text/html
/guide          -> 301 location: /guide/
/guide/dnssec/  -> 404 text/html
/sitemap.xml    -> 200 application/xml; 41 <loc> entries
```

## Phase 5 — Compare and status

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R23 `compare.toml` holds both tables, Table 2's axes, every row label, competitor columns with symbols and notes, legend, the honesty section, "Last reviewed"; rows give `checks`+`depth` or `product`+`note`; V1 ✗ rows have `checks = []` | green | crates/web/tests/pages_compare.rs, compare_column.rs |
| C2 | R24 generated netray column (✓/~/✗, note from catalogue titles in order; product rows); unknown ID fails a test naming row and ID; header "netray", section "Where netray trades depth for breadth"; competitor cells keep V1 content | green | crates/web/tests/compare_column.rs, pages_compare.rs |
| C3 | R25 `/compare` renders both tables on the frame with `<details>` rows and no island; `/status` server skeleton: heading, intro without a monitor count, `<noscript>` link to the public status page, `data-island="status"` containers | green | crates/web/tests/pages_compare.rs, pages_status.rs |
| C4 | R26 `status` island: the two V1 endpoints, `credentials: "omit"`, every 60 s, cards and incidents from `<template>` clones via `textContent`/attributes, `data-state` operational/degraded/down/paused, banner count of the list's length, error line on failure | green | crates/web/islands/test/status.test.ts |
| C5 | R30 `comparison-page-rules.md` applies to `compare.toml` and `templates/pages/compare.html`, the generated column and sitemap, keeps symbols, two tables, cadence, "Last reviewed", names netray | green | tests/repo/test_comparison_rules.sh |
| C6 | S: `/compare` carries every V1 row label, the netray column header and the competitor cells of `site/compare/index.html` | green | crates/web/tests/pages_compare.rs |
| C7 | S: row `checks = ["dns.dnssec", "dns.caa"]`, full → ✓ with "DNSSEC signed" and "CAA records"; `checks = []` → ✗ | green | crates/web/tests/compare_column.rs |
| C8 | S: a planted row naming `dns.nonexistent` fails naming row and ID | green | crates/web/tests/compare_column.rs |
| C9 | S: `/status` without JavaScript: the `<noscript>` line links the public status page; no monitor count printed | green | crates/web/tests/pages_status.rs |
| C10 | S: stubbed list of 3 with one `danger` → banner 1 of 3 affected, that card `data-state="down"`; rejecting fetch → error line | green | crates/web/islands/test/status.test.ts |
| C11 | S: `GET /privacy` is not matched by `router()` | already_implemented | crates/web/tests/pages_status.rs |

C11 held before the change: `router()` never had a `/privacy` route. The V1 event-feed shape is undocumented (V1 printed raw JSON); the island fills `title` and `date` and leaves the rest to the content pass.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 29967 | 30 |
| G2 | 2 | opus | 104755 | 319 |
| reader fixes | 1 + 1 + data edit | sonnet | 62397 | 75 |
| G3 | 1 | sonnet | 33029 | 29 |

### Reader

Pass 1: COUNTS blocker=4 amendment=1 deferred=4 nit=0 · Pass 2: COUNTS blocker=2 amendment=2 deferred=1 nit=2 · Pass 3 (fix only): COUNTS blocker=1 amendment=0 deferred=4 nit=1

| class | finding | outcome |
|---|---|---|
| BLOCKER (1) | SOA row ✓ on `dns.ttl` (judges only the SOA minimum) | `dns.ttl` partial |
| BLOCKER (1, 2) | Protocol versions ✓ on `tls.tls_version`; the TLS inspector negotiates 1.2/1.3 only | `checks = []` (✗) |
| BLOCKER (1) | status island read the event feed's `.data`; live shape is `{status, results, meta}` | reads `results`; a feed without `results` is a failure; tests use the live shape |
| BLOCKER (1) | rules doc said the sitemap is generated from the routes | doc names `pages::sitemap()` and its fixed page list |
| AMENDMENT (1) | V1 ✗ rows forced to `checks = []` although OCSP, DANE and blocklist checks exist | requirement 23 amended; those rows list their check, partial |
| BLOCKER (2) | Table 1 DNS checks ✓ while listing the partial `dns.dnssec` | partial |
| AMENDMENT (2) | SOA as a `product` row escaped the catalogue guard | lists `dns.ttl` |
| AMENDMENT (2) | incidents read never-observed `title`/`date` | an entry without a string `title` renders as raw JSON |
| BLOCKER (3) | NS records ✓: the NS checks read the child's NS set, never the parent's | partial (data edit), test added; review closed (Decision log) |
| DEFERRED | paused monitors count as affected (V1 rule); `Promise.all` drops a good monitor list when the feed fails; feed `status:false` with `results` not checked; empty monitor list shows the all-clear; the live monitor list names `lens.netray.info` (UptimeRobot configuration, renamed at v2-cutover); the honesty paragraph on blocklists | not built; content and configuration follow-ups |

### Behavioural verification

`netray lens lens.dev.toml` from this worktree on 127.0.0.1:18731, Playwright (Chromium 1243) at 390 and 1440 px, light and dark:

```
/compare      -> 200, six tables, 41 rows, generated netray column
/status       -> 200, island fetched the live UptimeRobot list: 7 cards, "All systems operational", "No incidents to report."
/static/nope  -> 404 page
```

Screenshots: `target/screenshots/v2-pages/` (not committed).

## Phase 6 — Rules and SDD

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R31 `frontend-rules.md` rewritten for V2 (templates and components, tokens, islands and limits, marker rule, streaming and the `stream` contract, accessibility, testing); each enforced rule stated once with its test path; no codename outside the codename check's word list; Solid apps frozen, old rules cited by commit; `CLAUDE.md` Rules row points at `crates/web/templates`, `static`, `islands`; Conventions line: `site/` frozen until v2-cutover | green | tests/repo/test_frontend_rules.sh |
| C2 | R34 SDD: V4.8's `site/` clause moves to V5.1; §8.4 lists `theme` and `status` and states the `stream` wire and `data-stream-*` attributes; §8.7 name-check scope; Phase 4 table marks V4.8–V4.12 built by `v2-pages` | green | tests/repo/test_v2_pages_sdd.sh |
| C3 | S: the codename word list grepped over `frontend-rules.md` finds none outside the rule's own wording; no section on SolidJS patterns or `common-frontend` | green | tests/repo/test_frontend_rules.sh |
| C4 | S: every test path `frontend-rules.md` cites exists | green | tests/repo/test_frontend_rules.sh |
| C5 | S: V5.1 holds the `site/` deletion, V4.8 does not; §8.4 lists `theme`, `status` and the `stream` wire; `tests/repo/test_sdd_in_repo.sh` passes | green | tests/repo/test_v2_pages_sdd.sh |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 3 | sonnet | 31006 | 44 |
| G2 | 1 | opus | 66553 | 142 |
| reader fixes | 2 | sonnet | ~40000 | ~120 |

### Reader

COUNTS blocker=2 amendment=10 deferred=1 nit=1 — every finding repaired in the documents (statements made true against the code: `netray:stream-fragment` targets, the form requirement, the `trigger` rule, what `pages_frame.rs` and `component_files.rs` check, the frame exception for v2-entries' pages, the `checkMeta.ts` exception to the Solid freeze, what "data attributes only" allows, V4.8 and V4.11 scope, where content changes go). DEFERRED: the per-crate CLAUDE.md files still point Solid-app edits at `frontend-rules.md` (the V1 rules are at commit `45898422`; they go with the apps in V5.1).

### Behavioural verification

skipped: the phase changes documents only; the repo checks above read them.
