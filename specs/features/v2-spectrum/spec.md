# Spec: v2-spectrum

Status: Ready for Implementation
Created: 2026-10-17

## Goal

`netray_web::spectrum::layout` is one pure Rust function. It takes the current run, the older runs
(per-check statuses, each with its own scoring profile), the catalogue and a container width in px.
It yields the spectrum's geometry, which the result page, `/r/` and the OG card will all use (SDD
V4.4, S7, S20):

- one line per catalogue check, grouped by protocol, with x positions shared by every run;
- per run, each line's tone and height, every warn, fail and fixed line's label in a row, and the row count.

A property test over random runs and widths asserts four things. No two labels in a row overlap.
Every warn, fail and fixed line has exactly one label and no other line has one. The plate is
exactly the container width while the pitch is at least 8 px, and n × 8 px below that, so rows are
added and the plate never widens. A placement stays valid at every wider container. The 4a
`spectrum` component renders a layout, every run precomputed and one shown, with one `band-<protocol>` group per protocol, label rows, leaders and sideways scroll, and
the design-system gate passes. `just adlc-verify` runs it all.

## Non-goals

- No route and no page. The result page (V4.5) and `/r/` (V2.7) come later, and so do the `spectrum` island, the trace gutter and the history sentences (V4.6, §5.6). This feature renders every run's precomputed state into the component (requirement 18); switching between them is the island's.
- No change to the OG card. It stays the V1 grade card (`crates/lens/src/og/render.rs:98-143`) until `v2-result-page` switches it to the layout (DECISIONS X4, see the decision log).
- No change to lens, the `netray` binary, any wire or any golden.
- No history type: no `Run`, no `netray-store`, nothing from the unmerged `v2-history` lane.
- No placement of the group names on the axis row. They keep 4a's position, at the group start (`spectrum.html:23-27`).
- No trace geometry. The trace component keeps its flex cells (`templates/components/trace.html`, `static/trace.css:22-33`).
- No release. The overnight build goes to a local 0.30.0 (operator, 2026-10-17).

## Context and constraints

- Build order (one order in every spec): lane A `v2-history` → `v2-admission` → `v2-entries` → `v2-api`; lane B `v2-spectrum` ∥ `v2-pages`, parallel to lane A; lane C, after the operator's UI review at midday 2026-10-18, `v2-inspectors` (its Phase 2 before `v2-result-page` Phase 1), then `v2-result-page`; then `v2-cutover`. This feature depends on no other feature.
- SDD `specs/sdd/v2.md`: S7 (`:33`, one layout function that also feeds the OG card), S16 (`:41`, 54 checks scroll at 390 px), S20 (`:45`), §3.8 (`:322-324`, a replayed run is scored with its own profile), §3.9 (`:454`, a line's height is the effective weight, "its weight relative to its section's measured weights, times the section weight"), §5.4 (`:677-695`), §5.5 (`:697-716`), §5.6 (`:718-733`), §8.3 (`:888-906`), V4.4 (Phase 4 table).
- The signature in §5.4:679 is `spectrum::layout(entry, history, catalogue, profile, width)`. `entry` and `history` are stored-entry types that only `v2-history` and `v2-entries` bring. This feature must not depend on that lane, so the layout takes plain per-check statuses. A later caller maps its `Run` onto them.
- §5.5 says "every non-passing line keeps its label" (`:711`). §5.4 (`:691`) and V4.4 say every warn, fail and fixed line. A fixed line is passing, so the two wordings conflict. This spec follows §5.4 and V4.4, and amends §5.5.
- `crates/web` (package `netray-web`) depends on `netray-common`, `askama` and `serde_json` (`crates/web/Cargo.toml:8-11`). SDD §3.1 lets it depend on `model` and `engine`, and both are workspace dependencies (`Cargo.toml:11-12`). `tests/repo/test_web_crate.sh:44` forbids axum in `netray-web`, and `:51` forbids the `netray` binary's direct dependency on it (it reads only that package's own `dependencies`, so a lens → `netray-web` edge would pass it), until `v2-pages`, `v2-entries` or `v2-api` lifts them (X1); this feature changes neither guard, and whichever of `v2-pages` and this feature lands second rebases onto the other's `crates/web/Cargo.toml`.
- The catalogue is `netray_engine::catalogue()` (`crates/engine/src/catalogue.rs:158-163`). It holds 54 entries, already grouped DNS 10, TLS 21, HTTP 12, email 10, IP 1 in `checks.toml`. Each entry has a `short` of at most 8 characters (`catalogue.rs:13-14`, `:109-112`). Today's shorts are 2 to 8 characters and some contain a space (`Lame NS`, `HTTPS RR`, `CAA ok`, `MX DANE`).
- `Protocol` declares `Dns, Tls, Http, Email, Ip` in that order (`crates/model/src/lib.rs:14-20`), and `Status` has six values (`:119-126`).
- A profile holds per-section `weight` and per-check weights (`crates/engine/src/profile.rs:28-44`). Only v3 is embedded (`profile.rs:141-153`). In v3 the section weights are DNS 20, TLS 35, HTTP 20, email 15 and IP 10. The check weights sum to 35 for DNS, 103 for TLS, 23 for HTTP and 22 for email, and IP's single check `reputation` weighs 5 (`crates/engine/profiles/v3.toml`).
- Scoring counts only pass, warn and fail as measured: not-applicable and not-tested count neither way, and unmeasured makes the section unscored (`crates/engine/src/scoring.rs:40-69`).
- Tokens (`crates/web/tokens.json`):
  - `spectrum.linePitchMin` is 8px (`:406-410`). Its description still says "41 checks … 8.7 px".
  - `font.size.label` is 12px (`:274-278`), `space.1` is 4px, `spectrum.plateHeight` is 92px and 80px at phone width (`:401-405`).
  - `build.rs` exposes each value as a `&str` constant named by its path, for example `SPECTRUM_LINE_PITCH_MIN = "8px"` (`crates/web/build.rs:166-174`, `src/tokens.rs:8-10`).
