# Frontend Rules

The V2 view layer: askama templates on one page frame, generated tokens, small TypeScript islands
and one streaming island, all in `crates/web` (SDD `specs/sdd/v2.md` §8). Every page is rendered
in Rust; the client computes no layout and holds no verdict.

The Solid apps (`crates/*/frontend`, `packages/common-frontend`) are frozen until V5.1, except the catalogue texts `CLAUDE.md` requires in the one `checkMeta.ts` (`CHECK_LABELS`, `CHECK_DESCRIPTIONS`; its path is in `tests/repo/test_checkmeta_catalogue.sh`); their old rules are in commit `45898422` (`git show 45898422:specs/rules/frontend-rules.md`).

A rule the gate or a test enforces is stated once below with the test's path; the test is the
definition, this file the pointer.

## 1. Templates and components

| Path | Holds |
|---|---|
| `crates/web/templates/base.html` | the one page frame: head, header, `main`, footer, scripts |
| `crates/web/templates/components/` | one askama macro per component |
| `crates/web/templates/pages/` | one template per page, each extending `base.html` |
| `crates/web/templates/partials/` | shared fragments (the tagline) |
| `crates/web/static/` | one stylesheet per component, `base.css`, `pages.css`, the identity SVGs |

- **One frame.** Every page extends `base.html`. For the 404, the 400 and the 500 page,
  `crates/web/tests/it/pages_frame.rs` checks that head, header, footer and scripts appear exactly
  once, in order, and that the 404 and 500 pages share the frame's prefix and footer. The entry page
  `/r/<id>` renders inside the full frame (v2-result-fidelity, requirement 13); the gone and
  privacy pages override `styles` with one inline `<style>` and `header` and `scripts` with
  nothing (spec v2-pages, requirement 32), so they carry no navigation, script or island; the
  footer stays shared: `crates/web/tests/it/entries_frame.rs`.
- **Components exist once.** The ten components (`grade`, `run_line`, `progress_ray`, `spectrum`,
  `trace`, `check_row`, `evidence_block`, `diff_block`, `grouping_switch`, `query_input`) are a
  macro in `templates/components/<name>.html` plus a stylesheet `static/<name>.css` each:
  `crates/web/tests/it/component_files.rs`, `crates/web/tests/it/component_gate.rs`; each renders its
  root class and its view's values: `crates/web/tests/it/components.rs`.
- **Gate rule 2.** A component's markup is rendered only through its macro, and its class (the
  name with `_` turned into `-`) is defined only in its own stylesheet:
  `crates/web/tests/it/gate.rs`.
- **Gate rule 3.** No inline `<script>` except the speculation rules and `application/ld+json`
  data blocks; no inline event handler, no `javascript:` URL; askama control flow and comments go
  inside a quoted attribute value, never in an unquoted one: `crates/web/tests/it/gate.rs`.
- **CSP.** Every route carries the security headers, and the CSP's script hash is the hash of the
  speculation rules, the only inline executable script: `crates/web/tests/it/pages_csp.rs`.
- Layout and label placement are computed in Rust (`crates/web/src/spectrum.rs`), never in a
  template or an island.

## 2. Tokens

- `crates/web/tokens.json` is the only token source; `build.rs` generates `tokens.css` and the
  Rust `tokens` module from it. Edit the JSON, never the output: `crates/web/tests/it/tokens.rs`.
- **Gate rule 1.** Outside `tokens.css`, no template, stylesheet or island holds a colour literal
  (hex, a colour function, a named colour), and every `border-radius`, `font-family`,
  `font-size`, `stroke-width` and border width is a `var(--…)` of its own token family (a radius
  only `--radius-*`); island string literals are scanned for colours too:
  `crates/web/tests/it/gate.rs`. The gate's code is `crates/web/src/gate.rs`; an identifier-valued
  property it misreads as a colour goes into its `IDENTIFIER_PROPERTIES`.
- Every status colour meets its contrast against `bg` and `plate` in both themes (`mark` and
  `hair` 3:1, `text` 4.5:1): `crates/web/tests/it/contrast.rs`.
- Fonts are the system stacks of `font.sans` and `font.mono`; no web font is loaded.

## 3. Islands and their limits

The islands live in `crates/web/islands/src/`, one module each, bundled by esbuild into
`crates/web/islands/dist/islands.js`, which the frame loads once.

- **Registry.** `main.ts` keeps the registry: `register(name, island)` adds an island,
  `mount(root)` runs the registered island on every `[data-island="<name>"]` element in `root`
  (the root included), once per element; an unknown name is skipped:
  `crates/web/islands/test/main.test.ts`. `main.ts` calls `mount(document)` on load (no test pins
  it); the `stream` island runs `mount` over every fragment it inserts, which
  `crates/web/islands/test/stream-wire.test.ts` tests.
- **Data attributes only.** An island reads the data attributes and the markup the server wrote
  inside its element (a `<template>` to clone) and its own form's fields; no inline script, no
  global set by the page and no attribute expression configures it. An island may read browser
  state it owns (`theme` reads `localStorage`) and fetch a fixed third-party endpoint (`status`
  reads UptimeRobot). It still writes no styles, computes no layout and holds no verdict.
- **No style writes.** An island never writes styles: no `.style` member, `cssText`,
  `setProperty(`, `insertRule(` or `setAttribute("style", …)`; state goes through attributes
  that stylesheets select on (gate rule 1): `crates/web/tests/it/gate.rs`.
- **Budget.** The built bundle stays under 30 KB gzipped (30720 bytes):
  `crates/web/islands/test/budget.test.ts`.
