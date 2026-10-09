# Plan: parse cursor

## Phase 1 — Cursor on a char boundary

### Plan

- `crates/mhost-prism/src/api/parse.rs` `parse_handler`: after the clamp, round `cursor_pos` down to the previous char boundary; docs name byte offsets.