- The 4a component:
  - `SpectrumLine { code, x, height, tone, label }` takes x and height as percentages of the plate (`crates/web/src/components.rs:128-147`), and `SpectrumView { label, lines, regions }` holds them (`:157-163`).
  - The macro renders labels only for the warn, fail and fix tones, in one row at their line's x (`templates/components/spectrum.html:3-7`). It draws no leader.
  - The stylesheet sets labels in `--font-size-caption` (11px, 10px at phone width) with `padding-inline-start: var(--space-1)` (`static/spectrum.css:53-58`, `:66-71`), and the plate is `width: 100%` with no scroll (`:9-16`).
- The test files `crates/web/tests/components.rs:123-160` and `crates/web/tests/drawing_rules.rs:67-110`, `:278-306` build `SpectrumLine` and `SpectrumView` literals. These tests are protected, so changing the view structs needs `ADLC-Test-Change` naming each file, with the drawing-rule assertions kept.
- The gate (`crates/web/src/gate.rs`, 4a requirement 6) refuses colours, radii, fonts and widths outside the tokens in templates, stylesheets and `style` attributes. Positions (`left`, `bottom`, `height`, `min-width`) are not token-bound.
- The OG card is rendered by lens (`crates/lens/src/routes.rs:358-360`). It reads lens's moka cache and has a grade, a score and a timestamp, but no predecessor (`crates/lens/src/og/handler.rs:95-121`). Its font database holds only Inter (`crates/lens/src/og/fonts.rs:5-6`).
- `proptest` 1.11.0 (MIT/Apache-2.0) is in the local cargo cache. No crate uses a property-testing library today. With a single user it is a crate-local dev-dependency (CLAUDE.md "Shared dependencies").
- `v2-result-page` (its requirements 15 and 17, Phase 1; the OG cards in its requirement 29) calls `spectrum::layout(current, older, catalogue, width)` with per-check statuses, section states and a profile per run (requirement 10), mapping each shown `Entry` onto them, and relies on per-run tones and label rows for every shown entry and on `spectrum`/`trace` templates that mark each protocol band with the ID `band-<protocol>`, carry the per-run states and show an unmeasured band (requirement 18). Its island (its requirement 17) holds three values and writes only `data-*`, `aria-*`, class attributes and text. `v2-inspectors` (its requirement 25) calls it over `Catalogue::for_protocol` with one live run and no older run. The trace already carries one `data-tone` per cell and run (`templates/components/trace.html:7`).
- Nothing in this feature is user-visible: no route serves the component until `v2-result-page`. The `(default, not interviewed)` values below therefore do not halt the implementer. `v2-result-page` revisits them against the design canvas.
- Gate: `just adlc-verify` (`cargo test --workspace --exclude netray-ip`, offline). Amend `specs/sdd/v2.md` in the same commit as the code that changes what it says. `v2-history` amends §3.1, §3.7, §4 and the Phase 2 table, and this feature touches none of them.

