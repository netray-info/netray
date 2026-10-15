# Report: v2-design-system

## Phase 1 — Tokens

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | `crates/web` is `netray-web`, edition 2024, a library with no route, on `netray-common` by path and workspace askama 0.16; the binary does not depend on it | green |`tests/repo/test_web_crate.sh` |
| C2 | `crates/web/tokens.json` is the token source moved byte for byte; the SDD's four mentions, `CLAUDE.md` and `test_sdd_in_repo.sh` name it; the old file is gone; §3.1 gains `common` | green |`tests/repo/test_sdd_in_repo.sh` |
| C3 | `build.rs` writes `tokens.css` (light `:root`, dark media and `[data-theme]`, phone overrides, dark-only glow), names from the JSON path; `tokens::CSS` | green |`crates/web/tests/tokens.rs` |
| C4 | `build.rs` writes a Rust `tokens` module, one `&str` constant per value and theme | green |`crates/web/tests/tokens.rs` |
| C5 | a contrast test: `mark`/`hair` ≥ 3:1, `text` ≥ 4.5:1 against `bg` and `plate`, both themes, failure names colour, ground and ratio | green |`crates/web/tests/contrast.rs` |
| C6 | `--color-bg: #f7f8fa` on `:root`, `#07080a` under both dark selectors, `--font-size-h1: 26px` in the phone query | green |`crates/web/tests/tokens.rs` |
| C7 | `status.light.warn.text` becomes `--status-warn-text` | green |`crates/web/tests/tokens.rs` |
| C8 | `:root` sets `--status-glow-fail: none`; only the dark blocks set the shadow | green |`crates/web/tests/tokens.rs` |
| C9 | real tokens pass the contrast test; a `text` of `#9aa0a6` on light `bg` fails naming it | green |`crates/web/tests/contrast.rs` |
| C10 | `test_sdd_in_repo.sh` passes with `crates/web/tokens.json` and fails when it is missing | green |`tests/repo/test_sdd_in_repo.sh` |

The crate skeleton (8b3bc4b, empty library, askama in the workspace) was committed before the tests so they compile against the crate.

`tokens.json` moved byte for byte (verified against `8b3bc4b:specs/sdd/v2-tokens.json`), then gained `layout.phoneMax` (below).

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| G1 (token move, SDD, CLAUDE.md) | 0 (done by the implementer: a move and four text edits) | implementer | 0 | 0 |
| G2 (build.rs, tokens, contrast) | 3 | implementer, after a test fix | 36784 | 66 |

G2's coder stayed red on one test that could not pass: `weak_status_text_is_reported_with_path_hex_ground_and_ratio` wrote and read colours as bare strings while `tokens.json` stores `$value` nodes. The test was fixed (dee4dc9) and the coder's bare-string fallback in `contrast.rs` removed.

### Review

Reader over the phase diff: 1 blocker, 2 amendments.

| Class | Finding | Outcome |
|---|---|---|
| BLOCKER | `violations()` looped over fixed state and part names; an added state (`unmeasured`), a renamed part or a missing `status` key went unchecked | fixed: walks every state and part under both themes; no colours is a violation |
| AMENDMENT (phase 1, repaired) | a status colour present in one theme only was skipped; dark then falls back to the light value unchecked (`warn.text` 3.83:1 on dark bg) | fixed: a colour missing in the other theme is a violation |
| AMENDMENT (phase 1, repaired) | the phone query at `layout.phone` (390 px, a reference width) left 393-430 px phones on desktop sizes | operator 2026-10-14: new token `layout.phoneMax` = 480px, query there; SDD §8.2, spec R3 and the decision log updated |

Probes after the fix (a throwaway test, deleted): added `status.light.unmeasured.mark #e0e3e8` → 1.21 < 3.0 plus the missing dark twin; `status.dark.warn.text` removed → named; `warn.Text #eeeeee` → 1.09 < 3.0; `status` removed → "holds no status colours".

### Behavioural verification

skipped: a library with no entry point; the generated `tokens.css` is read by the tests.

## Phase 2 — The gate

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R6: the gate scans `templates/**/*.html`, `static/**/*.css`, `islands/**/*.ts` and fails on rule 1 (colour literal; radius, font-family, font-size, `font`, stroke-width or border width not a `var(--…)` of a `tokens.css` property), rule 2 (component class defined outside its own stylesheet or used outside its macro file), rule 3 (inline `<script>` other than `speculationrules`); each rule has a planted-violation fixture | green | `crates/web/tests/gate.rs` |
| C2 | the crate's real templates and stylesheets pass | green | `crates/web/tests/gate.rs` |
| C3 | `color: #fff` fails rule 1; `border-radius: var(--radius-max)` passes; `--r: 50%; border-radius: var(--r)` fails | green | `crates/web/tests/gate.rs` |
| C4 | `.check-row` defined outside `check_row.css` fails rule 2 | green | `crates/web/tests/gate.rs` |
| C5 | `<script>alert(1)</script>` fails rule 3; `<script type="speculationrules">` passes | green | `crates/web/tests/gate.rs` |

