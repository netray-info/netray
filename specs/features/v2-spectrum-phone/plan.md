# Plan: v2-spectrum-phone

## Phase 1 — Phone placement and drawing

## Groups

- G1 (C1-C12): one group; every criterion touches the spectrum module, the spectrum component or its stylesheet.

## Plan

- `crates/web/src/spectrum.rs`: `place` keeps the desktop plate and pitch and hands placement to `place_on(lines, plate, pitch, width)`; `place_phone` calls it at `phone_plate()` (`layout.phone` less two `layout.gutterPhone`) with `phone_label_width` (`font.size.captionPhone`); `layout` stores `phone_labels` and `phone_rows` on each `RunLayout`.
- `crates/web/src/components.rs`: `SpectrumLabel` gains `phone_row` and `phone_width` (from the run's phone label of the same line), `SpectrumView` gains `phone_rows` and `phone_extra()`.
- `crates/web/templates/components/spectrum.html`: the non-slim root writes `--label-extra-phone`; leaders write `--row-phone`; labels write `--x`, `--row-phone`, `--width-phone` after the desktop geometry.
- `crates/web/static/spectrum.css`: in the phone media query, band height, leader height, label bottom and edge clamp read those properties, `!important` over the inline desktop geometry.
- `specs/sdd/v2.md` §5.5 points 2 and 3 amended.