## Requirements

1. `crates/web/src/spectrum.rs` is the module `netray_web::spectrum`. It holds the layout as pure functions with no I/O, no askama and no global state. Every renderer gets its geometry from `spectrum::layout`; no other function in the workspace places spectrum labels.
2. For n ≥ 1 lines and a container width `W` (px, `u32`):
   - when `W ≥ n × linePitchMin`, the plate width is `W` and the pitch is `W / n`;
   - otherwise the plate width is `n × linePitchMin` and the pitch is `linePitchMin`.

   `linePitchMin` is read from `tokens::values::SPECTRUM_LINE_PITCH_MIN` (a px parse that panics on any other unit), never written as a literal. A layout with no lines is refused with an error.
3. Line `i` (0-based, in line order) sits at x = (i + 0.5) × pitch. Each group starts at x = k × pitch, where k is the index of its first line. Its width is its line count × pitch. A boundary sits at each group start except the first.
4. A label box is `chars × 0.62 × font.size.label + space.1` px wide (§5.5:702-705, both values read from `tokens::values`). With 12px and 4px, a 4-character label is 33.76 px. The box is left-aligned at its line, [x, x + w), with the padding between leader and text, as `spectrum.css:66-69` draws it (default, not interviewed).
   - A box that would pass the plate's right edge is shifted to start at plate width − w.
   - A box wider than the plate starts at 0.
   - The leader always stays at the line's x.
5. Labels are placed in line order. Each goes into the lowest row (row 0 has the shortest leader) where its box overlaps no box already in that row; half-open boxes that only touch do not overlap. The row count is at least 3. When no existing row is free, the label opens a new row above, and no label is ever dropped. The plate width never depends on the labels.
6. Exactly the lines with tone warn, fail or fixed carry a label, one each, and its text is the line's `short`. Pass lines and muted lines (not applicable, not tested, unmeasured) carry none.
7. A placement computed at width `W` stays valid at any width `W' ≥ W` when every x and every box start is scaled by the ratio of the plate widths: no two boxes in a row overlap, and every box no wider than the plate stays inside it. (This is the property the page relies on; see the decision log.)
8. A property test (`crates/web/tests/spectrum_layout.rs`, proptest 1 as a dev-dependency of `netray-web`) runs over random line lists and random widths:
   - Inputs: 1–120 lines; labels of 1–8 characters including spaces; random tones among pass, warn, fail, fixed and muted; random group splits; widths 100–2400 px.
   - It asserts the claims of requirements 2–7:
     - (a) no overlap within a row;
     - (b) the labelled set equals the warn, fail and fixed set, with one label each;
     - (c) the plate width and pitch of requirement 2, and pitch ≥ `linePitchMin` always;
     - (d) rows ≥ 3, and every label in row k > 0 overlaps a label in each row below k (no gratuitous row);
     - (e) every box no wider than the plate lies within [0, plate width], and each leader is at its line's x;
     - (f) requirement 7 for a second random `W' ≥ W`.
   - It runs 256 cases with failure persistence off, so the gate writes no file into the checkout (default, not interviewed).
9. SDD §5.5 is amended. Point 3's "every non-passing line" becomes "every warn, fail and fixed line". Point 2 states the left alignment at the leader, the half-open overlap and the shift of an over-wide box to 0.
10. `spectrum::layout(current, older, catalogue, width)` takes:
    - each run as its per-check statuses (`CheckId` → `netray_model::Status`), its per-section states (`Protocol` → `spectrum::SectionState`) and the `&netray_engine::Profile` it was scored with, `older` newest first and possibly empty;
    - `SectionState` is `Measured`, `NotApplicable` or `Unmeasured`, a plain enum of `netray-web`; a caller maps `SectionOutcome::Measured` to `Measured`, `NotApplicable` to `NotApplicable`, and `Incomplete` and `TimedOut` to `Unmeasured` (`crates/engine/src/lib.rs:112-125`). A protocol absent from the map is `Measured` (default, not interviewed);
    - the `&netray_engine::Catalogue`;
    - the width.

    It yields one line per catalogue entry, ordered by `Protocol` (DNS, TLS, HTTP, email, IP) and by catalogue order within a protocol. The x geometry of requirements 2–3 is shared by every run. Per run, current first, it yields each line's tone and height, its labels per requirements 4–6, its row count, and each protocol's `SectionState`. `netray-web` depends on `netray-model` and `netray-engine` (`{ workspace = true }`), and no type in the layout's signature comes from history, store or lens.
