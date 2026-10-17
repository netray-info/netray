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
