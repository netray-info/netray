## c3bc3980..d53c0137

### Reader

COUNTS blockers=1 majors=0 minors=2
LENSES Engineering, Security, Testing
BLOCKER | tests/acceptance/design/compare.mjs:52 | When a multi-token spacing value mixes px and a keyword, the comparator compares the whole string instead of each token, so a gap inside the 4 px tolerance is reported as an open deviation and the check exits 1. | A flex container with only `row-gap` set: the board reads `gap` as `16px normal` and the page as `18px normal` (Chromium 156 confirmed this shape, e.g. `row-gap:2px` reads back as `gap: "2px normal"`). `pxWithin` passes 16/18 inside 4 px, then hits `normal`, which is not px, and returns `'16px normal' === '18px normal'`, which is false. `deviations()` emits `gap board 16px normal page 18px normal (spacing)`. I ran the module: `2px normal` against `4px normal` also gives one row. R5 and the plan ("4 px component-wise; non-px values exact") both say this pair matches. No test row has a keyword token.
MINOR | specs/features/v2-design-gate/report.md:63 | C4 (R7 exit status 0 / 1 / stale→1) is marked green on a test that only runs the missing-export exit-2 path, so nothing checks that the runner's exit code follows `summary.status`. | Change `process.exit(result.status)` at tests/acceptance/design/check.mjs:102 to `process.exit(0)`. `test_design_export.sh` still passes because it exits at check.mjs:35, and `test_design_compare.sh` only imports compare.mjs. A run with 58 open deviations would then exit 0. An end-to-end test was writable: board.spec.ts already builds a fixture export.
MINOR | tests/repo/test_design_states.sh:16 | The test creates a temp dir and never removes it: the only EXIT trap is netray.sh's `_netray_cleanup`. Every `just test-repo` / `just adlc-verify` run leaves a directory behind. | Each run leaves `$TMPDIR/tmp.XXXX` holding `history.db`, `snapshots.db`, `lens.log`, `lens.pid` and `custom_cas/` from `design_stack_up`. The sibling tests and the `design-check` recipe all `rm -rf "$tmp"` in their trap.

```quote tests/acceptance/design/compare.mjs:52
    if (!bm || !pm) return String(b) === String(p);
```

```quote specs/features/v2-design-gate/report.md:63
| C4 | R7 Exit status: 0 within tolerance or accepted with every board in a state; 1 otherwise with the count per board; 2 on a manifest or setup error; a stale `accepted` entry fails with 1 | green | tests/repo/test_design_export.sh |
```

```quote tests/repo/test_design_states.sh:16
tmp=$(mktemp -d) || fail "mktemp"
```

### Spec reader

COUNTS blockers=0 majors=2 minors=2
REQUIREMENTS met=7 partial=2 missed=0 contradicted=0
LENSES requirements (R1-R9, phases 1-3)
MAJOR | tests/acceptance/design/check.mjs:72 | A board with no state skips `measureBoard`, so its iframe is never screenshotted and no `<board>-board.png` is written. | R3 says each manifest board is screenshotted. Case: a board on `/unseeded.example.com` renders `data-state=none`, hits `continue` at line 78, and `target/design/` gets only `<slug>-page.png`.
MAJOR | tests/acceptance/design/check.mjs:72 | The same `continue` skips `sideBySide`, so a board with no state gets no `<board>-sbs.png` and is left out of the report's Side-by-side list. design-rules.md:27 writes this skip down as intended ("gets only its page image"). | R6 asks for one side-by-side image per board. Case: the same unseeded board yields a `no state` row and no sbs image.
MINOR | tests/acceptance/design/check.mjs:94 | The run also writes `target/design/page.json`, an output the spec never names. | R6 lists only `report.md`, `-board.png`, `-page.png` and `-sbs.png`.
MINOR | tests/acceptance/design/manifest.mjs:12 | The manifest takes an extra top-level `export` key, resolved against the manifest's directory with the default `export/canvas.html` (line 140). No requirement names this key. | R1's list of manifest contents has no export path. Case: `"export": "missing/canvas.html"` is accepted, and `test_design_export.sh` checks it.

```quote tests/acceptance/design/check.mjs:72
    if (page.status === null || page.status >= 400) {
      found.push(noStateDeviation(board, `HTTP ${page.status}`));
      continue;
    }
    if (page.state !== null && page.state !== 'fresh') {
      found.push(noStateDeviation(board, `data-state=${page.state}`));
      continue;
    }
```

```quote specs/features/v2-design-gate/spec.md:64
3. **Board side.** The check opens the export once, finds each manifest board's iframe by title,
   screenshots it at full content height into `target/design/<board>-board.png`, and reads the
```