11. A line's tone in a run:
    - pass, warn and fail map to their own tone;
    - not applicable, not tested and unmeasured map to the muted tone (`Tone::Untested`), and so does a catalogue check absent from the run (default, not interviewed) or absent from the run's profile (requirement 12);
    - a line is fixed when the run's predecessor (the next older run given) has warn or fail and the run has pass (R12's "fixed", §4.3). Warn or fail followed by not applicable is not fixed.

    The oldest run given has no predecessor and therefore no fixed line.
12. A measured line's height in a run follows the effective weight of §3.9:454, under that run's profile. A measured line is one whose status is pass, warn or fail and whose check the run's profile weights.
    - e = w / M × W_s, where w is the check's weight, W_s its section's weight, and M the sum of the weights of the section's measured checks in this run (M > 0 whenever the line itself is measured).
    - The height is e divided by the largest e of the run's measured lines, so the tallest measured line is at full plate height (1.0).
    - Every other line (not applicable, not tested, unmeasured, absent from the run, or absent from the run's profile) is muted and takes the muted height: the smallest height of the run's measured lines, so it never exceeds a measured line, scoring weight 0 (`crates/engine/src/scoring.rs:58`) never draws taller than weight > 0, and no line is missing (default, not interviewed). A run with no measured line draws every line at the muted height 1.0.
    - A catalogue check the run's profile does not weight (a newer check shown against an older run's profile) is drawn muted, carries no label and is never fixed; it is not an error.
13. A second property test runs over the real `catalogue()` and `Profiles::current()`. It uses random statuses for a current run and one predecessor, and widths of 100–2400 px. It asserts requirement 8's claims (a)–(f), and in addition: exactly 54 lines; every height in (0, 1]; every muted line's height ≤ every measured line's height in its run; identical x for both runs; a label on every fixed line, that is, on every warn-or-fail → pass pair.
14. SDD amendments ride with requirement 10's code:
    - §5.4 gives the built signature and module path, and says that a caller maps its stored entries onto statuses with profiles;
    - §3.9:454 adds that only measured lines take the effective weight and that every other line takes the muted height of requirement 12;
    - §8.1's table gains `src/spectrum.rs`;
    - the Phase 4 table's V4.4 row says that `v2-spectrum` builds the layout and both property tests, that `v2-result-page` wires the page (V4.5) and switches the OG cards, `/r/<id>/og.png` included, to the layout (X4), and that `v2-inspectors` draws the protocol bands from it;
    - the description of `spectrum.linePitchMin` in `crates/web/tokens.json` says 54 checks at 390 px with 16 px gutters (358 px of content) give 6.6 px, as §5.4:695 does.
15. The `spectrum` component renders a layout. A single conversion in `netray-web` builds `SpectrumView` from a layout and the index of the run shown first (requirement 18 adds the other runs).
    - Line x is a percentage of the plate width, and height a percentage of the plate height.
    - Each label is rendered at its box start (a percentage) in its row: `bottom: calc(var(--spectrum-label-leader) + <row> * var(--spectrum-label-row))`.
    - A leader at the line's x has the same calc as its height and is drawn at `--line-hair` in the label's status mark colour (default, not interviewed).
    - Every leader precedes every label in document order, and each label box has an opaque `background: var(--color-bg)` (the page background the label band sits on). A leader that runs up past a lower row's label therefore passes behind that label's box and never crosses its text; greedy left-aligned placement makes such crossings common (default, not interviewed).
    - The label band is `calc(var(--spectrum-label-leader) + <rows> * var(--spectrum-label-row))` high, where `<rows>` is the largest row count of any run in the layout, so switching runs never changes its height.
    - Labels, plate and axis sit in one box with `min-width: calc(<n> * var(--spectrum-line-pitch-min))` inside `.spectrum`, which scrolls sideways (`overflow-x: auto`), so all three scroll together (R26).
    - The page renders the placement computed at `n × linePitchMin`, 432 px for 54 checks (decision log).
    - The 4a drawing rules hold unchanged: state widths, the hollow fixed line with its ring, the muted mark, boundary hairlines, group names below the plate and labels above it. The test literals change under `ADLC-Test-Change`.
