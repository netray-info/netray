# Spec: v2-ui-polish

Status: Ready for Implementation
Created: 2026-10-22

## Goal

The visual findings that features 2 to 7 left to the shell, the evidence view, the guides and the
design gate are rebuilt to the nearest board with existing tokens: the design gate reports a live
board that never reached its state, the DNS query builder and the run actions look like board G's
controls, the `+check` note shows only for `+check` queries, a refused live run keeps the idle
result frame, the email evidence view and the facts grid hold their content at 390 px, guide prose
reads in a column, the shell's check list sits in board F's card, spectrum leaders never cross a
label, and the phone header is board M's. Screenshots at 1280 and 390 px in both themes are read
and `just design-check` exits 0.

Closes the proposals: TLS-UI "design-check should report no state", DNS-UI V10, V12, V13, V14,
V24, V25, HTTP-UI V9, IP-UI V11, site-nav V2, V4, V6, V7, V9, V10, V14, V15.

## Non-goals

- Backend changes (`crates/http`, `crates/tls`, `crates/email`, `crates/dns` module code; lane B).
- Board edits: the claude.ai canvas is unreachable; pages without a board follow the nearest
  board's patterns and are listed as canvas proposals.
- DNS-UI V11 (the builder reflecting a typed query back into its controls).
- New checks, catalogue texts, evidence blocks or keys.

## Context and constraints

- Findings: decision logs and `report.md` of `v2-inspector-shell`, `v2-site-nav`, `v2-http-ui`,
  `v2-ip-ui`, `v2-dns-ui`, `v2-tls-ui`; `specs/solutions/2026-10-21-worktree-binary-measured-a-timed-out-board.md`.
- Boards (private canvas, `netray.info/specs/design/canvas`): F (result page: cards are 1 px line
  border, radius 2 px, surface background; card rows divided by 1 px rules), G (TLS inspector:
  controls 44 px, mono labels, pressed state with the marker and weight 700, dashed "+"), M
  (phone: header holds wordmark, Theme and the menu button only; a wide table scrolls with
  "← swipe the table" under it). No board exists for the DNS, HTTP, email and IP inspectors, the
  evidence view or the guides.
- Gate rules: CLAUDE.md, `frontend-rules.md`, `design-rules.md`. The CSS gate refuses colour,
  radius, font and line-width literals; widths and spacing are free. Layout geometry is computed
  in Rust (§8.4); islands write no style.
- Spectrum label placement: SDD §5.5 (amended by requirement 9).
- The design check: `tests/acceptance/design/{check,page,compare,manifest}.mjs`; manifest
  private in `netray.info/specs/design/canvas/manifest.json`.

## Requirements

### Design gate

1. **Live board without its state.** A manifest element may carry `"shell": true` (a boolean; any
   other value is refused by the loader). For a board with `wait` (a live board), the check
   reports one `no state` row instead of its measured rows when the `wait` selector was not
   reached ("wait <selector> not reached"), or when no element of the board without `shell`
   is on the page ("no board element on the page"). A board without `wait` is unchanged. The
   private manifest marks the shell elements of the two TLS boards. `design-rules.md` names the
   key.

### Inspector shell

2. **DNS query builder.** The builder is a card (board F's card frame) under the form card with
   three labelled rows, each label 12 px muted as the form's: "Types" (ten toggles), "Job" (five
   toggles, one pressed) and "Resolver" (a select). A toggle is board G's port toggle: 44 px
   minimum, mono, 1 px line border, radius 2 px; checked shows the marker, the ink border and
   weight 700; keyboard focus shows the focus ring on the toggle. The native checkbox and radio
   stay in the markup (visually hidden, still focusable) so the `dns` island and forms keep
   working. The select has the form field's border, background, font and 44 px height. Only
   existing tokens.
3. **`+check` note.** The note "+check is not a separate job: the DNS band covers its checks."
   is rendered `hidden` unless the page's query holds the token `+check`; the `dns` island shows
   it while the query field holds the token and hides it otherwise.
4. **Run actions.** In the inspector's run head at 480 px or narrower, the back link takes its
   own line and Copy link, Copy Markdown, CSV and JSON stand on one line, the first flush with the
   run title, each a 44 px target, none wrapping inside. In the evidence view the help button and
   the export row stand on the h1's line at its end at 1280 px (board F's hero actions) and on
   one line under the h1 at 390 px.
5. **Refused run frame.** A notice that replaces the inspector's result region (too many requests,
   busy, invalid input, timed out) renders the idle result frame: the visually hidden h1, the
   notice as the shell's notice card (board F's card frame with a warn-tone marker for "too many
   requests", "busy" and "timed out"; no tone for invalid input) with `role="status"`, and the
   inspector's example chips below it. The text of each notice is unchanged.
