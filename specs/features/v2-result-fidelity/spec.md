# Spec: v2-result-fidelity

Status: Ready
Created: 2026-10-19

## Goal

The result page `/<domain>`, its history trace and its phone layout look like the binding boards of
the design canvas (private): F (history, boards 13/14), FFirst (first visit, 15/16), FTrace (history
trace with time lens, 25/26) and M (mobile, 27/28), each light and dark, desktop at 1280 px and
phone at 390 px. Every deviation the canvas inventory of 2026-10-19 measured for these boards
(section 09, private) is rebuilt to the board, and the page gets back the numbers and the sharing
V1 had (inventory theme H): score, per-section grades, hard-fail banner, run duration and cached
marker, and a share-and-embed dialog with snapshot, re-run, badge and social-card snippets. The page
links to `/<domain>/history`, `/r/<id>` renders inside the site frame, and the spectrum label of
`tls.expiry_window` matches its row. `just design-check` (feature `v2-design-gate`) exits 0 for the
eight boards, every deviation fixed or accepted by an operator decision, and a reader of its
side-by-side images finds no missing, extra or misordered element.

## Non-goals

- The design gate itself, its manifest format and `specs/rules/design-rules.md`: `v2-design-gate`.
  Canvas row 9.1 (no reproducible state after the TTL) is closed by that gate's seeded, long-TTL
  stack, not here.
- The inspectors and board G (17/18): canvas rows 9.48–9.59 go to `v2-inspector-shell` (shell
  parts) and to the TLS depth feature (addresses, certificate, protocols, port chooser).
- Workflow tools on the result page (help modal, keyboard shortcuts, copy as Markdown; rows 1.11,
  1.20, 1.21, 7.64): `v2-inspector-shell` Phase 8, built on the shared component, after this
  feature.
- Score fields in `/api/v1` (the API half of row 1.4): a later API feature; this feature changes
  no API response and no contract golden.
- Site navigation beyond the boards' header (theme J: Email and Tools in the header, the full
  footer, grade legend on `/`, `.html` and trailing-slash redirects): the site-navigation feature.
- V1 snapshot links `/r/<shortid>` keep answering 410 (row 1.18 stays dropped, operator
  2026-10-19, spec 13j).
- The OG card and badge layouts: unchanged.
- MCP tools (row 1.35, dropped for now) and DKIM selectors (theme I).

## Context and constraints

- Boards (private, `netray-ops/specs/netray/design/canvas/`): sources `F.dc.html` (props `dark`,
  `first`, `trace`), `M.dc.html`, imports `FDark`, `FFirst`, `FFirstDark`, `FTrace`, `FTraceDark`,
  `MDark`; PNGs `png/13`–`16`, `25`–`28`. Canvas version v41 (2026-10-19) added the theme toggle in
  the header and the footer (tagline, Privacy, Status) to F, G and M, closing rows 9.4 (toggle) and
  9.40 as "adjust canvas", and set the Tokens board's pass hairline to F's #3a9a66 (row 9.25,
  operator decision).
- Inventory (private, `netray-ops/specs/netray/inventory/`): `sections/09-canvas.md` rows 9.2–9.47
  and 9.60–9.67 with evidence under `evidence/canvas/` (`measure-result.json`, `sbs14.png`,
  `sbs25.png`, `sbs27a.png`, `sbs27b.png`, `a11y-and-probes.txt`); `sections/01-lens.md` rows 1.4,
  1.5, 1.7, 1.13, 1.25, 1.26 and its findings (no link to `/<domain>/history`, `/r/<id>` without
  frame, "Renewal" label). Operator decisions of 2026-10-19 (`v2-parity.md`): theme H rebuilt, 1.18
  dropped; every canvas row not named otherwise is an error, rebuilt to the board.
- SDD `specs/sdd/v2.md`: §5 names "F.dc.html with trace, FFirst.dc.html, M.dc.html" as the visual
  reference; §5.4–§5.6 (spectrum, labels, trace and replay, phone stepper), §5.7 (page order), §5.8
  (open issues), §6.1 (evidence), §8.2 (tokens, phone values under `layout.phoneMax` 480 px), §8.3
  (one macro and one stylesheet per component), §8.4 (islands; bundle capped at 30 KB gzipped, a
  test enforces it), R43 (the marker is the only active state), R5 (views compute no verdict).
- Code on main (c3bc3980): `crates/web/templates/pages/result.html` and the partials
  `result_header.html`, `result_grade.html`, `result_run_line.html`, `result_share.html`,
  `result_spectrum.html`, `result_diff.html`, `result_issues.html`, `result_evidence.html`,
  `result_passed.html`; components under `templates/components/` with their stylesheets in
  `crates/web/static/`; views in `crates/web/src/pages/result/views.rs` (`issues`,
  `passed_checks`, `diff_view`, `issue_since`, `trace_view`, `history_sentences`);
  `templates/base.html` (header nav of seven items, boxed Theme button, footer);
  `templates/pages/entry.html` overrides `header` with an empty block and inlines its styles, which
  is why `/r/<id>` renders without the frame.
- The stream island replaces elements by ID (`#run-line`, `#spectrum`, `#issues`, the grade, the
  diff): every ID a fragment targets stays on the rebuilt markup, or the stream breaks.
- Scoring: `netray_engine::history::score_run(&Run, &Profile)` returns `Score { sections (each with
  percentage), overall_percentage, grade, complete, hard_fail_checks, not_applicable }`
  (`crates/engine/src/history.rs:18`, `crates/engine/src/scoring.rs:23`). The stored `Entry`
  (`crates/store/src/lib.rs:104`) has no run duration.
