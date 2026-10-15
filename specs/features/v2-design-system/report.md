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
