# Spec: Design gate — V2 pages measured against the design canvas

Status: Ready
Created: 2026-10-19

## Goal

A UI feature can prove that its pages match the binding boards of the design canvas before it
merges. `just design-check` renders every board state named in a manifest on a local V2 instance,
measures named elements on the board and on the page, compares them under fixed tolerances, and
writes one row per deviation in the shape of a `report.md` criteria row. It exits non-zero while a
measured deviation is neither fixed nor accepted. Visual classes (missing or extra elements,
hierarchy) are judged by a reader from side-by-side images the same run writes.

The operator's live review of 2026-10-19 found V2 diverging from the canvas in 67 places while
every gate was green: specs were built against tests and goldens, and no step compared a page with
a board. This feature adds that step; features 1–7 of the V2 gap closure use it as their gate.

## Non-goals

- No change to any page, template, stylesheet or island: fixing deviations is feature 1 onwards.
- No part of `just adlc-verify` or CI: the canvas is private and the check needs a browser.
- No pixel diffing: the measured channel compares computed values, the visual channel is a reader.
- No fixtures for inspector results; inspector boards run live against a host the manifest names.
- No canvas content in this repository: board exports, board PNGs and the manifest stay private.

## Context and constraints

- The canvas is a private claude.ai design artifact. Its HTML export (one self-contained file,
  each board an unscaled iframe at its design width, desktop 1280, mobile 390, light and dark as
  separate boards) and a manifest live in the private netray-ops checkout under
  `specs/netray/design/canvas/`. This repository cites it as "(private)" and reads its location
  from `NETRAY_DESIGN_DIR` (default `../netray-ops/specs/netray/design/canvas` relative to the
  main checkout, the parent of git's common directory, so a feature worktree finds it too).
- Rendering the export needs a page viewport taller than the tallest board (3200 px), or boards
  render cut off (seen 2026-10-19).
- The result page `GET /<domain>` starts a live run when the newest stored entry is older than
  lens's `[cache] ttl_seconds` (default 300). The check runs lens with a large `ttl_seconds` and
  seeds history right before it, so stored states render without a live run.
- `just acceptance-local` already seeds `history.db` from `tests/acceptance/fixtures/history/` and
  starts the services on free ports; Playwright and its browsers come from `tests/acceptance`.
- Headless Chromium on the operator's machine has no route to the internet (a local network rule);
  localhost works. Inspector boards that need a live target name it in the manifest; a target that
  cannot be reached produces a `no state` row, never a pass.
- Artifacts agents read are English; no real domains in committed fixtures (`example.com` family).

## Requirements

1. **Manifest.** `just design-check` reads a JSON manifest (`$NETRAY_DESIGN_DIR/manifest.json`,
   overridable by the recipe's first argument). Per board it names: the board's title in the export,
   the V2 host and path, viewport width, theme (`light` or `dark`), an optional live target (placed
   into the path at `{target}`; a target and the placeholder come together or not at all), and a
   list of named elements, each with a board selector, a page selector, a class (`spacing`,
   `typography`, `colour`) and the CSS properties to compare. The manifest also holds an `accepted`
   list (board, element, property, reason) for operator-decided "deliberately different" rows. The
   loader refuses an unknown key, a missing board title, a duplicate element name within a board,
   a class outside the three, two board titles that name the same output files, and a target without
   its placeholder (or the reverse), each with a message naming the entry.
2. **States.** The recipe seeds a fresh history store from a fixture directory the manifest names
   (default `tests/acceptance/fixtures/history/`), starts lens (and the inspector services the
   manifest's boards use) on free ports with `[cache] ttl_seconds` large enough that no seeded
   domain starts a live run during the check, and stops them on exit. A page visit that starts a run
   anyway is reported as a `no state` row for that board.
3. **Board side.** The check opens the export once, finds each manifest board's iframe by title,
   screenshots it at full content height into `target/design/<board>-board.png`, and reads the
   computed value of each listed property for each named element from inside the iframe.
4. **Page side.** For each board the check opens the V2 page at the board's viewport width with the
   board's theme (`prefers-color-scheme` emulation and the page's own theme switch when it has one),
   takes a full-page screenshot into `target/design/<board>-page.png`, and reads the same properties
   of each named element.
5. **Comparison.** A property is within tolerance when: `spacing` (margins, paddings, gaps,
   heights, widths; box shorthands compared side by side after expanding to four values; distances
   between elements are judged by the visual channel under hierarchy) differs by at most 4 px; `typography` (font
   size, line height, letter spacing) by at most 1 px, with font weight and font family (first
   family) exact; `colour` (colour, background colour, border colour) exact after normalising to
   lowercase hex with alpha. An empty value (a property the browser cannot read) never matches. A selector that matches nothing on either side is a class-1 row
   (`element missing on page` or `element missing on board`). A board without a reachable state is
   a `no state` row.
6. **Report.** The check writes `target/design/report.md` with one four-cell table row per
   deviation: id `D<n>`; criterion `<board> <width>/<theme>: <element> <property> board <value>
   page <value> (<class>)`; status `open`; test file `design-check`. No cell contains the pipe
   character, so the rows paste into a feature's `report.md` criteria table unchanged; then a list of `accepted` entries that matched; then one side-by-side image per board,
   `target/design/<board>-sbs.png`, board left and page right at the same scale, for the visual
   channel.
7. **Exit status.** Exit 0 when every measured property is within tolerance or accepted and every
   board has a state; exit 1 otherwise, printing the deviation count per board; exit 2 on a manifest
   or setup error (export missing, manifest invalid, a service not answering), printing the cause.
   An `accepted` entry that no longer matches a deviation is printed as stale and fails with exit 1.
8. **Rules.** `specs/rules/design-rules.md` states the method: the five classes and their
   tolerances, what does not count (sample data, states the canvas does not show, rendering noise),
   the three dispositions (rebuild; deliberately different, recorded in the feature's decision log
   and the manifest's `accepted`; adjust canvas, then re-export and re-measure), that every
   deviation row starts `open` and keeps `adlc next` on implement, that the visual classes are
   judged by a reader from the side-by-sides with each finding added as a row, and that a UI
   feature runs `just design-check` before review. CLAUDE.md's rules table names it for any change
   under `crates/web/templates`, `crates/web/static` or `crates/web/islands`.
9. **Verified on main.** Run against main with a manifest covering the binding result boards
   (history and first visit, light and dark, 1280; mobile 390 light), the check reports the
   deviations the 2026-10-19 inventory measured for those boards, at least: h1 domain size and
   weight, the re-check button's height and background, and the missing score line.

## Phase 1 — Manifest, states and page side

**Depends on:** none
**Requirements:** 1, 2, 4

A Playwright-driven runner under `tests/acceptance/design/` (sharing the acceptance package's
dependencies and browsers), a `just design-check` recipe that seeds, starts and stops the services
as `acceptance-local` does, and the manifest loader with its refusals. Unit tests for the loader
run without a browser.

### Test Scenarios

GIVEN a manifest with an unknown key WHEN loaded THEN the loader exits 2 naming the key.
GIVEN a manifest with two elements of one name on one board WHEN loaded THEN it exits 2 naming the board and element.
GIVEN a manifest element with class `motion` WHEN loaded THEN it exits 2 naming the element and the three allowed classes.
GIVEN seeded fixtures and the large ttl WHEN the result page of a seeded domain is visited THEN the history store gains no entry.
GIVEN a board with viewport 390 and theme dark WHEN the page side runs THEN the screenshot is 390 px wide and the page reports a dark colour scheme.

## Phase 2 — Board side, comparison and report

**Depends on:** 1
**Requirements:** 3, 5, 6, 7

Board rendering from the export, property reads inside the board iframes, the comparator with the
tolerances, the report and the side-by-side images, and the exit status. The comparator is a pure
function with unit tests over synthetic board and page values.

### Test Scenarios

GIVEN board font size 40px and page 41px (typography) WHEN compared THEN no row.
GIVEN board font size 40px and page 44px WHEN compared THEN one row naming 40px and 44px.
GIVEN board font weight 650 and page 700 WHEN compared THEN one row.
GIVEN board gap 36px and page 40px (spacing) WHEN compared THEN no row; with page 41px THEN one row.
GIVEN board colour rgb(255, 255, 255) and page #ffffff WHEN compared THEN no row.
GIVEN a page selector that matches nothing WHEN compared THEN one class-1 row `element missing on page`.
GIVEN a deviation matched by an `accepted` entry WHEN the report is written THEN the row is listed under accepted and the exit status ignores it.
GIVEN an `accepted` entry that matches no deviation WHEN the check ends THEN it is printed as stale and the exit status is 1.
GIVEN a report row WHEN written THEN it has exactly four cells and no cell contains the pipe character.
GIVEN the export path does not exist WHEN the check starts THEN it exits 2 naming the path.

## Phase 3 — Rules, manifest for the result boards, verification on main

**Depends on:** 2
**Requirements:** 8, 9

`specs/rules/design-rules.md`, the CLAUDE.md rules-table row, a first private manifest covering the
binding result boards (written to `$NETRAY_DESIGN_DIR/manifest.json` in the netray-ops checkout,
not committed here), and a run against main whose deviation list goes into this feature's
`report.md` under `### Behavioural verification`.

### Test Scenarios

GIVEN the result-board manifest and main WHEN `just design-check` runs THEN it exits 1 and the report lists the h1 domain's size or weight, the re-check button's height or background, and the score line as missing.
GIVEN CLAUDE.md WHEN read THEN its rules table names `specs/rules/design-rules.md` with the three frontend paths.

## Decision log

- Built on main, not on `feature/v2-cutover`: features land on main and the cutover re-merges main
  when it resumes; the design check runs against lens's result pages, which main serves
  (implementer under the operator's autonomy mandate, 2026-10-19).
- Board values are measured from the rendered export, not transcribed from board source: the export
  is the canvas as the operator sees it, and one measuring path serves both sides (implementer
  under the operator's autonomy mandate, 2026-10-19).
- Inspector boards run live instead of on recorded facts; recorded inspector fixtures wait until a
  board needs a state live targets cannot give (implementer under the operator's autonomy mandate,
  2026-10-19).
- Classes, tolerances and dispositions as agreed with the operator on 2026-10-19 (thinking-mode
  session; operator decision).
- Phase 1 review amendments: the design directory's default resolves beside the main checkout,
  not the worktree; a board's live target enters its path at `{target}`; board titles whose file
  slugs collide are refused; a page answering HTTP 400 or above is a `no state` row, as is a result
  page whose stored state is not fresh (implementer under the operator's autonomy mandate,
  2026-10-19).
- Phase 2 review amendments: distances between named elements leave the measured spacing class
  (no CSS property carries them; the manifest names properties) and are judged as hierarchy from
  the side-by-sides; box shorthands are compared after expansion to four sides; an empty value
  never matches; an `accepted` entry of a board without a state is neither matched nor stale;
  duplicate `accepted` entries are refused. A `no state` signal for inspector boards (pages that
  answer 200 and load their result over a stream) is deferred to the first feature whose manifest
  names an inspector board: this feature's manifest covers result boards only, where the signal is
  the HTTP status and `data-state` (implementer under the operator's autonomy mandate, 2026-10-19).
- Final review: a blocker (spacing values mixing px and keyword tokens compared as whole
  strings) is fixed on this branch rather than splitting the change; a board without a state still
  gets its board image and side-by-side; the manifest's optional `export` key (the export's path,
  relative to the manifest, default `export/canvas.html`) and the run's `target/design/page.json`
  (the page side's raw values) are kept as named here; the second pass's blocker (fractional px
  values exactly at the tolerance failing by float error) is a different defect, fixed the same
  way, and a third pass reads the result (implementer under the operator's autonomy mandate,
  2026-10-19).
- Spec drafted and approved by the implementer under the operator's mandate for features 0–7
  (operator, 2026-10-19).

## Open decisions

None.