- Catalogue `tls.expiry_window` has title "Expiry window" and short "Renewal"
  (`crates/engine/catalogue/checks.toml:231-241`); the catalogue carries `source` (e.g. "RFC 6797")
  per check.
- Gate rules (CLAUDE.md): identifier-valued CSS properties go into `IDENTIFIER_PROPERTIES`; page
  tests assert a component by its markup; no `Router` word in `crates/web/src` outside `entries/`,
  `pages/`, `api/`; a golden over live-run markup normalises the wall clock; islands write no
  style. `specs/rules/frontend-rules.md` and, once merged, `specs/rules/design-rules.md` apply.
- Test data: `example.com`, its subdomains and `netray.info` only.
- `just design-check` needs the private canvas (`NETRAY_DESIGN_DIR`) and a browser; it runs outside
  `just adlc-verify`. Its rows paste into `report.md` with status `open` and keep `adlc next` on
  implement until fixed or accepted.

## Requirements

Each requirement names the inventory rows (`9.n` canvas, `1.n` lens) or boards it closes. "Board
value" means the value the gate measures on the named boards; the inventory row holds the number.

### Frame and header

1. **Container.** At 1280 px the page content starts at x 112 and is 1056 px wide (max-width
   1120 px including 32 px side padding), for header, main and footer alike. (9.2; boards 13–16,
   25/26)
2. **Header.** The header holds, in one row, the wordmark, the tagline "your domain, in full
   spectrum" (13 px, muted, from its own partial; the footer keeps the site tagline of
   `partials/tagline.html`) beside it, and right-aligned the nav
   Inspectors, Guides, API, Status (13 px, muted) followed by the unboxed text button "Theme". The
   Inspectors link goes to `/inspect/dns`, API to `/api/v1/docs`. Compare leaves the header and is
   linked from the footer beside Privacy and Status until the site-navigation feature redoes the
   footer. Every link is apex-relative. (9.3, 9.4; boards 13–16, 25/26, 27/28)
3. **Result header layout.** One row: the grade block on the left (letter, then the score line
   under it), the run title above the h1 domain in the middle, and "Re-check now" and "Share" right
   aligned; the block is at most 100 px tall at 1280 px. (9.5; boards 13–16, 25/26)
4. **Grade.** The letter is 88 px, weight 700, letter-spacing −0.04em, line-height 0.85; its bar is
   4 px wide in the `pass` status colour of the theme (light #1e7a4c, dark #4fd18f) for a passing
   grade, in the warn or fail colour otherwise, as the board's palette maps grades. (9.7, 9.8;
   boards 13–16)
5. **Score line.** Under the letter, mono 13 px: "score <n> / 100", `<n>` the run's overall
   percentage from `score_run` under the entry's profile version, rounded half up, with `<n>` in
   ink at weight 700 (the board's `<strong>`). Under it, mono 11 px muted, the grade's descriptor and meaning as V1's grade
   legend words them (`crates/lens/frontend/src/lib/gradeLegend.ts`: A+ "excellent, ahead of most
   domains" through F "critical, fix today"), held once in `netray-web` so the grade legend of the
   site-navigation feature reads the same texts. The line
   is absent for an incomplete run and when `score_run`'s grade differs from the stored grade (the
   page never shows a number that contradicts the letter; such a mismatch logs a warning naming the
   entry). (9.6, 1.4; boards 13–16, 27/28)
6. **Run title.** Above the h1, mono 11 px uppercase with letter-spacing .12em: "RUN <k> ·
   <YYYY-MM-DD HH:MM> UTC · <c> CHECKS · PROFILE <v>", where `<k>` is the entry's ordinal among the
   domain's stored entries (oldest stored entry 1; the retention sweep renumbers), `<c>` the run's check count and `<v>` its profile version;
   then "· <s> S" with the creating run's duration in seconds to one decimal when it is known
   (requirement 9), then "· CACHED" when requirement 10 holds, then "· FIRST RUN" on a domain's
   first entry. The entry id stays reachable through Share (requirement 33), not in the title. A
   live run in progress keeps v2-result-page's run line in this place and style. (9.9, 9.43, 1.25;
   boards 13–16, 25/26)
7. **h1.** The domain is 40 px, weight 650. (9.10; boards 13–16, 27/28)
8. **Buttons.** "Re-check now" is the primary button: 44 px tall, padding 0 18 px, ink background,
   `bg` text, 14 px weight 600, radius 2 px, in both themes. "Share" is the secondary button beside
   it: 44 px tall, 1 px `line` border, radius 2 px, transparent background, ink text. Re-check is a
   `<button>`; Share is the link to `/r/<id>` that requirement 41 upgrades to the dialog trigger;
   both are styled by one stylesheet and no user-agent button style remains.
   (9.11, 9.12, 9.13; boards 13–16)
9. **Run duration.** The history store records each new entry's run duration in milliseconds (the
   engine run from start to grade) in a new nullable column; an `Identical` run leaves it unchanged;
   entries recorded before carry none and show no duration. The stored `Run`, its fingerprint and
   its diff are unchanged. (1.25)
10. **Cached marker.** "CACHED" is shown when the page renders a stored entry inside the cache
    window without starting a run (`data-state="fresh"`), and when `/<domain>/run` answered the
    stream from a cached entry instead of running. (1.25)
11. **Hard-fail banner.** Between the result header and the spectrum, when the run has failed
    hard-fail checks: a block with a 3 px fail-tone bar reading "Hard fail: <check title>[, <check
    title>]. A failed hard-fail check sets the grade to F whatever the score." When sections were
    left out of the score, a muted line "Score computed without: <Protocol> (<reason>)[, …]"
    follows, the reasons from `score_run`'s `not_applicable` and the section outcomes. Without a
    hard fail and without a left-out section, nothing renders. (1.7)
