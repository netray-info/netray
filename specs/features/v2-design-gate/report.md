# Report: v2-design-gate

## Phase 1 — Manifest, states and page side

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 Manifest: `just design-check` reads `$NETRAY_DESIGN_DIR/manifest.json` (or the recipe's first argument); boards with title, host, path, width, theme, optional target, named elements (board selector, page selector, class, properties); an `accepted` list; refuses unknown key, missing title, duplicate element, class outside the three | green | tests/repo/test_design_manifest.sh |
| C2 | R2 States: the recipe seeds a fresh history store from the manifest's fixture dir, starts lens (and the boards' inspector services) on free ports with a ttl no seeded domain outlives, stops them on exit; a visit that starts a run is a `no state` row | green | tests/repo/test_design_states.sh |
| C3 | R4 Page side: each board's page opened at its width and theme (colour-scheme emulation and the theme switch), full-page screenshot to `target/design/<board>-page.png`, the named elements' properties read | green | tests/acceptance/design/page.spec.ts |
| C4 | GIVEN a manifest with an unknown key WHEN loaded THEN the loader exits 2 naming the key | green | tests/repo/test_design_manifest.sh |
| C5 | GIVEN a manifest with two elements of one name on one board WHEN loaded THEN it exits 2 naming the board and element | green | tests/repo/test_design_manifest.sh |
| C6 | GIVEN a manifest element with class `motion` WHEN loaded THEN it exits 2 naming the element and the three allowed classes | green | tests/repo/test_design_manifest.sh |
| C7 | GIVEN seeded fixtures and the large ttl WHEN the result page of a seeded domain is visited THEN the history store gains no entry | green | tests/repo/test_design_states.sh |
| C8 | GIVEN a board with viewport 390 and theme dark WHEN the page side runs THEN the screenshot is 390 px wide and the page reports a dark colour scheme | green | tests/acceptance/design/page.spec.ts |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 manifest loader | 1 | sonnet | 29369 | 26 |
| G2 stack helper | 1 | sonnet | 29078 | 27 |
| G3 page side | 1 | sonnet | 27757 | 14 |
| G4 runner and recipe | written by the orchestrator (no own test; C1, C2 covered by G1, G2 tests) | n/a | n/a | n/a |

### Behavioural verification

`just design-check <scratch manifest>` with two boards (history dark 1280 on `/shop.example.com`,
mobile light 390 on `/new.example.com`): exit 0; `target/design/page.json` reports per board
`colorScheme` and `theme` equal to the board's theme, `state: "fresh"`, and the measured values
(history dark: h1 `font-size: 40px`, `font-weight: 700`, body `background-color: rgb(7, 8, 10)`;
mobile: h1 `font-size: 26px`); `target/design/` holds `d3-analyzer-history-dark-page.png` and
`d3-mobile-result-light-page.png`. `cd tests/acceptance && npx playwright test --project design`:
2 passed.

After the reader's fixes: the same run exits 0; a board on `/unseeded.example.com` prints
`D3 · Mobile result — light: no state (data-state=none)` and exits 1; `check.mjs` without
`tests/acceptance/node_modules` prints `design-check: cannot launch chromium: Cannot find package
'@playwright/test' ...` and exits 2.

### Reader

| class | finding | disposition |
|---|---|---|
| BLOCKER | `check.mjs` exited 1 (not 2) when `@playwright/test` is missing or a write fails | fixed: dynamic import and every setup step route to exit 2 |
| AMENDMENT | design-dir default relative to the repository root misses netray-ops from a worktree | spec amended (default beside the main checkout); repaired in phase |
| AMENDMENT | boards whose title slugs collide overwrite each other's files | loader refuses them; spec amended; test row added |
| AMENDMENT | the live target's use was unspecified (`{target}` placeholder silent) | spec amended; loader refuses a target without placeholder and the reverse; test rows added |
| DEFERRED | pages without `#result` (inspectors, error pages) never counted as `no state` | fixed now: HTTP status 400 and above or no response is `no state` |
| NIT | stack comment misdescribed `CLIENT_BUDGET_PER_HOUR=0` | comment corrected |
| NIT | CLI guard failed through a symlinked path | guard compares the real path |