The real tree needs one scanned file for C2 (a scan of zero files proves nothing), so this phase adds `crates/web/static/base.css` (tokens only); the R43 marker rule joins it in Phase 3.

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| G1 (gate) | 2 | sonnet | 44257 | 137 |
| G2 (base.css) | 1 | sonnet | 24424 | 17 |
| G1 fix, first reading | 2 | sonnet | 78938 | 327 |
| G1 fix, second reading | 1 | sonnet | 67001 | 173 |

### Review

First reading (working tree over 4f18c14): 11 BLOCKER, 5 AMENDMENT. The operator settled the amendments (token family per property, extended colour list and CSS-wide keywords, derived component classes, string-only island scan; spec and SDD §8.5 in 6248268); 34 rows for blockers and amendments in 777c806; gate.rs reworked, all rows green.

Second reading (working tree over 777c806): 6 BLOCKER, 4 AMENDMENT, 3 NIT.

| Class | Where | Finding |
|---|---|---|
| BLOCKER | `gate.rs:395` | `{% … %}`/`{{ … }}` not opaque: a `>` or `<` in an askama expression breaks the tag split and bypasses all three rules (`<rect {% if n > 0 %}fill="#c42b22"{% endif %}/>`) |
| BLOCKER | `gate.rs:418` | SVG `font-size`/`font-family` attributes not token-checked (`<text font-size="40" font-family="Arial">`) |
| BLOCKER | `gate.rs:579` | vendor-prefixed radius and border widths unchecked (`-webkit-border-radius: 50%`) |
| BLOCKER | `gate.rs:68` | deprecated system colours pass (`WindowText`, `ButtonHighlight`) |
| BLOCKER | `gate.rs:456` | a `;` inside a quoted string ends the declaration; the rest goes unscanned (`url("data:image/svg+xml;utf8,…fill='red'…")`) |
| BLOCKER | `gate.rs:501` | false refusals: `grid-area: mark`, `animation-name: highlight`, `url(/static/red.png)`, `tan(45deg)`, `fill="url(#fade)"` |
| AMENDMENT | `spec.md:37` | rule 3 silent on inline event handlers and `javascript:` URLs, which the S25 CSP blocks at runtime |
| AMENDMENT | `spec.md:37` | SVG `rx`/`ry` radii not covered |
| AMENDMENT | `spec.md:37` | outline widths, `text-decoration-thickness`, a border shorthand without width (renders `medium`) not covered |
| AMENDMENT | `spec.md:37` | URL-encoded colours in SVG data URIs inside stylesheets not covered |
| NIT | `gate.rs:780` | `[class~="check-row"]` in a foreign stylesheet passes |
| NIT | `gate.rs:757` | a component class reaching `class` through an askama string passes |
| NIT | `gate.rs:395` | `<!-->` treated as an open comment |
| DEFERRED | islands | the CSS-like-string heuristic refuses `'Error: Mark failed'` and `'#add'`; island styling is settled in Phase 4 |

Resolution: the operator set the gate's threat model to honest mistakes, not deliberate evasion (10bee03), and took the six blockers plus the amendments for `rx`/`ry`, outline and decoration widths, and inline event handlers and `javascript:` URLs into requirement 6; 24 rows in e5f4a7b, all green. The data-URI percent-encoding amendment and the three NITs stay out by that decision. No third reading: every second-reading blocker is a row of the table.

Known limits after the fix, inside the threat model: a token attribute filled by askama (`stroke-width="{{ w }}"`) passes, the value being opaque; `outline: auto` and `text-decoration-thickness: auto` are refused, as requirement 6 allows only a token, a CSS-wide keyword or `0`/`none`.

### Behavioural verification

skipped: the phase adds a library function and a test, no CLI, route or UI action.

The phase's implement record says "58 planted rows"; the table holds 78 (20 + 34 + 24). Records are not edited, so the count is corrected here.