12. **Heading order.** The page has one h1 (the domain) and an h2 per section: "Since run <k−1>"
    (or the first-run box heading), "<n> lines need attention", "Evidence"; group cards and items
    use h3 and below. No heading level is skipped. (9.66; boards 13–16, 27/28)
13. **Entry page frame.** `/r/<id>` renders inside the site frame: the header of requirement 2,
    the container of requirement 1 and the footer, with the shared stylesheet, keeping its own
    title and OG meta. (01-lens finding "`/r/<id>` without the site frame")

### Spectrum

14. **Line heights.** A measured line's height is `plate × (0.35 + 0.65 × w / w_max)`, `w` its
    effective weight (§5.4) and `w_max` the largest effective weight in the run, so the heaviest
    line fills the 92 px plate and the lightest is at least 35 % of it; order by weight is kept.
    Unmeasured and not-applicable lines keep their muted mark. SDD §5.4 is amended in the same
    commit. (9.15; boards 13–16, 25–28)
15. **Plate and axis.** The plate has radius 2 px. Under it, a 32 px axis with one tick per line
    boundary (N + 1 ticks for N lines): 4 px, and 10 px at the plate's ends and at region bounds.
    (9.18, 9.20; boards 13–16, 25–28)
16. **Region labels.** Mono 11 px uppercase, letter-spacing .14em, centred under each region; at
    phone width 9 px and the email region labelled "MAIL". Each region label carries the
    section's letter grade after the name ("DNS B"), computed from the section percentage of
    `score_run` with the profile's thresholds; a section without a score shows its name only.
    (9.19, 1.5; boards 13–16, 25–28)
17. **Line labels and label band.** Labels are mono 11 px weight 600 on a transparent background
    (no plate behind the text); the label band is 56 px for up to three rows, each row's leader one of three lengths
    (8, 22, 36 px), growing only when a fourth row is needed (§5.5). (9.16, 9.17; boards 13–16)
18. **Legend and key note.** The legend shows each status with its mark: pass, warn, fail as 2, 3
    and 4 px lines in their colours, "fixed since run <k−1>" as the hollow fix line with its ring,
    mono 12 px; the "muted" entry goes. Right of the legend, mono 12 px muted: "<N> checks · line
    height = weight in the grade". (9.14, 9.21; boards 13–16, 25–28)
19. **Expiry label.** The catalogue short of `tls.expiry_window` is "Expiry", so the spectrum label
    names the same check as its row title "Expiry window". (01-lens finding "Renewal")

### History trace, diff and first visit

20. **Trace form.** Under the spectrum each line carries its trace: one row per stored entry,
    newest on top, up to 24 entries (§5.6), 5 px per row (6 px at phone width); pass a hairline in
    #3a9a66 light (the dark board's pass hairline in dark), warn and fail a 3 px block in their
    colour, not tested and not applicable the muted mark. Right of the trace, mono 11 px muted, the
    newest and oldest dates ("10-08", "07-12") at its top and bottom edges. The horizontal
    waterfall rows go. (9.44; boards 25/26, 27/28; board F accepted per the Decision log)
21. **Caption and hint.** Under the trace, mono 11 px: left "Each line carries its trace: <n> runs,
    newest on top." in muted, right "hover a run to replay the spectrum · tap to pin"; while a run
    is hovered or pinned the caption names the line and its history sentence in ink and the hint
    reads "replaying run <k> · move away to return" or "pinned · tap again to release". (9.46;
    boards 25/26)
22. **Replay gutter.** Right of the plate, a column with "Run <k>", the date in bold and "grade
    <letter>", and a 3 px marker beside the trace row being replayed; the run title reads "Run <k> ·
    <date> · replaying history" while replaying. While a run is replayed, the region grades and the
    legend's fixed entry follow the replayed run (Phase 2 review). (9.45; boards 25/26)
23. **Hover dimming.** While a run is hovered, every other trace row is at 30 % opacity and the
    hovered row is enlarged; the island switches data attributes only and the stylesheet does the
    rest. (9.47; boards 25/26)
24. **History link.** Under the trace a link "Show history as a table" to `/<domain>/history`
    (boards 27/28; 9.22, 1.26, 01-lens finding "no link to `/<domain>/history`").
25. **Since heading.** An h2 "Since run <k−1>" 20 px weight 650 outside the box, followed by mono
    meta "<predecessor date> · grade <from> → <to>" (with the rescored step when present). (9.26;
    boards 13/14, 25/26)
26. **Diff rows.** Inside a bordered box: each row padding 10 px 16 px with a rule between rows, a
    3 × 16 px tone marker, a kind column 70 px wide (text starts at x 234 at 1280 px), the
    sentence, and right-aligned the line code (requirement 29) or the protocol for an evidence
    change. At phone width the sentence comes first and "<kind> · <code>" is a meta line under it.
    (9.27, 9.65; boards 13/14, 25–28)
27. **First-run box.** On a domain's first entry the since block is replaced by a bordered surface
    box: "First run recorded." 17 px weight 650, the explanation sentence of board FFirst, and a
    ghost trace of one row and two dashed rows. (9.42; boards 15/16)

### Open issues and passed checks

28. **Attention heading and switch.** An h2 "<n> lines need attention" with mono meta "<f> fail ·
    <w> warn" beside it; right-aligned on the same line the grouping switch as unboxed tabs, mono
    12 px, 36 px tall, the marker bar on the current tab and a rule below. (9.28, 9.29; boards
    13–16, 25/26)