```quote specs/features/v2-design-gate/spec.md:82
   character, so the rows paste into a feature's `report.md` criteria table unchanged; then a list of `accepted` entries that matched; then one side-by-side image per board,
   `target/design/<board>-sbs.png`, board left and page right at the same scale, for the visual
```

```quote specs/rules/design-rules.md:27
holds no fresh stored state, is a `no state` row and gets only its page image. Exit 0: everything
```

```quote tests/acceptance/design/check.mjs:94
  writeFileSync(join(outDir, 'page.json'), JSON.stringify(pages, null, 2) + '\n');
```

```quote tests/acceptance/design/manifest.mjs:12
const TOP = { required: [], optional: ['export', 'fixtures', 'boards', 'accepted'] };
```

```quote specs/features/v2-design-gate/spec.md:79
6. **Report.** The check writes `target/design/report.md` with one four-cell table row per
```

```quote specs/features/v2-design-gate/spec.md:49
1. **Manifest.** `just design-check` reads a JSON manifest (`$NETRAY_DESIGN_DIR/manifest.json`,
```

### Refutation

| finding | refuter | confidence | after |
|---|---|---|---|
| BLOCKER compare.mjs:52 keyword token in a spacing value | CONFIRMED | 9 | BLOCKER |
| MAJOR check.mjs:72 no board image for a no-state board | UNCERTAIN | 5 | MINOR |
| MAJOR check.mjs:72 no side-by-side for a no-state board | UNCERTAIN | 4 | MINOR |

### Refuted

None.

### Calibration

| finding | refuter answer | confidence | result | command |
|---|---|---|---|---|
| compare.mjs:52 | CONFIRMED | 9 | held | `node --input-type=module -e` on `deviations()` with gap `16px normal` against `18px normal`: 1 row |

### Roll call