6. **Check list card.** The shell's check list (every inspector's band checks, live and shared)
   is board F's card: the heading and meta above, the rows inside one 1 px line border with
   radius 2 px on the surface background, rows divided by the rows' own top rule (none above the
   first row); the row links stay inside the card.
7. **Phone header.** At 480 px or narrower the header row holds the wordmark, Theme and the menu
   button only (board M): the inspector name leaves the header (the tab row marks the inspector,
   the result region names it), and the Theme button keeps its 44 px target and is not squeezed.

### Evidence and components

8. **Email evidence view at 390 px.** (a) The view's explore links stand in one section
   "Explore live" as a list, each link named by its inspector and query ("DNS · shop.example.com
   MX", "TLS · mx1.shop.example.com:25"), never a bare "Explore live". (b) A check row's links to
   its blocks sit inside the row's card in the evidence section. (c) A table-shaped block (a
   table of three or more columns, or records) carries "← swipe the table" under it at 480 px or
   narrower (board M), hidden wider. (d) A block with no values renders "Nothing recorded." in the
   muted tone under its title instead of an empty card body. (e) The cells of an evidence table
   row share a baseline at 390 px.
9. **Spectrum leaders.** SDD §5.5 point 2 becomes: labels are taken right to left; each goes
   into the lowest row where its interval overlaps no label of that row and which lies above
   every placed label whose leader its box covers (covering: the leader's x lies more than half a
   px inside the box), so no leader crosses a box; a box at its leader covers only leaders to its
   right. A box pushed in from the plate's right edge, or wider than the plate, is exempt for the
   leaders it covers to its left. Intersecting boxes form a cluster, placed on its own: when a
   cluster's placement needs more rows than three and than its line-order placement (each label
   in the lowest row free of overlap, the rule before), the line-order placement stands for that
   cluster, as a dense cluster would otherwise stack every label above its neighbour; there
   leaders pass labels. Rows are added as before (at least three); the placement is made at the
   narrowest plate. SDD §5.5 is amended in the same commit.
10. **Wide facts.** A `facts_grid` cell whose values' display text (joined as rendered) is longer
    than `FACT_WIDE_CHARS` (one public constant, 40) spans the grid's full width
    (`data-wide`); the decision is one pure function in Rust, used by the template. Applies to
    every facts grid (TLS certificate, IP card).
11. **Guides.** A guide's article (title, lede, prose, code, tool cards, related guides) reads in
    one column at most 72 characters of body text wide, left-aligned in the main column. At 480 px
    or narrower a guide tool card stacks its field above a full-width submit button, both 44 px.

### Acceptance

12. **Visual reading and gate.** Screenshots at 1280 and 390 px, light and dark, of the DNS
    inspector (idle, a `+check` query, a run), a refused run, the email evidence view, the TLS
    inspector, the IP inspector, a guide and a result page with crowded labels are read beside
    the nearest board by a reader with no part in the build; every finding is a `report.md` row
    and closed. `just design-check` exits 0. At 390 px nothing scrolls sideways.

## Phase 1 — Design gate and spectrum leaders

**Depends on:** v2-tls-ui merged
**Requirements:** 1, 9

### Test Scenarios

GIVEN a manifest element with `"shell": true` WHEN loaded THEN it is accepted; GIVEN `"shell": "yes"` THEN the loader refuses it naming the element.
GIVEN a live board whose page holds only its shell elements WHEN compared THEN one `no state` row reads "no board element on the page" and no measured row.
GIVEN a live board whose wait selector was not reached WHEN compared THEN one `no state` row reads "wait <selector> not reached".
GIVEN a live board whose page holds one non-shell element WHEN compared THEN its measured rows are reported as before.
GIVEN a board without wait whose page holds none of its elements WHEN compared THEN its rows are `missing on page` as before.
GIVEN lines where a later label's leader falls inside an earlier label's box in row 0 WHEN placed THEN the earlier label sits above the later one and no box covers a leader above it; over any input, a box at its leader covers no higher leader unless its cluster fell back to the line-order placement.
GIVEN the existing spectrum scenarios WHEN placed THEN every label keeps a row, rows are at least three, and no two boxes of one row overlap.

## Phase 2 — Inspector shell

**Depends on:** Phase 1
**Requirements:** 2, 3, 4, 5, 6, 7

### Test Scenarios