29. **Line codes.** A check's code is its protocol tag and its 1-based position in the spectrum's
    line order, two digits: "DNS·04", "TLS·12", "HTTP·21", "MAIL·31", "IP·38" (tags DNS, TLS, HTTP,
    MAIL, IP). The same code is used in items, diff rows and the evidence feeds line.
    (9.32; boards 13–16, 25–28)
30. **Group cards.** Each group is a bordered card with a header row: the owner label 14 px weight
    650 and right-aligned "<n> lines" (or "1 line") in mono. (9.30; boards 13–16)
31. **Item layout.** Each item is a two-column grid: an 84 px left column with a 34 px tone bar,
    the severity word (mono 11 px weight 700 uppercase in the tone colour) and the code (mono 11 px
    muted); the right column holds the title with its since text, the body (14 px muted), the fix
    ("Fix:" then the hint, 14 px ink), and a meta line "<Protocol> · <catalogue source>" followed
    by the links "evidence →" and "guide →" (mono 12 px muted, underlined). At phone width the item
    is one column with the meta line "<SEV> · <code> · <since>" above the title. (9.31, 9.32, 9.33,
    9.34, 9.35; boards 13–16, 27/28)
32. **Since text.** Mono 11 px muted, from the domain's stored entries: "first run" on a first
    entry; "in all <n> runs" when the check was warn or fail in every entry; "lit up in run <k>"
    when it turned warn or fail at entry `k` and stayed; "down from fail since run <k>" when it
    went from fail to warn at entry `k`; "up from warn since run <k>" for the reverse. One pure
    function computes it, table-tested. (9.36, 9.43; boards 13–16)
33. **Clean bar.** Passed and not-applicable checks sit behind a bordered 48 px bar "<n> lines
    clean · checks that pass" with the count in the pass colour and "show" right-aligned, built on
    `<details>` so it works without JavaScript; the coverage line ("dns measured · tls measured …")
    goes, its links move into the evidence tabs (requirement 35). (9.38, 9.39; boards 13–16, 27/28)
34. **Section rhythm.** 36 px between consecutive sections at 1280 px. (9.41; boards 13–16)

### Evidence

35. **Evidence section.** Between the open issues and the clean bar: an h2 "Evidence" with mono
    meta "recorded with this run", a bordered card with tabs DNS, TLS, HTTP, EMAIL, IP (mono 12 px
    uppercase, the marker on the current tab), and per tab a panel holding the protocol's evidence
    blocks through v2-inspectors' section renderer, a line "Feeds lines: <code> <short>[ · …]",
    a link "All evidence →" to `/r/<id>/<protocol>` and the view's explore-live link ("Open in the
    TLS inspector →"). A section that is unmeasured or not applicable shows its reason in its panel.
    The TLS tab is current by default when TLS has a non-pass line, else the first protocol with
    one, else DNS. Without JavaScript every panel renders in order with the tabs as in-page links;
    with it a small island switches panels by data attribute. An incomplete live run fills the same
    tabs from the stream (§6.1). At phone width a too-wide table scrolls inside the panel with the
    note "← swipe the table". (9.37; boards 13–16, 27/28)

### Phone layout (390 px)

36. **Phone header.** One row: the wordmark at 58 × 33 px, the "Theme" button and a 44 × 44 px menu
    button (1 px `line` border, radius 2 px) that discloses the nav (built on `<details>`, so it
    works without JavaScript); every nav entry in the open menu is at least 44 px tall. (9.60, 9.61;
    boards 27/28)
37. **Phone result header.** The run title on top (without the profile part), then the grade
    letter at 64 px beside the h1, then "Re-check now" full width (primary, 44 px) with "Share"
    beside it. (9.62; boards 27/28)
38. **Phone spectrum.** The spectrum fits the 358 px column without sideways scroll: line labels
    10 px, region labels 9 px; the `min-width` that forced 432 px goes. (9.63; boards 27/28)
39. **Phone trace and stepper.** The per-line trace (requirement 20, 6 px rows) with a stepper bar
    under it: "‹" and "›" buttons of 44 px at the edges and between them "Run <k> · <date> · grade
    <letter>" over "latest run" (or "replaying history · › to return" while stepped back, as board M); the hint "Tap a line for its history. Step
    back with ‹ to replay a run."; then requirement 24's link. (9.64; boards 27/28)
40. **Tap targets.** At 390 px every link and button on the page is at least 44 px tall, footer
    links and the clean bar included. (9.67; boards 27/28)

### Share and embed

41. **Share dialog.** "Share" opens a `<dialog>` titled "Share and embed" with four tabs, each a
    read-only field and a copy button: Snapshot (the entry URL `<origin>/r/<id>`), Re-run link
    (`<origin>/<domain>`), Badge (Markdown and HTML snippets of `<origin>/badge/<domain>.svg`
    linking to `<origin>/<domain>`, with a style choice flat or for-the-badge that rewrites the
    snippets, and a preview), Social card (the URL `<origin>/r/<id>/og.png` with a preview image).
    `<origin>` is the one origin the page's canonical link uses. Esc and the close button close it;
    focus returns to "Share". Without JavaScript "Share" is a link to `/r/<id>`, as today. The
    dialog uses tokens and components only; the islands bundle stays under its cap. Share is absent
    without a stored entry and for an incomplete run (v2-result-page requirement 5). (1.13)

### Acceptance

