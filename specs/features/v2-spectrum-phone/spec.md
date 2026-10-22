# Spec: v2-spectrum-phone

Status: Ready for Implementation
Created: 2026-10-22

## Goal

The spectrum's labels are placed on the phone geometry as well as on the desktop one, so a label
box at 390 px covers no neighbour's leader and overlaps no box of its row wherever the desktop
placement is clear. Today `spectrum::layout` places labels once, on the 456 px plate at 11 px
captions, and the phone draws that placement on its 358 px plate at 10 px captions: the plate
shrinks by 21 % and the boxes by 9 %, so boxes reach leaders and boxes they cleared on the
desktop (v2-ui-polish R9, "at 390 DMARC's leader crosses MTA-STS").

## Non-goals

- The dense-cluster fallback (SDD §5.5 point 2): a cluster whose clear placement needs more rows
  than three and than its line-order placement keeps the line order at both widths, and its
  leaders pass labels (v2-ui-polish decision). The example.com fixture's twelve adjacent HTTP
  labels are such a cluster.
- Phones narrower than the 390 px reference: their plate is narrower than the phone placement's.
- Board edits: the canvas round that adjusts board M follows later; deviations are proposals.
- The OG card (its own fixed width) and the inspector band (no labels).

## Context and constraints

- SDD §5.4 (width, phone tokens) and §5.5 (placement); `crates/web/src/spectrum.rs` is the only
  label placement and stays pure (`tests/repo/test_spectrum_module.sh`, an allowlist).
- Spectrum and scoring stay consistent (CLAUDE.md): this feature changes no tone, height or
  muting, only label rows.
- The phone media query is `layout.phoneMax` (480 px); the phone tokens `font.size.captionPhone`
  (10 px), `spectrum.labelLeaderPhone`, `labelRowPhone`, `labelBandPhone`.
- Background: `specs/features/v2-ui-polish` (decision log on R9, report rows R9 and N1-N22),
  `specs/solutions/2026-10-22-leader-clear-labels-had-no-solution-in-a-dense-run.md`.

## Requirements

1. **Phone placement.** `spectrum::layout` places each run's labels a second time, at the phone
   plate: `layout.phone` less two `layout.gutterPhone` (358 px), its pitch the plate over the line
   count with no minimum (the phone plate never scrolls), boxes measured at
   `font.size.captionPhone`, by the same rule as SDD §5.5 point 2. Each run layout carries the
   phone labels and their row count (at least three).
2. **Phone drawing.** At phone width (`layout.phoneMax` or narrower) the spectrum draws the
   phone placement: each label's row and edge clamp come from its phone label (its box width at
   the phone caption), each leader rises to its phone row, and the label band is
   `spectrum.labelBand` plus one `spectrum.labelRow` per phone row beyond the third, where phone
   rows is the largest phone row count of any run. Wider, the drawing is unchanged.
3. **No crossings where the desktop is clear.** A run whose desktop placement crosses no leader
   and overlaps no box crosses none at the phone placement either, on every history fixture; the
   SDD §5.5 is amended in the same commit as the code.

### Acceptance

4. **Visual check.** Screenshots at 1280 and 390 px, light and dark, of the result pages of the
   dense fixture runs (example.com, shop.example.com with history,
   checkout.eu-central.storefront.example.com) show no leader crossing a label outside a dense
   cluster at 390 px, and the 1280 px drawing unchanged. `just design-check` is run; its
   deviations on board M's label band are logged as canvas proposals, not halts.

## Phase 1 — Phone placement and drawing

**Depends on:** v2-ui-polish merged
**Requirements:** 1, 2, 3, 4

### Test Scenarios

GIVEN 57 lines with one failing line WHEN placed for the phone THEN the plate is 358 px, the pitch 358/57 px, and the box 0.62 em x 10 px per character plus 4 px wide, at its leader.
GIVEN any line set of 2 to 60 lines WHEN placed for the phone THEN every flagged line keeps a label, rows are at least three, no two boxes of one row overlap, and a box at its leader covers no higher leader unless its cluster fell back to the line-order placement.
GIVEN each history fixture as the result page lays it out WHEN laid out THEN each run's phone labels and rows equal the phone placement of its tones.
GIVEN each history fixture WHEN laid out THEN a run clear at the desktop placement is clear at the phone placement, and only the two dense example.com fixtures cross, at both widths.
GIVEN HSTS, XFO and XCTO warning WHEN the desktop placement is drawn at the phone plate THEN HSTS's box covers XFO's leader, and the phone placement crosses nothing.
GIVEN the example.com fixture WHEN the spectrum renders THEN its root carries the phone band's extra rows, and each label and leader carries its phone row, each label its phone box width and its leader position.
GIVEN the slim inspector band WHEN rendered THEN its root carries no phone rows.
GIVEN the stylesheet WHEN read THEN at 480 px or narrower the label's bottom and edge clamp, the leader's height and the label band's height read the phone values.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate (operator,
  2026-10-19, extended 2026-10-22).
- Operator decision (2026-10-22): place the labels on the phone geometry; the label band height
  may change on every board, and board M's measured deviations are canvas proposals.
- The phone placement uses the 390 px reference phone's plate, 358 px: placed at the narrowest
  plate of its class, it holds at every wider phone plate up to 448 px (480 px viewport), as
  §5.5's `scaled_start` argument holds for the desktop. Phones narrower than 390 px are not
  covered (implementer under the operator's autonomy mandate, 2026-10-22).
- The phone values override the desktop inline geometry from the phone media query with
  `!important`, reading custom properties (`--row-phone`, `--width-phone`, `--x`,
  `--label-extra-phone`) the component writes beside the desktop styles: the desktop markup and
  its tests stay byte-identical in the properties they read (implementer under the operator's
  autonomy mandate, 2026-10-22).
- "No leaders cross at 390 px on dense runs" is met where the desktop placement is clear; a
  dense cluster (example.com's twelve adjacent HTTP labels: the clear placement needs twelve rows,
  the line order five) keeps the line-order fallback at both widths, by v2-ui-polish's decision.
  Raising the fallback threshold would stack every label of such a cluster above its neighbour;
  listed as a proposal (implementer under the operator's autonomy mandate, 2026-10-22).