GIVEN the idle DNS page WHEN rendered THEN the builder holds the rows Types, Job and Resolver, each toggle a label of class `dns-builder__toggle` around its native input, and the stylesheet styles the checked toggle with the marker.
GIVEN `/inspect/dns` WHEN rendered THEN the `+check` note is `hidden`; GIVEN `/inspect/dns?q=example.com%20%2Bcheck` THEN it is not hidden.
GIVEN the `dns` island WHEN the query field gains ` +check` THEN the note shows, and WHEN the token leaves THEN it hides.
GIVEN the stylesheets WHEN read THEN at 480 px or narrower the run actions put the back link on its own line and the share and export buttons in one non-wrapping row.
GIVEN the evidence view WHEN rendered THEN the h1 and the tools row share one head element.
GIVEN a stream refused with too many requests WHEN its fragment renders THEN it holds the visually hidden h1, a `p.inspector-notice` with `role="status"` and `data-tone="warn"` holding the unchanged text, and the inspector's example chips.
GIVEN an invalid input notice WHEN rendered THEN the notice has no `data-tone`.
GIVEN a run's check list WHEN rendered THEN its rows sit inside one `.inspector-checks__card` element with its row links.
GIVEN the stylesheets WHEN read THEN at 480 px or narrower `.page-inspector` is not displayed.

## Phase 3 — Evidence view, facts grid and guides

**Depends on:** Phase 1
**Requirements:** 8, 10, 11

### Test Scenarios

GIVEN the stored email view of shop.example.com WHEN rendered THEN one section "Explore live" lists links named "DNS · shop.example.com MX", "TLS · mx1.shop.example.com:25", "DNS · _dmarc.shop.example.com TXT" and "DNS · _mta-sts.shop.example.com TXT", and no link text is a bare "Explore live".
GIVEN a check row with block links in the evidence section WHEN rendered THEN the links sit inside the row's card element.
GIVEN a table block of three columns WHEN rendered THEN a `.evidence-block__swipe` follows the table; GIVEN a two-column table THEN none.
GIVEN `email.bimi` with no fields WHEN rendered THEN the block reads "Nothing recorded."
GIVEN a facts cell of 41 characters WHEN `wide` is asked THEN it is true; GIVEN 40 THEN false; GIVEN a chips cell whose joined names pass 40 THEN true.
GIVEN a TLS certificate grid with a long SAN list WHEN rendered THEN its cell carries `data-wide`.
GIVEN a guide WHEN rendered THEN its article sits in a `.guide-article` column whose stylesheet sets a 72ch maximum.

## Phase 4 — Visual reading and design gate

**Depends on:** Phases 1–3
**Requirements:** 12

### Test Scenarios

GIVEN the manifest WHEN `just design-check` runs THEN it exits 0.
GIVEN the screenshots WHEN read beside the nearest board THEN every finding is a `report.md` row and none stays open.
GIVEN every captured page at 390 px WHEN measured THEN the document is not wider than the viewport.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate (operator,
  2026-10-19, extended 2026-10-22).
- The "429 page without the site frame" (DNS-UI V24) reproduces as the refusal notice replacing
  the inspector's result region with a bare unstyled line; the page's header and footer stay.
  Requirement 5 rebuilds that region to the idle frame (implementer under the operator's autonomy
  mandate, 2026-10-22).
- A live board's shell elements are named in the manifest (`shell`), not guessed from the
  result region: the TLS board's band and section heading render in a timed-out run too
  (implementer under the operator's autonomy mandate, 2026-10-22).
- Requirement 9 rewritten during Phase 1: the drafted "a label in a lower row never covers a
  higher leader" with line-order placement has no solution in a dense run (every box wider than
  the pitch: each label above its neighbour, 50 rows on an all-fail card). Built: right-to-left
  placement clear of leaders per cluster of intersecting boxes, the line-order placement where
  a cluster would need more rows than three and than line order, and boxes pushed in from the
  right edge exempt for the leaders to their left (implementer under the operator's autonomy
  mandate, 2026-10-22).
- The phone header drops the inspector name rather than shrinking it: board M draws wordmark,
  Theme and menu only, and the tab row already marks the inspector (implementer under the
  operator's autonomy mandate, 2026-10-22).
- The swipe note is static at phone width for tables of three or more columns (board M's
  pattern); knowing whether a table overflows would need an island measuring layout (§8.4)
  (implementer under the operator's autonomy mandate, 2026-10-22).
- An empty block reads "Nothing recorded." as presentation of the absence; the block itself
  holds no placeholder (CLAUDE.md) (implementer under the operator's autonomy mandate,
  2026-10-22).
- The guide column uses a `ch` width, not a token: the gate tokens colours, radii, fonts and
  lines, and no board draws a guide (implementer under the operator's autonomy mandate,
  2026-10-22).

## Open decisions

None.

## Out of scope

- DNS-UI V11 (builder reflects the query), boards for the inspectors, home and /tools.
