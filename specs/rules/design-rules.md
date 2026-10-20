# Design rules

How a V2 page is measured against the design canvas before a UI feature merges. The canvas is
private (claude.ai design artifact); its HTML export, board images and the manifest live in the
private netray-ops checkout under `specs/netray/design/canvas/` (cited here as "(private)").
Binding are the reference boards the canvas README names: `D3 · Analyzer — history …` and
`D3 · Analyzer — first visit …` (result page), `D3 · History: trace with time lens …`,
`D3 · TLS inspector …`, `D3 · Mobile result …`, `D3 · W5 Scale, developed` and `D3 · Tokens …`.
The other boards (A–D2 and the D3 logo and wordmark explorations) are context, not reference.

## When

A feature that changes anything under `crates/web/templates`, `crates/web/static` or
`crates/web/islands` runs `just design-check` after its last phase and before the review, and
pastes the deviation rows into its `report.md`. The check is not part of `just adlc-verify` or CI:
it needs the private canvas and a browser.

## The check

`just design-check [manifest]` reads the manifest (default `$NETRAY_DESIGN_DIR/manifest.json`;
`NETRAY_DESIGN_DIR` defaults to `../netray-ops/specs/netray/design/canvas` beside the main
checkout), seeds lens's history from the manifest's fixture directory, starts lens (and the
inspectors the boards name) with a cache ttl no fixture outlives, renders each board from the
export and its page at the board's width and theme, and writes `target/design/`:
`report.md`, `page.json`, `<board>-board.png`, `<board>-page.png` and `<board>-sbs.png` (board
left, page right, same scale). A board whose page answers HTTP 400 or above, or whose result page
holds no fresh stored state, is one `no state` row instead of its measured rows. Exit 0: everything
within tolerance or accepted; 1: open deviations, a `no state` row or a stale accepted entry;
2: manifest or setup error.

A manifest board names the board's title in the export, host, path (a live target enters at
`{target}`), width, theme and its elements: a name, a board selector, a page selector (Playwright
selectors), a class and the CSS properties to compare. An optional `state` names one more page state of
the same canvas board (a long domain, full multi-address evidence): the board is measured again at
its own path, its files are `<title slug>-<state slug>-*` and its rows and `accepted` entries name
it "<title> [<state>]".

## Five classes

Class 1 is missing or extra elements; the report labels such rows `(class 1)`.

Measured by the check, from computed values:

| class | properties | tolerance |
|---|---|---|
| spacing | margins, paddings, gaps, heights, widths (box shorthands compared per side) | 4 px |
| typography | font size, line height, letter spacing | 1 px; font weight and first font family exact |
| colour | colour, background colour, border colour | exact after normalising to lowercase hex with alpha |

An empty computed value (a misspelt property) never matches.

Judged by a reader from the side-by-side images:

| class | what |
|---|---|
| missing or extra elements | a part the board shows and the page lacks, or the reverse (a selector that matches nothing is measured as a class-1 row) |
| hierarchy | order, grouping, emphasis and placement of the parts, distances between them |

Each visual finding is added to the feature's `report.md` as its own criteria row, in the same
shape as the measured rows.

## What does not count

- Sample data: domains, dates, counts, grades and finding texts differ between board and page.
- States the canvas does not show: a page state without a board is not a deviation.
- Rendering noise: anti-aliasing, sub-pixel layout, font hinting within the tolerances above.

## Dispositions

Every deviation row starts `open` in the feature's `report.md`, which keeps `adlc next` on
implement. A row is closed by setting its status to `green` once it is resolved one of three ways:

1. **Rebuild**: change the page until the row disappears from the next run.
2. **Deliberately different**: the operator decides the page stays; the decision goes into the
   feature's decision log. A measured value row also gets an entry (board, element, property,
   reason) in the manifest's `accepted` list, so later runs list it under accepted; an accepted
   entry that no longer matches a deviation is stale and fails the check until removed. For a
   missing element the page deliberately lacks, the element leaves the manifest instead; a visual
   row needs only the decision log.
3. **Adjust canvas**: the board is wrong; change the canvas, re-export it, and re-measure.
