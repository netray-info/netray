# Plan: v2-design-gate

## Phase 1 — Manifest, states and page side

### Groups

G1: C4, C5, C6 · G2: C7 · G3: C3, C8 · G4: C1, C2. No production file is shared; G3 imports
`slug` from G1's `manifest.mjs` without editing it; G4 is the integration layer (recipe and runner).

### Plan

- **G1** `tests/acceptance/design/manifest.mjs` (new): `ManifestError`, `slug(title)` (the canvas
  renderer's rule), `loadManifest(path)` refusing unknown keys at every level, a missing title, a
  duplicate element per board, a class outside spacing/typography/colour, bad theme/width/properties;
  CLI guarded by `import.meta.url`: validate, `--fixtures` (absolute, relative to the repo root,
  default `tests/acceptance/fixtures/history`), `--hosts`; every failure exit 2.
- **G2** `tests/acceptance/design/stack.sh` (new): `design_stack_up <tmp> <fixtures> [host...]` on
  the helpers of `tests/repo/lib/netray.sh`; seeds `<tmp>/history.db`, starts lens with
  `LENS_CACHE__TTL_SECONDS=315360000` and `LENS_ADMISSION__CLIENT_BUDGET_PER_HOUR=0` plus the named
  inspectors (rows as `acceptance-local`, which stays unchanged); exports `LOCAL_<HOST>_URL`;
  returns 2 on failure; no own EXIT trap.
- **G3** `tests/acceptance/design/page.mjs` (new): `measurePage(browser, board, baseUrls, outDir)`:
  context at the board width with colour-scheme emulation, localStorage `theme` init script,
  goto, reads colour scheme, `data-theme`, `#result` `data-state`, computed properties (null when
  the selector matches nothing), full-page screenshot `<slug>-page.png`.
- **G4** `tests/acceptance/design/check.mjs` (new) and `justfile` recipe `design-check manifest=""`:
  resolve `NETRAY_DESIGN_DIR` (default beside the main checkout), validate, seed and start via
  `stack.sh`, run the page side per board, print `no state` for a non-fresh result, exit 0/1/2.

## Phase 2 — Board side, comparison and report

### Groups

G1: C2, C5–C13 (comparator, report rows, summary) · G2: C1 (board side, side-by-side) ·
G3: C3, C4, C14 (runner: export check first, board and page sides, report file, exit status).
G3 imports G1 and G2 without editing them.

### Plan

- **G1** `tests/acceptance/design/compare.mjs` (new, pure, no dependencies): `normalizeColour`,
  per-class tolerance (`spacing` 4 px component-wise; `typography` 1 px for sizes, weight and first
  family exact; `colour` exact `#rrggbbaa`; non-px values exact), `deviations`, `noStateDeviation`,
  `criterion`, `applyAccepted`, `reportRows` (pipes in cells become `/`), `reportMarkdown`, `summary`.
- **G2** `tests/acceptance/design/board.mjs` (new): `openExport` (file URL, 1600x3200 viewport,
  waits for every iframe body), `measureBoard` (iframe by exact title, grown to content height,
  element screenshot `<slug>-board.png`, computed values inside the frame), `sideBySide` (one page
  holding both PNGs at natural size, full-page screenshot).
- **G3** `tests/acceptance/design/check.mjs`: resolve the export via `resolveExport` and exit 2
  naming it before service URLs or the browser; per board: page side (a `no state` deviation for
  HTTP >= 400 or a non-fresh result), board side, deviations, side-by-side; then `applyAccepted`,
  `target/design/report.md`, the summary lines, exit `summary.status`.

## Phase 3 — Rules, manifest for the result boards, verification on main

### Groups

G1: C1, C4 (rules document and the CLAUDE.md row). C2, C3 are the run against main with the
private manifest (no committed test).

### Plan

- **G1** `specs/rules/design-rules.md` (new): when, the check, the five classes and tolerances,
  what does not count, the dispositions and how a row closes; `CLAUDE.md` rules-table row (prose
  commit at the end).
- Private manifest `netray-ops/specs/netray/design/canvas/manifest.json` (five binding result
  boards), committed in netray-ops; `just design-check` on the branch, whose pages equal main's.
