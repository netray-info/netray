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
