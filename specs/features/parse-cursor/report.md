# Report: parse cursor

## Phase 1 — Cursor on a char boundary

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R1: a `cursor_pos` off a char boundary rounds down; 200 with that position's completions | green | crates/mhost-prism/tests/parse_cursor.rs |
| C2 | `exämple.com @sy`, cursor inside `ä` → 200, no completions | already_implemented | crates/mhost-prism/tests/parse_cursor.rs |
| C3 | `example.com @sÿ`, cursor inside `ÿ` → 200, completions for `@s` | green | crates/mhost-prism/tests/parse_cursor.rs |
| C4 | ASCII end cursor → today's completions | already_implemented | crates/mhost-prism/tests/parse_cursor.rs |
| C5 | cursor beyond length → 200, clamped | already_implemented | crates/mhost-prism/tests/parse_cursor.rs |

C3 failed at the baseline (`6506c0c`) with the panic "end byte index 15 is not a char boundary" at `parse.rs:264`; C2 passes there because a cursor on the domain token never slices; C4 and C5 pin today.

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| cursor | 1 | sonnet | 16380 | 170 |

### Review

The phase diff is four lines in `parse_handler` (round down with `is_char_boundary`, which holds at 0, so the loop ends) plus doc text naming byte offsets; the tokenizer splits on ASCII whitespace, so its ranges are char boundaries and no other slice can panic. The feature review reads the range independently.

### Behavioural verification

skipped: driven in-process through the router by `crates/mhost-prism/tests/parse_cursor.rs`.