42. **Design gate.** The private manifest names the elements of requirements 1–40 on the eight
    boards as the export titles them: "D3 · Analyzer — history (option 1+2), light/dark" and
    "D3 · Analyzer — first visit, light/dark" (1280 px), "D3 · History: trace with time lens —
    light/dark (Play: hover a run)" (1280 px) and "D3 · Mobile result — light (Play: ‹ › and tap
    lines)" and "D3 · Mobile result — dark" (390 px), each phase adding its own elements. Board F's
    waterfall rows are measured as an element "history rows" (board: a waterfall row; page: a trace
    row) whose value rows are matched by `accepted` entries carrying the operator's decision; every
    other row is fixed. The header theme toggle and the footer are built (boards' v41 sources) but
    enter the manifest only once the operator's fresh export carries them: until then they would
    measure as `missing on board`. `just design-check` exits 0 with no open and no stale accepted
    row; its deviation rows go into `report.md` as they appear and are closed there. (09-canvas rows
    9.2–9.47, 9.60–9.67)
43. **Visual reading.** A reader with no part in the build compares every side-by-side image the
    gate writes and finds no missing, extra or misordered element besides the theme-H additions this
    spec places (score descriptor, section grades, hard-fail banner, duration and cached marker,
    share dialog, history link on desktop); each finding becomes a `report.md` row and is closed.

## Phases

## Phase 1 — Frame, header and hero

**Depends on:** `v2-design-gate` merged
**Requirements:** 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13

`base.html` header and container, the result header partials, the grade and run-line components,
the button styles, `score_run` in the result views with the descriptor texts, the store column and
its migration, the hard-fail banner, and `entry.html` on the frame. The manifest gains the header,
grade, score, run title, h1 and button elements of all eight boards.

### Test Scenarios

GIVEN a stored entry graded B with overall percentage 81.4 under profile 3 WHEN `/example.com` renders THEN the grade block holds "score 81 / 100" and the descriptor of B.
GIVEN an incomplete live run WHEN the header renders THEN there is no score line.
GIVEN a stored run whose `score_run` grade differs from its stored grade WHEN the page renders THEN there is no score line and a warning names the entry.
GIVEN the domain's twelfth entry with 40 checks under profile 3, recorded in 4210 ms WHEN rendered THEN the run title reads "RUN 12 · <date> UTC · 40 CHECKS · PROFILE 3 · 4.2 S".
GIVEN an entry recorded before the duration column WHEN rendered THEN the run title has no duration part.
GIVEN a fresh entry inside the cache window WHEN `/example.com` is fetched THEN the run title ends with "CACHED" and no run starts.
GIVEN a domain's first entry WHEN rendered THEN the run title ends with "FIRST RUN".
GIVEN an `Identical` run of an existing entry WHEN recorded THEN the entry's duration is unchanged.
GIVEN a run with `tls.hostname_match` failed WHEN rendered THEN the hard-fail banner names "Hostname match" and the grade F.
GIVEN a run with email not applicable WHEN rendered THEN the banner's muted line reads "Score computed without: Email (<reason>)".
GIVEN a run with neither hard fail nor left-out section WHEN rendered THEN no banner element exists.
GIVEN the result page WHEN its headings are listed THEN it has one h1 and the section headings are h2 with no skipped level.
GIVEN `/r/<id>` of a stored entry WHEN rendered THEN it carries the site header, the container class and the footer.
GIVEN the header WHEN rendered THEN its nav holds Inspectors, Guides, API, Status and the Theme button, in that order, and no absolute URL.
GIVEN the result page at 1280 px light and dark WHEN `just design-check` runs THEN no row names the header, grade, score, run title, h1, Re-check or Share elements.

## Phase 2 — Spectrum figure

**Depends on:** Phase 1
**Requirements:** 14, 15, 16, 17, 18, 19

`spectrum::layout` heights, the spectrum component's axis, ticks, region labels with section
grades, line labels and legend, the catalogue short. SDD §5.4 amended in the same commit.

### Test Scenarios

GIVEN effective weights 10, 5 and 2 WHEN laid out on a 92 px plate THEN the heights are 92, 62.1 and 44.2 px within rounding.
GIVEN a run with one unmeasured section WHEN laid out THEN that section's lines keep the muted mark and do not change `w_max`.
GIVEN 40 lines in five regions WHEN rendered THEN the axis holds 41 ticks, the ends and the four region bounds 10 px and the others 4 px.
GIVEN a DNS section at 84 percent under profile 3's thresholds WHEN rendered THEN its region label reads "DNS" followed by that letter.
GIVEN a not-applicable email section WHEN rendered THEN its region label reads "EMAIL" with no letter.
GIVEN the legend WHEN rendered THEN it has pass, warn, fail and fixed entries each with its mark element and no "muted" entry, and the key note reads "<N> checks · line height = weight in the grade".
GIVEN the catalogue WHEN `tls.expiry_window` is read THEN its short is "Expiry".
GIVEN boards F and M WHEN `just design-check` runs THEN no row names the plate, axis, ticks, region labels, line labels, label band or legend.

## Phase 3 — History trace, diff and first visit

**Depends on:** Phase 2
**Requirements:** 20, 21, 22, 23, 24, 25, 26, 27

The trace component in the per-line form, the spectrum island's caption, hint, gutter and dimming
states, the since heading and diff rows, the first-run box.

### Test Scenarios

GIVEN 12 stored entries WHEN the trace renders THEN each line has 12 rows, newest first, and the edge dates are the newest and oldest entry's month and day.
GIVEN 30 stored entries WHEN the trace renders THEN each line has 24 rows.
GIVEN a pass row and a warn row WHEN rendered THEN the pass row is a 1 px hairline and the warn row a 3 px block in the warn colour.
GIVEN the trace at rest WHEN rendered THEN the caption reads "Each line carries its trace: 12 runs, newest on top." and the hint "hover a run to replay the spectrum · tap to pin".
GIVEN the island WHEN run 5 is hovered THEN every other trace row carries the dimmed attribute, the gutter shows "Run 5", its date and grade, and the run title reads "Run 5 · <date> · replaying history".
GIVEN a pinned run WHEN the hint renders THEN it reads "pinned · tap again to release".
GIVEN the result page of a domain with history WHEN rendered THEN it links "Show history as a table" to `/example.com/history`.
GIVEN entry 12 with predecessor 11 graded C and itself B WHEN rendered THEN an h2 reads "Since run 11" and its meta "<date> · grade C → B".
GIVEN a diff with a fixed check at position 31 WHEN rendered THEN its row shows kind "fixed", the sentence and the code "MAIL·31".
GIVEN a domain's first entry WHEN rendered THEN the box reads "First run recorded." with the explanation and a ghost trace of one row and two dashed rows, and no since heading.
GIVEN boards FTrace and FFirst WHEN `just design-check` runs THEN no row names the trace, caption, gutter, since heading, diff rows or first-run box.

## Phase 4 — Open issues and passed checks

**Depends on:** Phase 1
**Requirements:** 28, 29, 30, 31, 32, 33, 34

The issues partial, the grouping switch, `check_row` as the two-column item (the same component
the evidence views and inspectors use), the line-code and since functions, the clean bar.

### Test Scenarios

GIVEN a run with one fail and four warns WHEN rendered THEN an h2 reads "5 lines need attention" and its meta "1 fail · 4 warn".
GIVEN `http.hsts` at spectrum position 21 WHEN its code is built THEN it is "HTTP·21"; GIVEN an email check at position 31 THEN "MAIL·31".
GIVEN two items owned by the web server WHEN grouped by owner THEN one card's header reads the owner label and "2 lines".
GIVEN an item of `http.hsts` WHEN rendered THEN it has the left column with severity "FAIL" and code, the title, the body, "Fix:" with the hint, the meta "HTTP · RFC 6797" and the links "evidence →" and "guide →".
GIVEN statuses pass, pass, warn, warn over entries 1 to 4 WHEN the since text is built THEN it reads "lit up in run 3".
GIVEN statuses warn in all 12 entries WHEN built THEN "in all 12 runs"; GIVEN fail, fail, warn THEN "down from fail since run 3"; GIVEN one entry THEN "first run".
GIVEN 35 passed checks WHEN rendered THEN the clean bar reads "35 lines clean · checks that pass" and "show", and opening it without JavaScript lists the 35 checks.
GIVEN the result page WHEN rendered THEN no coverage line element exists.
GIVEN board F WHEN `just design-check` runs THEN no row names the attention heading, switch, group card, item parts, clean bar or the section gaps.

## Phase 5 — Evidence section

**Depends on:** Phase 4
**Requirements:** 35

The evidence partial on the result page with tabs, panels, feeds line, links, and a tabs island
(or an extension of an existing island) within the bundle cap.

### Test Scenarios

GIVEN a stored entry WHEN rendered without JavaScript THEN the Evidence h2 is followed by five tab links and five panels in protocol order, each with its blocks, a feeds line, "All evidence →" to `/r/<id>/<protocol>` and its explore-live link.
GIVEN a run whose only non-pass lines are TLS WHEN rendered THEN the TLS tab carries the marker and its panel is shown.
GIVEN an IP section not applicable WHEN its panel renders THEN it shows the reason.
GIVEN the tabs island WHEN the HTTP tab is activated THEN only the HTTP panel carries the shown attribute and the island wrote no style.
GIVEN an incomplete live run WHEN its sections stream THEN the panels fill from the fragments.
GIVEN boards F and M WHEN `just design-check` runs THEN no row names the evidence heading, tabs, panel table, feeds line or inspector link.

## Phase 6 — Phone layout

**Depends on:** Phase 3, Phase 4, Phase 5
**Requirements:** 36, 37, 38, 39, 40, and the phone halves of 16, 26 and 31

Phone values under the `layout.phoneMax` query, the menu disclosure, the phone result header, the
spectrum fit, the stepper bar and hint, tap targets.

### Test Scenarios

GIVEN 390 px WHEN the header renders THEN wordmark, Theme and the 44 × 44 menu button share one row and the nav is hidden until the menu opens.
GIVEN 390 px WHEN the page renders THEN the document is not wider than the viewport and the spectrum's region labels include "MAIL".
GIVEN 390 px with 12 entries WHEN rendered THEN the stepper reads "Run 12 · <date> · grade B" over "latest run" and the hint "Tap a line for its history. Step back with ‹ to replay a run.".
GIVEN 390 px light WHEN every link and button is measured THEN none is below 44 px tall.
GIVEN boards M and MDark WHEN `just design-check` runs THEN it reports no row.

## Phase 7 — Share and embed

**Depends on:** Phase 1
**Requirements:** 41

The share dialog markup in the result header, a share island (open, close, tab switch, copy,
style choice), snippet texts built on the server.

### Test Scenarios

GIVEN entry `Ab3dE5fG` of example.com WHEN the dialog renders THEN the Snapshot field holds `<origin>/r/Ab3dE5fG` and the Re-run field `<origin>/example.com`.
GIVEN the Badge tab with style for-the-badge WHEN chosen THEN both snippets carry `style=for-the-badge`.
GIVEN the Social card tab WHEN rendered THEN it holds `<origin>/r/Ab3dE5fG/og.png` and a preview image of it.
GIVEN the share island WHEN a copy button is pressed THEN the clipboard receives the field's text and the button announces it.
GIVEN the open dialog WHEN Esc is pressed THEN it closes and focus returns to Share.
GIVEN no JavaScript WHEN Share is followed THEN it leads to `/r/Ab3dE5fG`.
GIVEN an incomplete run WHEN the header renders THEN there is no Share control.
GIVEN the islands bundle WHEN built THEN the cap test passes.

## Phase 8 — Design gate and visual reading

**Depends on:** Phases 1–7
**Requirements:** 42, 43

A full `just design-check` over the eight boards on the local stack, the reader's pass over the
side-by-sides, and the operator's accepted entries in the manifest; results under `###
Behavioural verification` in `report.md`.

### Test Scenarios

GIVEN the manifest's eight boards and the seeded fixtures WHEN `just design-check` runs THEN it exits 0, prints "0 open deviation(s), 0 stale accepted entries" and every board has a side-by-side image.
GIVEN board F light and dark WHEN `just design-check` runs THEN the "history rows" value rows are listed under Accepted with the operator's decision as reason.
GIVEN the side-by-side images WHEN a reader compares them THEN every finding is a `report.md` row and none stays open.

## Decision log

- Spec drafted by the drafting agent and approved by the implementer under the operator's mandate for features 0–7 (operator, 2026-10-19). Re-checked against what v2-design-gate built: board titles, the `accepted` list that matches value rows only, `no state` rows; requirement 42 and Phase 8 rewritten to the real tool (implementer under the operator's autonomy mandate, 2026-10-19).
- Board F's waterfall is accepted through value rows of an element "history rows", because the gate's `accepted` list matches value rows only and a class-1 row cannot be accepted (implementer under the operator's autonomy mandate, 2026-10-19).
- The export `export/canvas.html` predates canvas v41 (theme toggle, footer, Tokens pass hairline): toggle and footer are built from the board sources and measured once the operator re-exports (implementer under the operator's autonomy mandate, 2026-10-19).
- Desktop history is the per-line trace of board FTrace, as SDD §5/§5.6 and board M; board F's waterfall rows (9.22–9.25 on F and FDark) go into the manifest's `accepted` list with this decision as the reason (operator, 2026-10-19).

- Phases follow the page from top to bottom, each landable alone: each owns its elements in the
  manifest, so `just design-check` shows its progress row by row (drafting agent, 2026-10-19).
- Score, section grades and hard fails come from `score_run` over the stored run at render time,
  with a guard that hides the score when its grade disagrees with the stored grade; storing the
  score would change the stored `Run` and every fingerprint. The stored grade stays the verdict
  (R5) (drafting agent, 2026-10-19).
- The score is shown as the board shows it, an integer out of 100, not V1's one decimal; the
  board wins over V1 (drafting agent, 2026-10-19).
- Placements for theme H items the boards do not draw: the grade descriptor under the score, the
  section grade in the region label, the hard-fail banner between header and spectrum, duration
  and cached in the run title. The operator decided to rebuild theme H; where the reader flags one
  as extra, the operator's theme-H decision is the disposition (drafting agent, 2026-10-19).
- The run duration is stored on the entry in a new nullable column, not in `Run`, so fingerprints
  and diffs stay byte-identical (CLAUDE.md: a new volatile path changes every stored fingerprint)
  (drafting agent, 2026-10-19).
- Line heights get a 35 % floor with the heaviest line at full height (an affine map of effective
  weight), read from the boards' 35–100 % range; strictly proportional heights leave the spectrum
  flat beside the single IP line (9.15). This amends SDD §5.4; the operator may override
  (drafting agent, 2026-10-19).
- Line codes are spectrum positions ("HTTP·21") as the boards draw them, replacing the catalogue
  short as code; the short stays the spectrum label (drafting agent, 2026-10-19).
- Only `tls.expiry_window`'s short changes; other shorts that are abbreviations (PFS, XFO,
  Legacy, Expired) are not the reported mismatch and stay (drafting agent, 2026-10-19).
- The evidence tabs render every block of the protocol through the shared section renderer, not a
  hand-picked primary block per protocol, so the web crate holds no module knowledge (§3.7); the
  board's single chain table is the TLS sample (drafting agent, 2026-10-19).
- The coverage line (9.39, not on any board) is removed under the default "rebuild to the board";
  its per-section links live in the evidence tabs, so nothing is lost (drafting agent,
  2026-10-19).
- Compare moves from the header to the footer until the site-navigation feature: the boards' nav
  has four items, and dropping the link would lose a page (drafting agent, 2026-10-19).
- Inspectors links to `/inspect/dns`, the first inspector tab, until the inspector shell decides
  otherwise (drafting agent, 2026-10-19).
- The result page's workflow tools (rows 1.11, 1.20, 1.21, 7.64) go to `v2-inspector-shell`, which
  builds the one shared component; this feature leaves room for them in the header (drafting
  agent, 2026-10-19).
- The menu and the clean bar are built on `<details>`, so the phone nav and the passed list work
  without JavaScript, as S7 asks of server-rendered pages (drafting agent, 2026-10-19).

- Header tagline from its own partial: the boards draw two different lines (header "your domain, in full spectrum", footer "Every check of a domain, in one spectrum."), so the single-tagline partial keeps the footer line and the header gets its own (implementer under the operator's autonomy mandate, 2026-10-19).
- The score number is weight 700, the board's `<strong>`, not the drafted 600: the board wins (implementer under the operator's autonomy mandate, 2026-10-19).
- Share stays a link styled as a button: requirement 41 needs the no-JavaScript link, and the island upgrades it (implementer under the operator's autonomy mandate, 2026-10-19).
- The hard-fail banner sits in a stream-replaced wrapper `#hard-fail-slot`, so a streamed re-check updates it (implementer under the operator's autonomy mandate, 2026-10-19).
- `/r/<id>` loads the frame's scripts, so the header's Theme button works there (implementer under the operator's autonomy mandate, 2026-10-19).

- Review of Phase 1: the run number counts stored entries, so the retention sweep renumbers them; the history migration is forward-only (SDD §4.1); the hard-fail sentence follows the score's guard (hidden when `score_run`'s grade differs from the shown grade) and is built for unstored and unrecorded runs too; a history re-read failure after a record drops the run number and the first-run marker instead of claiming run 1 (implementer under the operator's autonomy mandate, 2026-10-19).

- Leader lengths follow the label's row (8, 22, 36 px for rows 1–3), and rows stay the lowest free row of SDD §5.5 (operator decision S20), rather than the board's stagger by label index, which would move OG plates and break S20's property tests (implementer under the operator's autonomy mandate, 2026-10-19).
- A label's `data-code` stays the check id (the key of the sentences and targets); its text is the short (implementer under the operator's autonomy mandate, 2026-10-19).
- The design fixtures `shop.example.com` (three entries) and `new.example.com` now carry the boards' story (CAA, ECH, CSP, DMARC warn; HSTS fail; expiry, DKIM, MTA-STS fixed since the previous entry), each stored grade equal to `score_run`'s (A): the score guard hid the score on the old, inconsistent fixtures and their 14 labels outgrew the 56 px band (implementer under the operator's autonomy mandate, 2026-10-19).
- A token `radius.round` (50 %) draws the legend's fix ring, the board's circle (implementer under the operator's autonomy mandate, 2026-10-19).

- Review of Phase 2: region labels centre and overflow symmetrically, the last one end-aligned, so none leaves the plate; region grades follow the score guard; the key note counts the lines drawn; label widths are measured at the drawn 11 px (SDD §5.5). The phone half of requirement 16 (9 px, "MAIL") is built in Phase 6 with requirement 38. Region grades and the legend name the latest run; while replaying, Phase 3's gutter (requirement 22) names the replayed run's grade, and the region grades and the fixed entry follow the shown run (built with Phase 3). The first visit's legend has no fixed entry, as board FFirst. The design fixtures' evidence blocks are made consistent with their statuses in Phase 5 (implementer under the operator's autonomy mandate, 2026-10-19).

- Phase 3: the trace's caption, hint, edge dates and table link live in the trace component and the gutter in the spectrum component (the gate's ownership rule); the first-run box and the since meta are page parts in `pages.css`; tokens `font.size.title` (17 px) and `font.size.text` (14 px) for the first-run box; a fixed line is a pass mark in the trace. Review: the pinned hint wins over hover on the pinned row, the replayed row carries the 3 px marker, the gutter always holds the run slot, the window's oldest row claims no fix, the caption names a line only while a run is replayed and names it by short and code, the unstored run's since meta keeps the grade change; the phone half of requirement 26 moves to Phase 6 (implementer under the operator's autonomy mandate, 2026-10-19).

- Phase 4: owner labels start with a capital ("Your DNS provider") as the board draws them, the words otherwise unchanged (SDD amended); the check row carries `data-check` with its id on its main column. Review: the since text reads all stored entries, and none when the run's place is unknown; the clean bar counts pass and not-applicable lines only and is hidden without a run; heading, meta and groups count the same listed rows; groups sort fail first and follow their first row, as board F; the switch stays hidden until the island can work it (S7); the phone half of requirement 31 moves to Phase 6; until Phase 5 the evidence section holds no links (implementer under the operator's autonomy mandate, 2026-10-20).

- Phase 5: the explore link reads "Open in the <TOOL> inspector →" after the board, the tool taken from the link's inspector path (email and IP open the DNS inspector); an unmeasured section without a stored reason reads "timed out, nothing measured" or "incomplete, nothing measured" (implementer under the operator's autonomy mandate, 2026-10-20).

- Phase 6: the phone menu is a `details` around the nav with Theme after it; the label band and leaders follow the phone tokens (46 px; 6, 18, 30 px); `font.size.labelPhone` 11 px lowers every label-size text at phone width as board M does; the email region's short "MAIL" comes from `data-short` through `::after` with an empty alt text; browsers without `::details-content` get the menu button on the desktop too (implementer under the operator's autonomy mandate, 2026-10-20).

