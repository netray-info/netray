# Intent: parse cursor

## Problem

`POST /api/parse` panics on a `cursor_pos` inside a multi-byte character.

## Proposed outcome

The request is answered; the cursor rounds down to a char boundary.

## Affected users and systems

Callers of prism's public `/api/parse`.

## Constraints

Ships in 0.23.0.

## Open questions

None.
