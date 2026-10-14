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
