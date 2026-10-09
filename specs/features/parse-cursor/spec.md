# Spec: parse cursor

Status: Draft
Created: 2026-10-09

## Goal

`POST /api/parse` answers every well-formed request: a `cursor_pos` that falls inside a multi-byte UTF-8 character no longer panics the handler. Token ranges and `cursor_pos` stay byte offsets, as today.

## Non-goals

- Changing `cursor_pos` to character or UTF-16 offsets (the frontend does not call `/api/parse`).
- Any other `/api/parse` behaviour.

## Context and constraints

- `parse_handler` clamps `cursor_pos` to `input.len()` (`crates/mhost-prism/src/api/parse.rs:131`); the tokenizer works on bytes (`:152-162`), so token `from`/`to` are byte offsets; `completions_at` slices `&input[t.from..cursor_pos]` (`:264`), which panics when `cursor_pos` is not on a char boundary.
- Found by the backend-correctness Phase 4 reader (2026-10-09); operator: own small feature before the 0.23.0 release.
- The gate is `just adlc-verify`.

## Requirements

1. `parse_handler` moves a `cursor_pos` that is not on a UTF-8 char boundary down to the previous boundary before tokens and completions use it; the response is 200 with the completions for that position.

## Phase 1 — Cursor on a char boundary

**Depends on:** none
**Requirements:** 1

### Test Scenarios

- GIVEN input `"exämple.com @sy"` and `cursor_pos` inside the two bytes of `ä` WHEN `POST /api/parse` THEN 200 with no completions (the cursor is on the domain token).
- GIVEN input `"example.com @sÿ"` and `cursor_pos` between the two bytes of `ÿ` WHEN `POST /api/parse` THEN 200 with the completions for prefix `@s`.
- GIVEN input `"example.com @sy"` and `cursor_pos` 15 (end, ASCII) WHEN `POST /api/parse` THEN today's completions (`@system` when allowed).
- GIVEN `cursor_pos` beyond the input length WHEN `POST /api/parse` THEN 200, clamped as today.

## Decision log

- Round down to the previous char boundary, over rejecting the request with 400: a completion request should not fail on a mid-character cursor.

## Open decisions

None.

## Out of scope

- R5.8 (prism nameserver queries under the target policy).
