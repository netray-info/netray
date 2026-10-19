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
