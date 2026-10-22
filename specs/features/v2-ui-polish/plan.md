# Plan: v2-ui-polish

## Phase 1 — Design gate and spectrum leaders

## Groups

- G1: C1, C3, C4, C5, C6, C7 (design-check tooling)
- G2: C2, C8, C9 (spectrum label placement)

## Plan

- G1: `tests/acceptance/design/manifest.mjs` (`ELEMENT.optional` gains `shell`; `checkElement` refuses a non-boolean), `tests/acceptance/design/compare.mjs` (new pure `liveState(board, pageValues, waited)`), `tests/acceptance/design/page.mjs` (`measurePage` returns `waited`: true, false, or null without `wait`), `tests/acceptance/design/check.mjs` (pushes `noStateDeviation(board, reason)` when `liveState` returns one). The private manifest marks the TLS boards' shell elements.
- G2: `crates/web/src/spectrum.rs` `place`: labels placed in a topological order of the cover relation (a label after every label whose leader its box covers, mutual pairs exempt, ties by descending x), each in the lowest row with no overlap in that row and above every placed label whose leader it covers. `specs/sdd/v2.md` §5.5 point 2 amended.

## Phase 2 — Inspector shell

## Groups

- G1: C1, C2, C7, C8, C9 (DNS builder and +check note)
- G2: C3, C10, C11 (run actions, evidence view head)
- G3: C4, C12, C13 (refusal notice frame)
- G4: C5, C6, C14, C15 (check list card, phone header)

## Plan

- G1: `templates/partials/inspect_dns_builder.html` (card of three labelled rows, toggle labels, note), `src/pages/inspect.rs` (`DnsBuilder.check_note` from the prefill), `islands/src/dns.ts` (the note follows the field), `static/inspector_shell.css` (builder rules replace the fieldset rules).
- G2: `static/inspector_shell.css` (phone run actions), `templates/pages/evidence.html` and `static/pages.css` (`.evidence-view__head`).
- G3: `templates/partials/inspect_notice.html`, `src/pages/inspect.rs` (`NoticeFragment` tone and examples; `examples(tool)` shared with the shell; `notice(.., warn)`), `static/inspector_shell.css` (notice card).
- G4: `templates/partials/inspect_band.html` (`.inspector-checks__card`), `static/inspector_shell.css` (card, phone header).

## Phase 3 — Evidence view, facts grid and guides

## Groups

- G1: C1, C4, C5, C6, C7 (evidence view)
- G2: C2, C8, C9 (wide facts)
- G3: C3, C10 (guides)

## Plan

- G1: `src/explore.rs` (`ExploreLink::target`), `templates/pages/evidence.html` (Explore live section), `templates/partials/evidence_section.html` (`.evidence-section__rows` card), `src/components.rs` (`EvidenceShape::is_empty`, `swipes`), `templates/components/evidence_block.html`, `static/evidence_block.css`, `static/pages.css`.
- G2: `src/components.rs` (`FACT_WIDE_CHARS`, `FactCell::wide`), `templates/components/facts_grid.html`, `static/facts_grid.css`.
- G3: `templates/pages/guide.html` (`article.guide-article`), `static/pages.css`, `static/query_input.css` (phone tool card).