16. `crates/web/tokens.json` gains `spectrum.labelLeader` = 8px and `spectrum.labelRow` = 14px, the canvas stagger of 8, 22 and 36 px that §5.5:699 cites (default, not interviewed). They are generated as `--spectrum-label-leader` and `--spectrum-label-row`. Line labels use `--font-size-label` and `--font-mono`, the size and font that requirement 4 measures, in place of `--font-size-caption`. The gate passes on the changed template and stylesheet.
17. SDD §5.5 point 3 names the two row tokens, and §8.3's `spectrum` row says the component renders a layout with every run's state and one `band-<protocol>` group per protocol.
18. Bands and per-run states (§5.5 point 4, §5.3's band fragments), the contract `v2-result-page` builds on:
    - The plate holds one `<g id="band-<protocol>" data-protocol="<protocol>">` per protocol in requirement 10's order (`dns`, `tls`, `http`, `email`, `ip`, the lower-case `Protocol` name). It holds that protocol's lines and the boundary at its start. IDs are unique per page, so the component is rendered once per page with bands.
    - Inside each band, every run of the layout is one `<g class="spectrum__run" data-run="<k>">` (k = 0 for the current run, then newest first) with that run's tones and heights. The label band likewise holds one `<div class="spectrum__labels" data-run="<k>">` per run with that run's placement.
    - Exactly the run shown first carries the attribute `data-shown` on its groups and label set; the stylesheet hides every `[data-run]:not([data-shown])`. The island switches runs by moving that attribute only, which is §5.6's "switches data attributes; computes no layout" as written, so §5.6 is not amended.
    - A band's run group carries `data-unmeasured` exactly when that run's `SectionState` for the protocol is `Unmeasured` (an incomplete or timed-out section, §5.3:670, Rule M). A `NotApplicable` section (e.g. TLS or HTTP with "no A or AAAA record", `crates/engine/src/lib.rs:299-303`) and a `Measured` section never carry it, whatever their line statuses. Either way the lines stay muted at the muted height (requirement 12), so no line is missing.
    - The SDD §5.5 point 4 sentence gains the markup shape (bands, `data-run`, `data-shown`, `data-unmeasured` for an unmeasured section).

## Phases

## Phase 1 — Layout core

**Depends on:** none (lane B, parallel to lane A and `v2-pages`)
**Requirements:** 1, 2, 3, 4, 5, 6, 7, 8, 9

`crates/web/src/spectrum.rs`: the pure placement over a list of line inputs (short, group, tone), covering pitch and plate width, x, groups and boundaries, label boxes, greedy rows and the edge shift, plus the token px parse. `crates/web/tests/spectrum_layout.rs` holds the proptest suite and the table scenarios. `proptest` becomes a dev-dependency in `crates/web/Cargo.toml`. The SDD §5.5 amendment goes in the same commit.

### Test Scenarios

GIVEN 54 lines WHEN laid out at 1056 px THEN the plate is 1056 px, the pitch is 19.56 px and line 0 is at x 9.78.
GIVEN 54 lines WHEN laid out at 432 px THEN the plate is 432 px and the pitch is 8 px; at 431 px or 358 px THEN the plate is 432 px and the pitch is 8 px.
GIVEN no lines WHEN laid out THEN an error is returned.
GIVEN lines 0–3 of 54 failing with 4-character labels at 432 px (pitch 8 px) WHEN placed THEN they take rows 0, 1, 2 and 3, and the row count is 4 (§5.5:711-712).
GIVEN lines 0–3 of 54 failing with 4-character labels at 648 px (pitch 12 px, boxes start at 6, 18, 30 and 42 px) WHEN placed THEN the fourth label returns to row 0 and the row count is 3.
GIVEN one warn line WHEN placed THEN its label is in row 0 and the row count is 3.
GIVEN 54 lines at 432 px with the last line failing and an 8-character label (63.52 px) WHEN placed THEN its box starts at 368.48 px with its leader at x 428; GIVEN a failing line whose box fits THEN its box starts at the line's x.
GIVEN 4 lines at 32 px (plate 32 px) with an 8-character fail label WHEN placed THEN its box starts at 0.
GIVEN a pass line, a muted line and a fixed line WHEN placed THEN only the fixed line carries a label.
GIVEN two labels whose boxes touch end to start WHEN placed THEN they share a row.
GIVEN a dense placement computed at 432 px WHEN scaled to 1056 px THEN no two boxes in a row overlap and every box stays inside the plate.
GIVEN random line lists and widths WHEN the property test runs 256 cases THEN claims (a)–(f) hold and no file is written under the crate.

## Phase 2 — Layout from catalogue, profiles and runs

**Depends on:** Phase 1
**Requirements:** 10, 11, 12, 13, 14

`crates/web/src/spectrum.rs`: the public `layout` over the catalogue and each run's statuses and profile, covering the protocol order, the tone mapping with fixed against the next older run, effective-weight heights with the muted height, section states, and the per-run output. `crates/web/Cargo.toml` gains `netray-model` and `netray-engine`. The real-catalogue property test and the SDD and token-description amendments go in the same commit.

### Test Scenarios

GIVEN the real catalogue WHEN laid out THEN line 0 is `dns.dnssec`, line 53 is `ip.reputation`, and boundaries sit at lines 10, 31, 43 and 53.
GIVEN a current run, all pass except `dns.caa` warn and `tls.not_expired` fail, and a predecessor with `http.hsts` fail that is pass now WHEN laid out THEN the labels are exactly `CAA`, `Expired` and `HSTS`, and `HSTS`'s line is fixed.
GIVEN a predecessor with `dns.caa` warn and a current run with `dns.caa` not applicable WHEN laid out THEN the line is muted, unlabelled and not fixed.
GIVEN three runs WHEN laid out THEN the oldest run has no fixed line and every run has the same x per line.
GIVEN a current run missing `email.bimi` WHEN laid out THEN that line is muted, and its height equals the run's smallest measured height (> 0).
GIVEN an all-pass run under v3 WHEN laid out THEN `ip.reputation` has height 1.0 (e = 10), `tls.tls_reachable` 0.34 (10/103 × 35 / 10) and `http.csp` 0.087 (1/23 × 20 / 10).
GIVEN a run with `tls.tls_reachable` fail and every other TLS check not applicable WHEN laid out THEN `tls.tls_reachable` has height 1.0 (e = 35) and `ip.reputation` 0.286.
GIVEN an all-pass run under v3 except `email.dmarc` not applicable WHEN laid out THEN `email.dmarc` is muted at the muted height, the smallest height of the run's measured lines, which lies below dmarc's weighted 0.27 (4/22 × 15 / 10).
GIVEN a run whose email checks are all not applicable WHEN laid out THEN every email line is muted at the run's smallest measured height, no taller than any measured line.
GIVEN an older run scored with an in-memory profile that doubles the DNS section weight WHEN laid out THEN its DNS heights differ from the current run's, and its x positions do not.
GIVEN an older run scored with an in-memory profile without `dns.ttl`, and `dns.ttl` pass in it WHEN laid out THEN the layout succeeds, and in that run `dns.ttl` is muted at the muted height, unlabelled, and the current run's `dns.ttl` is not fixed against it.
GIVEN random statuses for a current run and a predecessor and random widths WHEN the real-catalogue property test runs THEN requirement 13's claims hold.
GIVEN `cargo metadata` WHEN `tests/repo/test_web_crate.sh` runs THEN it passes in the shape it has when this phase runs (this feature adds no axum, no `Router` and no direct `netray` → `netray-web` dependency).

## Phase 3 — The component renders a layout

**Depends on:** Phase 2
**Requirements:** 15, 16, 17, 18

`crates/web/src/components.rs` holds the view conversion from a layout. The phase also changes `templates/components/spectrum.html` (label rows, leaders, scroll box, bands, per-run groups), `static/spectrum.css` (label font, opaque label box, leader, scroll, `data-shown`) and `tokens.json` (two row tokens), updates the 4a tests' view literals under `ADLC-Test-Change`, and amends SDD §5.5 and §8.3.

### Test Scenarios

GIVEN the real catalogue's layout at 432 px with a fail on line 0 WHEN rendered THEN the line's `x1` is `0.9259%` (4/432), and the box carries `min-width: calc(54 * var(--spectrum-line-pitch-min))`.
GIVEN a label in row 2 WHEN rendered THEN its style holds `bottom: calc(var(--spectrum-label-leader) + 2 * var(--spectrum-label-row))`, and a leader at its line's x has the same calc as its height.
GIVEN a run with 4 rows WHEN rendered THEN the label band is `calc(var(--spectrum-label-leader) + 4 * var(--spectrum-label-row))` high; GIVEN 1 used row THEN it is 3 rows high; GIVEN a current run with 3 rows and an older run with 5 THEN it is 5 rows high.
GIVEN the real catalogue's layout WHEN rendered THEN the plate holds exactly the groups `band-dns`, `band-tls`, `band-http`, `band-email` and `band-ip` in that order, each ID once, with 10, 21, 12, 10 and 1 lines per run.
GIVEN a layout of three runs WHEN rendered with run 0 shown THEN each band holds `data-run` 0, 1 and 2, the label band holds three label sets, and only run 0's groups and label set carry `data-shown`; GIVEN run 1 shown THEN only run 1's do.
GIVEN a run whose email section is `Unmeasured` (timed out) WHEN rendered THEN `band-email`'s group for that run carries `data-unmeasured` and its 10 lines are muted with a height > 0; no other band's group does.
GIVEN a run whose TLS and HTTP sections are `NotApplicable` ("no A or AAAA record") WHEN rendered THEN neither `band-tls` nor `band-http` carries `data-unmeasured`, and their lines are muted with a height > 0.
GIVEN two renders of one layout with different runs shown WHEN diffed THEN only `data-shown` attributes differ.
GIVEN a layout with labels in rows 0 and 1 WHEN rendered THEN every leader element precedes every label element in the label band, and `.spectrum__line-label` has `background: var(--color-bg)`.
GIVEN a pass line and a muted line WHEN rendered THEN the label band holds no label and no leader for them.
GIVEN `spectrum.css` WHEN resolved THEN `.spectrum` has `overflow-x: auto`, and `.spectrum__line-label` has `font-size: var(--font-size-label)` and `font-family: var(--font-mono)`.
GIVEN the generated `tokens.css` WHEN read THEN `:root` holds `--spectrum-label-leader: 8px` and `--spectrum-label-row: 14px`.
GIVEN the 4a drawing-rule tests WHEN run against the new view THEN they pass, with only their literals changed.
GIVEN the changed template and stylesheet WHEN the gate runs THEN it passes; GIVEN a planted `bottom: 22px; font-size: 12px` on a label THEN it fails on the font size only.

## Decision log

- The layout takes per-check statuses and a profile per run, not `entry` and `history`, because the feature must not wait for the history lane (operator, 2026-10-17). A stored entry maps onto it in its caller, `v2-result-page`, for the page, the trace and the OG cards. Each run carries its own profile because §3.8 scores a replayed run with its own profile (implementer, from the SDD, 2026-10-17).
- Placement once at `n × linePitchMin` (432 px for 54 checks, the narrowest plate) over placements per container width toggled by container queries: requirement 7 makes the one placement valid at every wider container, so the markup and the per-run replay data exist once; dense issues on desktop take more rows than the width would need (operator, 2026-10-17).
- OG card switch in `v2-result-page` over `v2-entries`: `v2-result-page` owns the page that consumes the layout, and `v2-entries` renders the entry OG card in the V1 layout until then (operator, 2026-10-17, DECISIONS X4). It does not switch here either; the reasons come from the code (implementer, 2026-10-17):
  - R24's fixed line needs the predecessor, and the OG handler reads only lens's cache (`og/handler.rs:95-121`).
  - X4 assigns the switch to `v2-result-page`. No guard stands in the way: `test_web_crate.sh:51` reads only the `netray` binary's direct dependencies, so a lens → `netray-web` edge would pass it; the reason is ownership, and a lens edge here would also collide with `v2-entries`' lens changes in lane A while this feature runs in lane B.
  - The OG font database has no mono face for the 0.62 em label measure (`og/fonts.rs:5-6`).
- Labels go on warn, fail and fixed lines (§5.4, V4.4) rather than §5.5's "every non-passing line". A fixed line passes and must carry its label (R24). §5.5 is amended (implementer, from the SDD, 2026-10-17).
- Heights use the run's measured weights, as §3.9:454 says, not the static profile weights, so the SDD's wording holds.
- Muted lines at the run's smallest measured height over the effective weight with an all-weights fallback: not applicable and not tested carry scoring weight 0 (`scoring.rs:58`), so a muted line drawn taller than a measured one would misstate the grade, while one fixed minimal height keeps every line visible and needs no new constant or token (implementer under the operator's autonomy mandate, 2026-10-17).
- A catalogue check absent from a run's profile drawn muted over refused with an error: a newer profile adds checks, and the page shows older entries scored with older profiles, so an error would break every history spanning a profile change (implementer under the operator's autonomy mandate, 2026-10-17).
- `data-unmeasured` from a per-run `SectionState` the caller supplies over inferring it from the absence of pass, warn and fail lines: a not-applicable section (`lib.rs:299-303`) has no measured line either, and drawing it as "could not check" contradicts §5.3:670 and Rule M (implementer under the operator's autonomy mandate, 2026-10-17).
- The shown run marked by `data-shown` over a `spectrum__run--shown` class with an amended §5.6: §5.6 says the island "switches data attributes", so a data attribute needs no SDD change; `v2-result-page` requirements 15 and 17 and its Phase 1 scenario name the class and follow this contract (implementer under the operator's autonomy mandate, 2026-10-17).
- Opaque label boxes drawn after the leaders over a placement that keeps leaders clear of lower labels: greedy left-aligned placement puts a lower row's text across most upper labels' leaders; a background and document order fix every case and a render test asserts them, while a leader-avoiding placement would add a constraint to the greedy rows and their property test (implementer under the operator's autonomy mandate, 2026-10-17).
- The label box is left-aligned at its leader, matching 4a's `padding-inline-start` (`spectrum.css:66-69`) and the 4 px of §5.5's formula, rather than centred on the line (default, not interviewed).
- The row offsets 8 and 14 px are tokens, not layout constants. The template then writes no px, and the OG renderer reads the same values (P36) (default, not interviewed).
- The layout takes the width as an integer `u32` rather than a float, so no NaN or negative width can reach the placement (implementer, 2026-10-17).
- proptest is a crate-local dev-dependency, not a hand-rolled seeded generator: it shrinks a failing case to a minimal one, and it has one user, so it stays out of `[workspace.dependencies]` (implementer, 2026-10-17).

- Every run's state rendered into the component, switched by one data attribute, over per-line data attributes the island writes into geometry: heights differ per run (each run's profile and measured weights), and the island may write only `data-*`, `aria-*`, class attributes and text, so precomputed per-run groups are the only way it switches state without layout; the markup grows with the run count (at most 25) (implementer, from `v2-result-page` requirement 17 and §5.5 point 4, 2026-10-17).

- The layout's callers are `v2-result-page` (page, trace and both OG cards, mapping stored entries) and `v2-inspectors` (a live run's protocol band), over `v2-entries` mapping entries for `/r/`: `v2-entries`' page has no spectrum (its non-goals) and its OG card keeps the V1 layout until `v2-result-page` (X4) (implementer under the operator's autonomy mandate, 2026-10-17).
- Each spec asserts the `test_web_crate.sh` shape it leaves (Router allowed under
  `crates/web/src/entries/`, `pages/` and `api/`; axum allowed; the `netray` → `netray-web` guard
  lifted by `v2-pages` or `v2-api`, whichever lands first) and lifts only what is still in place,
  over asserting guards a parallel lane may already have lifted (implementer under the operator's autonomy mandate, 2026-10-17).
- One build order in every spec (lane A `v2-history` → `v2-admission` → `v2-entries` → `v2-api`;
  lane B `v2-spectrum` ∥ `v2-pages`, parallel to lane A; lane C `v2-inspectors`, its Phase 2 before
  `v2-result-page` Phase 1, then `v2-result-page`; then `v2-cutover`) over each spec's own reading
  of the program: every **Depends on** and Context names the same order (implementer under the operator's autonomy mandate, 2026-10-17).

## Open decisions

None.

## Out of scope

- Wiring the result page to the layout, the trace aligned under the lines, the history sentences and the replay island: `v2-result-page` (V4.5, V4.6; SDD §5.6, §8.4).
- `/r/<id>`: `v2-entries` (V2.7; SDD §4.6, §5.1), with its OG card in the V1 layout.
- The OG card rendered from the layout with resvg, including a mono font for the labels: `v2-result-page` (X4).
- Mapping the history's entries onto the layout's per-run statuses and profiles: `v2-result-page`. A live run's protocol band: `v2-inspectors`.
- Group-name placement on the axis row and the one-line IP group at the right edge: `v2-result-page`, checked at 390 px (V4.6).
- The badge: it shows the grade only (§8.3) and uses no spectrum.