- **Package.** `crates/web/islands` is a workspace package with a pinned build, and the gate
  refuses a checkout without the built bundle: `tests/repo/test_islands_package.sh`. Its
  `npm test` type-checks before vitest runs: `tests/repo/test_islands_typecheck.sh`.
- The `theme` and `status` islands: `crates/web/islands/test/theme.test.ts`,
  `crates/web/islands/test/status.test.ts`. The result page's `tabs` (evidence tabs) and `share`
  (share dialog) islands: `crates/web/islands/test/tabs.test.ts`,
  `crates/web/islands/test/share.test.ts`.
- **Phone values.** Phone sizes are `*Phone` tokens, which `build.rs` emits inside the
  `layout.phoneMax` media block, so a rule that reads the token adapts by itself; a geometry the
  layout writes inline (the spectrum's label band and leaders) is a `calc()` over those tokens,
  never a px value, or the phone value cannot apply.

## 4. The marker rule

The marker, a line of `--line-marker` crossing an axis, is the only active state. It is one CSS
rule in `base.css` (`.marker`), applied by `aria-pressed="true"`, `aria-current` or
`aria-selected="true"`; `crates/web/tests/it/component_files.rs` checks that `base.css` holds that
rule. The favicon and the badge mark are the marker crossing the
axis: `crates/web/tests/it/identity_marks.rs`. Each mark of the spectrum and the trace resolves to
the token its state owns: `crates/web/tests/it/drawing_rules.rs`.

## 5. Streaming

No Datastar and no attribute expressions (S25): HTML carries no code, and the `stream` island
posts, reads the SSE stream and replaces elements by ID. A run's stream patches the elements of
SDD §5.3; a request that joins a run in flight first receives the fragments of the sections
already finished, then the live ones, so the client needs no distinction.

The `stream` island's contract (spec v2-pages, requirement 28):

- **Mount.** It mounts on a `<form data-island="stream">` (it needs `FormData`) and posts the
  form's fields except one named `trigger`, URL-encoded, to `data-stream-url` (else `action`)
  with `fetch`, `POST`, `Accept: text/event-stream` and `credentials: "same-origin"`. It sets
  `trigger` to the submitter's `value` when the submitter is named `trigger`, else to
  `data-stream-autostart`; with neither, no `trigger` is sent.
- **Autostart.** With `data-stream-autostart="<trigger>"` it posts once on mount, and not before
  `prerenderingchange` while `document.prerendering` is true. Without it, it posts only on
  submit; a post while one runs aborts the running one.
- **Wire.** The body is parsed as SSE whatever the HTTP status when the content type is
  `text/event-stream`, so a refusal's fragment is applied too. A `fragment` event's data is JSON
  `{"id": string, "html": string}`: the element with that ID is replaced by the HTML parsed in
  that element's context (inside an `<svg>` wrapper for an SVG target, else
  `Range.createContextualFragment`), with every `<script>` stripped before insertion, and `mount`
  runs over the inserted nodes. A fragment for an unknown ID is dropped. An `end` event (data
  `{}`) ends the stream; nothing after it, and nothing of a superseded post, is applied.
- **State and hooks.** The form carries `data-stream-state`: `running` from the post, `done`
  after `end`, `error` on a network failure, a body that ends without `end`, or a response that
  is not `text/event-stream`; on `error` the server-written `[data-stream-error]` element, which
  must sit inside that form, is unhidden. Each top-level element of the inserted HTML receives a
  bubbling `netray:stream-fragment` (detail: the ID), the form a bubbling `netray:stream-end`. Consumers build their behaviour on
  these attributes and events, never inside the island.
- **Tests.** `crates/web/islands/test/stream-post.test.ts`,
  `crates/web/islands/test/stream-wire.test.ts`,
  `crates/web/islands/test/stream-fixture.test.ts`.
- **Server half.** `crates/web/src/stream.rs` (`fragment(id, html)`, `end()`) is the only
  encoder of the events; its bytes are pinned to `crates/web/islands/test/fixtures/stream.sse`,
  which the island's fixture test parses: `crates/web/tests/it/stream_wire.rs`
  (`UPDATE_GOLDEN=1 cargo test -p netray-web --test it stream_wire` rewrites it).

## 6. Accessibility

- Semantic elements first: a `button` for an action, an `a` for navigation, a `form` for input,
  one `main` per page from the frame.
- State is an ARIA attribute (`aria-pressed`, `aria-current`, `aria-selected`), which is also
  what the marker selects on (section 4); a graphic that carries data has an `aria-label`.
- Colour is never the only carrier of a status: the status word or letter is in the text.
- Contrast: section 2.
- Motion: cross-document view transitions run only under
  `prefers-reduced-motion: no-preference`: `crates/web/tests/it/view_transition.rs`.

## 7. Testing

- **Islands.** vitest with happy-dom, one config (`crates/web/islands/vitest.config.ts`), tests
  in `crates/web/islands/test/`; they are the only frontend unit tests of V2. They run through
  `just test-frontend`, part of `just adlc-verify`.
- **The gate.** `crates/web/src/gate.rs`, a `cargo test` in `netray-web` run by
  `just adlc-verify`, enforces its three rules over `templates/**/*.html`, `static/**/*.css` and
  `islands/**/*.ts`: `crates/web/tests/it/gate.rs`.
- **The codename check.** No codename appears as a whole word in the tracked templates, static
  files, island sources or `compare.toml`: `tests/repo/test_no_codenames.sh`; nor in any rendered
  page, static file or header: `crates/web/tests/it/pages_codenames.rs`. Its word list: `lens`, `prism`, `tlsight`, `spectra`, `beacon`, `ifconfig-rs`.
- Test a page through the composed router, not the template alone: the frame, the headers and
  the CSP are added there.