## Phase 2 — Board side, comparison and report

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R3 Board side: the export opened once, each board's iframe found by title, screenshot at full content height to `target/design/<board>-board.png`, the listed properties read inside the iframe | green | tests/acceptance/design/board.spec.ts |
| C2 | R5 Comparison: spacing within 4 px, typography sizes within 1 px with weight and first family exact, colour exact as lowercase hex with alpha; a selector matching nothing is a class-1 row; a board without a reachable state is a `no state` row | green | tests/repo/test_design_compare.sh |
| C3 | R6 Report: `target/design/report.md` with one four-cell row per deviation (`D<n>`, criterion, `open`, `design-check`), no pipe in a cell, the matched `accepted` entries, one side-by-side image per board | green | tests/repo/test_design_compare.sh |
| C4 | R7 Exit status: 0 within tolerance or accepted with every board in a state; 1 otherwise with the count per board; 2 on a manifest or setup error; a stale `accepted` entry fails with 1 | green | tests/repo/test_design_export.sh |
| C5 | GIVEN board font size 40px and page 41px (typography) WHEN compared THEN no row | green | tests/repo/test_design_compare.sh |
| C6 | GIVEN board font size 40px and page 44px WHEN compared THEN one row naming 40px and 44px | green | tests/repo/test_design_compare.sh |
| C7 | GIVEN board font weight 650 and page 700 WHEN compared THEN one row | green | tests/repo/test_design_compare.sh |
| C8 | GIVEN board gap 36px and page 40px (spacing) WHEN compared THEN no row; with page 41px THEN one row | green | tests/repo/test_design_compare.sh |
| C9 | GIVEN board colour rgb(255, 255, 255) and page #ffffff WHEN compared THEN no row | green | tests/repo/test_design_compare.sh |
| C10 | GIVEN a page selector that matches nothing WHEN compared THEN one class-1 row `element missing on page` | green | tests/repo/test_design_compare.sh |
| C11 | GIVEN a deviation matched by an `accepted` entry WHEN the report is written THEN the row is listed under accepted and the exit status ignores it | green | tests/repo/test_design_compare.sh |
| C12 | GIVEN an `accepted` entry that matches no deviation WHEN the check ends THEN it is printed as stale and the exit status is 1 | green | tests/repo/test_design_compare.sh |
| C13 | GIVEN a report row WHEN written THEN it has exactly four cells and no cell contains the pipe character | green | tests/repo/test_design_compare.sh |
| C14 | GIVEN the export path does not exist WHEN the check starts THEN it exits 2 naming the path | green | tests/repo/test_design_export.sh |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 comparator | 1 | sonnet | 30690 | 28 |
| G2 board side | 2 | sonnet | 31081 | 32 |
| G3 runner | written by the orchestrator (C4, C14 covered by `test_design_export.sh`) | n/a | n/a | n/a |

### Behavioural verification

Against the real export (private), a scratch manifest with one board (history light) and one
`accepted` entry for a property within tolerance: `just design-check` printed
`D3 · Analyzer — history (option 1+2), light: 3 deviation(s)`, `stale accepted: ... / h1 domain /
font-size`, exit 1; `target/design/report.md` held three four-cell rows (h1 font-weight board 650
page 700; re-check button height board 44px page 21px; background board rgb(15, 18, 22) page
rgb(239, 239, 239)) and the stale entry; `target/design/` held the board, page and side-by-side
PNGs (side-by-side checked by eye: board left, page right, same scale). First attempt exited 2:
the export's boards are sandboxed blob: iframes the parent cannot read, so `openExport` now waits
inside each frame. With missing export: exit 2 naming the path (`test_design_export.sh`).

### Reader

| class | finding | disposition |
|---|---|---|
| BLOCKER | box shorthands with different token counts (`0px 18px` against `0px 18px 2px`) were a deviation within tolerance | fixed: both sides expanded to four values; test rows added |
| AMENDMENT | an unreadable (misspelt) property read `''` on both sides and passed | fixed: an empty value never matches; spec amended; test rows added |
| AMENDMENT | R5 named offsets between elements, which no property measures | spec amended: distances are judged as hierarchy from the side-by-sides |
| AMENDMENT | an `accepted` entry of a board without a state was reported stale | fixed: neither matched nor stale; test row added |
| DEFERRED | inspector boards (HTTP 200, result over a stream) never become `no state` | deferred to the first feature whose manifest names an inspector board; decision log |
| NIT | duplicate `accepted` entries made one always stale | loader refuses duplicates; test row added |
