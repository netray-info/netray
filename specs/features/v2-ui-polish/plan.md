# Plan: v2-ui-polish

## Phase 1 — Design gate and spectrum leaders

## Groups

- G1: C1, C3, C4, C5, C6, C7 (design-check tooling)
- G2: C2, C8, C9 (spectrum label placement)

## Plan

- G1: `tests/acceptance/design/manifest.mjs` (`ELEMENT.optional` gains `shell`; `checkElement` refuses a non-boolean), `tests/acceptance/design/compare.mjs` (new pure `liveState(board, pageValues, waited)`), `tests/acceptance/design/page.mjs` (`measurePage` returns `waited`: true, false, or null without `wait`), `tests/acceptance/design/check.mjs` (pushes `noStateDeviation(board, reason)` when `liveState` returns one). The private manifest marks the TLS boards' shell elements.
- G2: `crates/web/src/spectrum.rs` `place`: labels placed in a topological order of the cover relation (a label after every label whose leader its box covers, mutual pairs exempt, ties by descending x), each in the lowest row with no overlap in that row and above every placed label whose leader it covers. `specs/sdd/v2.md` §5.5 point 2 amended.