## Phase 4 — Identity

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R8: `static/logo.svg` is W5b: "net" regular and "ray" bold as SVG `<text>` in the `font.sans` stack, a tick scale under each, the marker line through the scale axis between them | green | `crates/web/tests/identity_logo.rs` |
| C2 | R8: `static/favicon.svg` and `static/badge-mark.svg` are the marker crossing the axis on a tile with at most 2 px radius | green | `crates/web/tests/identity_marks.rs` |
| C3 | R8: all three SVGs are monochrome: only `color.light.ink` and `color.dark.ink` (the dark one under an embedded `prefers-color-scheme: dark` rule), no status colour, no gradient; the logo's `font-family` is the `font.sans` stack; a test compares the literals with the token values | green | `crates/web/tests/identity_colours.rs` |
| C4 | R9: `templates/partials/tagline.html` holds the one tagline "Every check of a domain, in one spectrum."; no other template or stylesheet contains it | green | `crates/web/tests/tagline.rs` |
| C5 | GIVEN the three SVGs WHEN a test reads them THEN every colour equals `color.light.ink` or `color.dark.ink`, none is a status colour, there is no gradient, no radius over 2 px, and the logo's font stack equals `font.sans` | green | `crates/web/tests/identity_colours.rs` |
| C6 | GIVEN the tagline WHEN a test searches templates and stylesheets THEN it appears only in `templates/partials/tagline.html` | already_implemented | `crates/web/tests/tagline.rs` |

C6 passes before the phase (no copy of the tagline exists yet); its test is kept as a guard, and `search_helper_finds_a_planted_nested_copy` proves the walk sees nested files.

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| G1 (tagline partial) | 0 (done by the implementer: one line of text) | implementer | 0 | 0 |
| G2 (logo, favicon, badge mark) | 1 | sonnet | 31915 | 30 |
| G2 fix, reading (logo widths) | 1 | sonnet | 19772 | 13 |

The favicon and badge tile is the ink at `fill-opacity: 0.12` with the marks in full ink, not the canvas's solid ink tile with knocked-out marks. The plan took a knock-out to need `url(`/mask; the reader showed an ink-only `fill-rule="evenodd"` path draws it (first AMENDMENT below), which needs the operator's choice and a test change (the marks test reads `rect#tile`, `#axis`, `#marker`).

### Review

Reader over the working tree (production files only): 2 BLOCKER, 3 AMENDMENT, 1 DEFERRED.

| Class | Where | Finding | Outcome |
|---|---|---|---|
| BLOCKER | `static/logo.svg:10` | `textLength` 160/175 stretched both words ~30% past their natural width in the system font (SF Pro: "net" 125.0, "ray" 134.6 at em 100); W5b sizes each scale to its word | fixed: `textLength` 125/135, scales and marker recomputed, viewBox `0 0 280 160` |
| BLOCKER | `static/logo.svg:11` | the trailing `letter-spacing: -0.03em` inside `textLength` pushed the "y" past the viewBox; clipped as `<img>` | fixed: `letter-spacing` dropped (`textLength` sets the run), 4 units right margin |
| AMENDMENT (phase 4, repairable) | `static/favicon.svg:10` | the canvas favicon is the icon at S=20 centred (6 px inset) and knocked out of a solid ink tile; the build draws it at S=32 edge to edge in full ink over a 12% ink tile. R8 is silent on tile fill and inset; an ink-only evenodd path reproduces the canvas | open: operator's choice (see the return question) |
| AMENDMENT (phase 4, not repaired) | `static/logo.svg:6` | the SVGs switch ink on `prefers-color-scheme` only; `[data-theme="dark"]` (R3) sets no `color-scheme`, so a forced-dark page under a light OS shows `#0f1216` on `#07080a` (1.07:1) | open: `[data-theme="dark"]{color-scheme: dark}` belongs to R3 / `tokens.css` |
| AMENDMENT (phase 4, not repaired) | `static/badge-mark.svg:6` | the badge mark follows the viewer's scheme, but sits on the badge label colour the spec does not define (canvas: `#2a2f36` both schemes, mark `#ffffff`); light scheme gives 1.39:1 | open: badge label colour and mark ink for the badge renderer |
| DEFERRED | `static/badge-mark.svg:4`, `logo.svg:5` | element selectors (`rect`, `text, rect`) restyle every rect/text of a host document when the SVG is inlined | pages phase: inline use (badge renderer, `base.html`) scopes or drops the embedded style |

### Behavioural verification

skipped: no route or CLI; the SVGs were rendered with Quick Look (`qlmanage -t`) after the fix: words at natural width, the marker through the axis between "net" and "ray", the "y" inside the viewBox; the reader rendered all three in headless Firefox in both schemes.
