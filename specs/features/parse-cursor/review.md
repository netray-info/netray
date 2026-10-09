# Review: parse cursor

## f92667b..1911523

### Reader

COUNTS blockers=0 majors=0 minors=1
LENSES Engineering, Testing

MINOR | crates/mhost-prism/tests/parse_cursor.rs:101 | `cursor_beyond_input_is_clamped` checks only a 200 with a completions array, so it passes with the clamp deleted. | Remove `.min(input.len())` at `parse.rs:132`: the rounding loop walks 999 down to 15 and the test still passes. Fixed after the reading: the test compares with the completions at byte 15 (`ADLC-Test-Change`).

Traced sound: the rounding loop ends (`is_char_boundary(0)`); the tokenizer splits on ASCII whitespace, so no other slice can panic; the test offsets round as intended; no frontend calls `/api/parse`; the baseline trailer names the parent.

### Refuted

None: no BLOCKER or MAJOR. Anchor check skipped for this reading's single MINOR (quoted inline above).

### Calibration

No refuter ran. verified 0, held 0.

### Roll call

Not run for this four-line change: no principle's procedure touches `parse_handler`'s cursor handling (no outbound call, no new check, no config, no generated file).

### Summary

0/0/1 before and after refutation. verified 0, held 0.
