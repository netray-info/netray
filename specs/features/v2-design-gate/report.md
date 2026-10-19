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