| P | verdict | procedure | evidence |
|---|---|---|---|
| P03 | absence | sg for *Status/*Verdict enums, rg -i codenames in added lines | no enum, template, site or API serializer added; `lens` only as an internal host name in test tooling |
| P10 | absence | changed checks or scoring paths | none changed |
| P12 | absence | reqwest builders, engine module calls | none |
| P13 | absence | serve-command limiter setup | unchanged |
| P18 | absence | reqwest::Client constructions | no Rust changed |
| P26 | absence | new check IDs | none |
| P35 | convergence | a new rule arrives with its tests/repo check | design-rules.md with test_design_rules.sh and the design-check tests |
| P36 | absence | generated files in the diff | none |
| P40 | absence | module add or remove | none |

### Summary

Before refutation: reader 1/0/2, spec reader 0/2/2. After: reader 1/0/2, spec reader 0/0/4
(REQUIREMENTS met=7 partial=2 missed=0 contradicted=0); total 1 blocker, 0 majors, 6 minors.
verified 1, held 1. Roll call: 9 answered, 1 convergence, 0 divergence, 8 absence.

## c3bc3980..ad239fbc

### Reader

COUNTS blockers=1 majors=0 minors=0
LENSES Engineering, Security, Testing
BLOCKER | tests/acceptance/design/compare.mjs:57 | Two px values exactly 4 px apart (spacing) or exactly 1 px apart (typography) are reported as a deviation when they have fractional parts, because the raw float difference is compared to the tolerance with `>`. R5 and design-rules.md say "at most" 4 px or 1 px, and the spec's own boundary scenario (gap 36px against 40px: no row) passes only because integers subtract exactly. | `deviations()` with board `padding-left: 13.6px` (0.85rem) and page `17.6px` (1.1rem): 17.6-13.6 = 4.000000000000002, so it returns `B 1280/light: e padding-left board 13.6px page 17.6px (spacing)`. Board `line-height: 15.6px` (13px × 1.2) and page `16.6px` give 1.0000000000000018 and a typography row. A brute-force pass over one-decimal values found 30 such pairs between 10 and 100 px. Each spurious row makes `summary().status` 1, so `just design-check` exits 1 for a page that is within tolerance. Confirmed by importing compare.mjs in node.

```quote tests/acceptance/design/compare.mjs:57
    if (Math.abs(parseFloat(bm[1]) - parseFloat(pm[1])) > tol) return false;
```

### Spec reader

COUNTS blockers=0 majors=0 minors=3
REQUIREMENTS met=9 partial=0 missed=0 contradicted=0
LENSES requirements-conformance (R1-R9, Phases 1-3 including Review fixes), unrequested-behaviour
MINOR | tests/acceptance/design/compare.mjs:193 | report.md gets a "Stale accepted entries" section, which R6 does not list. R6 names three contents: the deviation rows, the matched accepted entries and the side-by-side images. R7 only says a stale entry is "printed". | R6/R7: a manifest with an `accepted` entry that matches no deviation writes `## Stale accepted entries` into target/design/report.md, as well as the stdout line `stale accepted: ...`.
MINOR | tests/acceptance/design/check.mjs:48 | Every run deletes earlier `*-board.png`, `*-page.png`, `*-sbs.png`, `page.json` and `report.md` from the output directory. The spec does not mention this side effect. | Unrequested: target/design/old-board-board.png, left by a run with another manifest, is gone after the next `just design-check`.
MINOR | tests/acceptance/design/manifest.mjs:86 | The loader also refuses a board with an empty `elements` list. That refusal is not among R1's listed ones (unknown key, missing title, duplicate element, bad class, slug collision, target/placeholder mismatch), and it means a board cannot be configured for the visual channel only. | R1: a board with `"elements": []`, meant only to get its side-by-side, exits 2 with `boards[0]: elements must be a non-empty array`.

```quote tests/acceptance/design/compare.mjs:193
    lines.push('', '## Stale accepted entries', '');
```

```quote specs/features/v2-design-gate/spec.md:82
   character, so the rows paste into a feature's `report.md` criteria table unchanged; then a list of `accepted` entries that matched; then one side-by-side image per board,
```

```quote specs/features/v2-design-gate/spec.md:88
   An `accepted` entry that no longer matches a deviation is printed as stale and fails with exit 1.
```

```quote tests/acceptance/design/check.mjs:48
    if (/-(board|page|sbs)\.png$|^page\.json$|^report\.md$/.test(f)) rmSync(join(outDir, f));
```

```quote tests/acceptance/design/manifest.mjs:86
  if (!Array.isArray(board.elements) || board.elements.length === 0) {
```

```quote specs/features/v2-design-gate/spec.md:56
   loader refuses an unknown key, a missing board title, a duplicate element name within a board,
```

### Refutation

| finding | refuter | confidence | after |
|---|---|---|---|
| BLOCKER compare.mjs:57 float difference at the tolerance boundary | CONFIRMED | 9 | BLOCKER |

### Refuted

None.

### Calibration

| finding | refuter answer | confidence | result | command |
|---|---|---|---|---|
| compare.mjs:57 | CONFIRMED | 9 | held | `node --input-type=module -e` on `deviations()` with padding-left `13.6px` against `17.6px`: 1 row |

### Summary

Second pass after the first pass's blocker was fixed (the earlier blocker no longer reported).
Reader 1/0/0, spec reader 0/0/3 (REQUIREMENTS met=9 partial=0 missed=0 contradicted=0), the same
after refutation. verified 1, held 1. Roll call not repeated: the range adds only comparator and
runner fixes to the one the first pass's roll call read.

## c3bc3980..d8974303

### Reader

COUNTS blockers=0 majors=1 minors=1
LENSES Engineering, Security, Testing
MAJOR | tests/acceptance/design/compare.mjs:44 | The four-side box expansion is meant for box shorthands, but it runs on every multi-token value of 4 tokens or fewer. A list of grid tracks with a different number of columns therefore expands to the same four tokens and produces no deviation. | Manifest element `{class: "spacing", properties: ["grid-template-columns"]}`. Board `300px 300px` against page `300px 300px 300px 300px` gives `deviations()` → `[]`. Board `600px 600px` against page `600px` also gives `[]`. I ran both cases with node against the committed `compare.mjs`. The loader accepts any property name, so a 2-column board passes against a 1-column or 4-column page.
MINOR | tests/acceptance/design/board.spec.ts:84 | The `sideBySide` test still passes when the page image is missing, so it cannot fail for the outcome it names ("both images next to each other at natural size"). | The side-by-side context has an 800 px viewport (board.mjs:71). The fixture boards are 390 px wide, so `390*2 = 780` is always ≤ the screenshot width of at least 800. The height check is still met by a single image. I copied board.mjs into the scratchpad with the page `<img>` removed and ran it: the output was 800x2000, the same as the original (800x2000), and both pass the test's assertions.

```quote tests/acceptance/design/compare.mjs:44
  if (bs.length !== ps.length && bs.length <= 4 && ps.length <= 4) {
```

```quote tests/acceptance/design/board.spec.ts:84
  expect(dims.width).toBeGreaterThanOrEqual(one.width * 2);
```

```quote tests/acceptance/design/board.mjs:71
    viewport: { width: 800, height: 600 },
```

### Spec reader

COUNTS blockers=0 majors=2 minors=2
REQUIREMENTS met=7 partial=2 missed=0 contradicted=0
LENSES requirements (R1–R9, phases 1–3)
MAJOR | tests/acceptance/design/compare.mjs:21 | Colours are normalised to hex only when Chromium returns them as `#hex`, `rgb()`/`rgba()` or `transparent`. Any other computed form (`color(srgb …)`, which is what `color-mix()` computes to, or `oklch(…)`/`lab(…)`) makes `normalizeColour` return null, and line 83 then compares the raw strings. | R5 (partial): board `color: color(srgb 0.0588 0.0706 0.0863)` against page `rgb(15, 18, 22)` is the same colour, so both normalise to `#0f1216ff` under the requirement. The code compares the raw strings instead and writes a deviation row.
MAJOR | specs/features/v2-design-gate/report.md:148 | The run on main does not report the h1 domain's size. It reports only the weight, because size matched at 40px. R9 requires "size and weight"; the phase 3 scenario's "size or weight" is looser than the requirement it tests. | R9 (partial): five result boards, desktop and mobile. h1 font-weight and line-height rows are written; no h1 font-size row on any board.
MINOR | tests/acceptance/design/compare.mjs:193 | `report.md` gets a `## Stale accepted entries` section that R6 does not list. R7 only asks for stale entries to be printed. | R6: a manifest with one stale `accepted` entry gives a report with four sections instead of the three R6 names.
MINOR | tests/acceptance/design/check.mjs:48 | Every run first deletes any `*-board.png`, `*-page.png`, `*-sbs.png`, `page.json` and `report.md` already in the out dir. No requirement or decision-log line names this side effect. | A file `target/design/foo-page.png` left by anything else is deleted when `just design-check` starts.

```quote tests/acceptance/design/compare.mjs:21
  if (!m) return null;
```
```quote tests/acceptance/design/compare.mjs:83
    return String(b) === String(p);
```
```quote specs/features/v2-design-gate/spec.md:75
   family) exact; `colour` (colour, background colour, border colour) exact after normalising to
```
```quote specs/features/v2-design-gate/spec.md:76
   lowercase hex with alpha. An empty value (a property the browser cannot read) never matches. A selector that matches nothing on either side is a class-1 row
```
```quote specs/features/v2-design-gate/report.md:148
- h1 domain font-weight board 650, page 700; line-height board 44px, page `normal` (size 40px matches)
```
```quote specs/features/v2-design-gate/spec.md:99
   deviations the 2026-10-19 inventory measured for those boards, at least: h1 domain size and
```
```quote specs/features/v2-design-gate/spec.md:100
   weight, the re-check button's height and background, and the missing score line.
```
```quote tests/acceptance/design/compare.mjs:193
  if (stale.length > 0) {
```
```quote specs/features/v2-design-gate/spec.md:82
   character, so the rows paste into a feature's `report.md` criteria table unchanged; then a list of `accepted` entries that matched; then one side-by-side image per board,
```
```quote tests/acceptance/design/check.mjs:48
    if (/-(board|page|sbs)\.png$|^page\.json$|^report\.md$/.test(f)) rmSync(join(outDir, f));
```

### Refutation

| finding | refuter | confidence | after |
|---|---|---|---|
| MAJOR compare.mjs:44 box expansion on grid track lists | REFUTED (R5's spacing list has no grid tracks) | 7 | restored MAJOR after my check (below) |
| MAJOR compare.mjs:21 non-rgb colour forms compared raw | REFUTED (gate.rs refuses those colour words in V2 CSS) | 6 | MINOR |
| MAJOR report.md:148 no h1 size row on main | REFUTED (the inventory measured size equal, 40 px on both sides) | 7 | dropped |

### Refuted

- report.md:148: the private inventory's row 9.10 records the h1 as "40 px / 650" against "40 px / 700"; no size deviation exists on main, and R9 asks for the deviations the inventory measured.

### Calibration

| finding | refuter answer | confidence | result | command |
|---|---|---|---|---|
| compare.mjs:44 | REFUTED | 7 | held | `deviations()` with spacing `grid-template-columns` `300px 300px` against `300px 300px 300px 300px`: no row; the loader accepts any property name, so a manifest can reach it |
| compare.mjs:21 | REFUTED | 6 | not held | `rg` for colour functions in `crates/web/static` and `crates/web/templates`: none; the gate refuses them |
| report.md:148 | REFUTED | 7 | not held | read netray-ops inventory row 9.10: size equal |

### Summary

Third pass. Before refutation: reader 0/1/1, spec reader 0/2/2 (REQUIREMENTS met=7 partial=2
missed=0 contradicted=0). After: reader 0/1/1 (box expansion restored at MAJOR), spec reader
0/0/3 (colour forms downgraded, h1 size dropped); total 0 blockers, 1 major, 4 minors. verified 1,
held 1. Roll call not repeated (first pass: 9 answered, 1 convergence, 0 divergence, 8 absence);
the range adds only comparator and runner fixes. The major (expansion should be limited to box
shorthands) is a follow-up for the first feature whose manifest names a non-box multi-value
property; this feature's manifest names none.