- Phase 7: `#result-share` becomes a `div` (a dialog cannot sit in a `p`); one canonical origin const feeds the canonical link and every snippet; the badge style query is lens's `style=for-the-badge`. Review: the previews load only when the dialog opens (a badge request on a stale head starts a run in lens), and a share dialog replaced by the stream hands focus to the new Share link (implementer under the operator's autonomy mandate, 2026-10-20).

### Proposals for the operator

- Phone legend and section grades (board M): board M draws no legend under the trace and no section grades on the axis, so at phone width the legend is hidden and the region grades are visually hidden (kept for screen readers; the narrow regions have no room). Options: keep (board M), or show the legend and grades on the phone too (requirement 16 and 18 read without a phone exception).
- Evidence tab content (board F, "adjust canvas" or "deliberately different"): each tab renders every evidence block of its protocol through the shared renderer (Decision log), so the TLS tab of a two-address domain holds two chains, two handshakes, CT and CAA, much longer than the board's single chain table. Options: keep (complete evidence, the board's table is a sample), or a compact first block per protocol with "All evidence →" for the rest (needs module knowledge in the web crate or a summary field per block).

## Open decisions

None.

## Out of scope

- Inspector pages and board G: `v2-inspector-shell` and the TLS depth feature.
- Help modal, shortcuts and copy as Markdown on the result page: `v2-inspector-shell`.
- API score fields: a later API feature.
- Theme J navigation and footer, theme I clients, row 1.18.
