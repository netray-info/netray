# Report: v2-ui-polish

## Phase 1 — Design gate and spectrum leaders

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 a live board without its state is one no state row; elements may carry shell; the TLS boards mark theirs; design-rules names the key | green | tests/repo/test_design_live_state.sh |
| C2 | R9 labels right to left, each above the leaders its box covers, per cluster of intersecting boxes, line order where a cluster would need more rows; edge-shifted boxes exempt; SDD 5.5 amended | green | crates/web/tests/it/spectrum_leaders.rs |
| C3 | S shell true is accepted, shell "yes" refused naming the element | green | tests/repo/test_design_live_state.sh |
| C4 | S a live board holding only shell elements gives one no state row "no board element on the page" | green | tests/repo/test_design_live_state.sh |
| C5 | S a live board whose wait was not reached gives one no state row "wait selector not reached" | green | tests/repo/test_design_live_state.sh |
| C6 | S a live board with one non-shell element reports its measured rows as before | green | tests/repo/test_design_live_state.sh |
| C7 | S a board without wait holding none of its elements reports missing on page as before | already_implemented | tests/repo/test_design_live_state.sh |
| C8 | S an earlier box covering a later leader sits above it; a box at its leader crosses a higher leader only in a cluster placed in line order | green | crates/web/tests/it/spectrum_leaders.rs |
| C9 | S existing spectrum scenarios keep every label, at least three rows, no overlap in a row | green | crates/web/tests/it/spectrum_leaders.rs |

Changed protected tests: `spectrum_scenarios.rs` c13 (four labels now stack right to left, rows 3, 2, 1, 0), `spectrum_component.rs` c6 (the row-two label is now the first DNS line), `spectrum_layout.rs` and `spectrum_catalogue_props.rs` claim (d) (a row is skipped for an overlap or to clear a covered leader), `spectrum_leaders.rs` (a box pushed in from the right edge is exempt; a dense run falls back to the line-order placement).

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 design gate | 1 | sonnet | 41235 | 41 |
| G2 spectrum leaders | 3 | orchestrator (the coder's topological order fell back on cycles; rewritten right to left with the edge exemption and the dense fallback) | 64888 | 134 |

### Behavioural verification

```
$ just design-check                 # full private manifest, live TLS boards
0 open deviation(s), 0 stale accepted entries
$ just design-check nostate.json    # TLS light board, its non-shell selectors pointed at nothing
D1: D3 · TLS inspector — light 1280/light: no state (no board element on the page)
1 open deviation(s), 0 stale accepted entries
```

### Review

| class | finding | disposition |
|---|---|---|
| AMENDMENT | the dense fallback applied to the whole spectrum, so one dense cluster reverted every label to line order | repaired in phase: the fallback is decided per cluster of intersecting boxes (requirement 9 and SDD §5.5 amended) |
| AMENDMENT | the stated reason for exempting edge-shifted boxes was false | repaired in phase: the exemption is kept and stated as what it is (leaders to the box's left; the edge rarely holds two labels) |
| AMENDMENT | the C8 scenario and criterion claimed no crossing anywhere; no decision log entry for the R9 rewrite | repaired in phase: scenario and criterion state the cluster rule; decision log entry added |

## Phase 2 — Inspector shell

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R2 the DNS builder is a card with rows Types, Job and Resolver of board G toggles and a styled select, native inputs kept | green | crates/web/tests/it/polish_dns_builder.rs |
| C2 | R3 the +check note is hidden unless the query holds +check; the dns island toggles it | green | crates/web/tests/it/polish_dns_builder.rs |
| C3 | R4 run actions at 480 px or narrower: back link on its own line, share and export buttons on one line; evidence view tools on the h1 line | green | crates/web/tests/it/polish_run_actions.rs |
| C4 | R5 a notice replacing the result region renders the idle frame with a status notice card, warn tone for refusals, and the example chips | green | crates/web/tests/it/polish_refusal_notice.rs |
| C5 | R6 the shell check list is one card holding its rows and row links | green | crates/web/tests/it/polish_check_card.rs |
| C6 | R7 at 480 px or narrower the header holds wordmark, Theme and menu only | green | crates/web/tests/it/polish_check_card.rs |
| C7 | S the idle DNS page builder holds rows Types, Job, Resolver with dns-builder__toggle labels around native inputs; the checked toggle carries the marker | green | crates/web/tests/it/polish_dns_builder.rs |
| C8 | S /inspect/dns renders the note hidden; with q holding +check it is not hidden | green | crates/web/tests/it/polish_dns_builder.rs |
| C9 | S the dns island shows the note when the field gains +check and hides it when the token leaves | green | crates/web/islands/test/dns-note.test.ts |
| C10 | S at 480 px or narrower the run actions put the back link on its own line and the buttons in one non-wrapping row | green | crates/web/tests/it/polish_run_actions.rs |
| C11 | S the evidence view's h1 and tools row share one head element | green | crates/web/tests/it/polish_run_actions.rs |
| C12 | S a stream refused with too many requests renders the hidden h1, a status notice with warn tone and unchanged text, and the example chips | green | crates/web/tests/it/polish_refusal_notice.rs |
| C13 | S an invalid input notice has no tone | green | crates/web/tests/it/polish_refusal_notice.rs |
| C14 | S a run's check list rows sit inside one inspector-checks__card with their row links | green | crates/web/tests/it/polish_check_card.rs |
| C15 | S at 480 px or narrower .page-inspector is not displayed | green | crates/web/tests/it/polish_check_card.rs |

Changed protected tests: `shell_frame.rs` (the builder root is `div.dns-builder`, no longer a fieldset), `polish_check_card.rs` (char-boundary walk; the stub carries no row links, so the link count may be 0; the first-row rule is `.inspector-checks__card > :first-child`, as the gate keeps `.check-row` in its own stylesheet).

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 builder and note | 0 | orchestrator | 0 | 0 |
| G2 run actions | 0 | orchestrator | 0 | 0 |
| G3 refusal notice | 0 | orchestrator | 0 | 0 |
| G4 check card, header | 0 | orchestrator | 0 | 0 |

The four groups share `inspector_shell.css` and `inspect.rs`, so they were built in one pass by the orchestrator rather than by parallel coders.

### Behavioural verification

```
$ node shot.mjs p2/dns 'http://127.0.0.1:18080/inspect/dns?q=example.com%20%2Bcheck' 1280,390 light,dark
p2/dns-1280-light.png status 200 scrollWidth 1280 viewport 1280
p2/dns-390-light.png status 200 scrollWidth 390 viewport 390
```
Read: builder card with Types, Job (query pressed), Resolver; the +check note shown; the check list in one card; at 390 px the header holds wordmark, Theme and menu, the back link has its own line and Copy link, Copy Markdown, CSV, JSON stand on one line.

### Review

| class | finding | disposition |
|---|---|---|
| AMENDMENT | the +check note missed field writes the shell makes without an input event (suggestion accept, clear, chips, history) | repaired in phase: the dns island re-reads the field after each click, keyup and submit in the shell and on popstate; island test added |
| AMENDMENT | an email "busy" refusal (every email slot taken) rendered without the warn tone | repaired in phase: `Refusal::Busy` carries it through the job's gate, 503 with the warn notice |
| AMENDMENT | the caller's own IP run kept the old notice | repaired in phase: same card, `role="status"` and warn tone; it keeps the caller's address as h1 (v2-ip-ui requirement 3); the example chips followed in the branch review |
| NIT | C14's link half renders no links in the stub | kept: the template puts every row link inside the card by construction; recorded |
| NIT | row links touched the card's edge at phone width | fixed: inset by `--space-4` inside the card |

## Phase 3 — Evidence view, facts grid and guides

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R8 the email evidence view at 390 px: explore links in one section named by inspector and query, row links inside the row card, swipe note under wide tables at phone width, Nothing recorded for an empty block, row cells on one baseline | green | crates/web/tests/it/polish_evidence_view.rs |
| C2 | R10 a facts cell longer than FACT_WIDE_CHARS spans the grid, decided by one pure Rust function | green | crates/web/tests/it/polish_wide_facts.rs |
| C3 | R11 a guide article reads in a 72ch column; at phone width a tool card stacks its field above a full-width submit | green | crates/web/tests/it/polish_guides.rs |
| C4 | S the stored email view lists Explore live links named DNS shop.example.com MX, TLS mx1.shop.example.com:25, DNS _dmarc.shop.example.com TXT, DNS _mta-sts.shop.example.com TXT and no bare Explore live link | green | crates/web/tests/it/polish_evidence_view.rs |
| C5 | S a check row's block links sit inside the row's card element in the evidence section | green | crates/web/tests/it/polish_evidence_view.rs |
| C6 | S a three-column table block carries a swipe note after its table; a two-column one none | green | crates/web/tests/it/polish_evidence_view.rs |
| C7 | S email.bimi with no fields reads Nothing recorded. | green | crates/web/tests/it/polish_evidence_view.rs |
| C8 | S a facts cell of 65 characters is wide, 64 is not, a chips cell whose joined names pass 64 is wide | green | crates/web/tests/it/polish_wide_facts.rs |
| C9 | S a TLS certificate grid with a long SAN list marks that cell data-wide | green | crates/web/tests/it/polish_wide_facts.rs |
| C10 | S a guide article sits in a .guide-article column with a 72ch maximum | green | crates/web/tests/it/polish_guides.rs |

Changed protected tests: `polish_evidence_view.rs` (the anchor helper read every href as empty), `polish_guides.rs` (the guide tool card's phone rules live in `query_input.css`, the component gate's owner of `.query-input__*`), `polish_wide_facts.rs` (threshold 64, relative to the constant).

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 evidence view | 0 | orchestrator | 0 | 0 |
| G2 wide facts | 0 | orchestrator | 0 | 0 |
| G3 guides | 0 | orchestrator | 0 | 0 |

### Behavioural verification

```
$ node shot.mjs p3/email http://127.0.0.1:18080/shop.example.com/email 1280,390 light,dark
p3/email-390-light.png status 200 scrollWidth 390 viewport 390
$ node shot.mjs p3/tls 'http://127.0.0.1:18080/inspect/tls?h=1.1.1.1:443' 1280,390 light,dark
p3/tls-1280-light.png status 200 scrollWidth 1280 viewport 1280
```
Read: Explore live section with four named links; row links inside the rows card; "Nothing recorded." under BIMI; swipe notes under the four-column tables at 390 px; the certificate grid's Subject, Alternative names, Issuer, SHA-256 and SCT logs span the grid.

### Review

| class | finding | disposition |
|---|---|---|
| AMENDMENT | the acceptance suite read explore hrefs from `.evidence-view__explore`, now the section | repaired in phase: `tests/acceptance/v2/inspectors.spec.ts` reads `.evidence-view__explore a` |
| AMENDMENT | the guide tool field rendered 62 px at phone width (content-box) | repaired in phase: `box-sizing: border-box` |
| NIT | the card-closing guard in `polish_evidence_view.rs` cannot fail with check rows inside | kept: the template's rows loop and links loop are one loop inside the card |

## Phase 4 — Visual reading and design gate

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R12 screenshots at 1280 and 390 px, light and dark, of the listed pages read beside the nearest board; every finding a row and closed; design-check exits 0; nothing scrolls sideways at 390 px | green | design-check, report.md visual rows |
| C2 | S design-check over the manifest exits 0 | green | design-check |
| C3 | S the screenshots read beside the nearest board leave no open finding | green | report.md visual rows |
| C4 | S every captured page at 390 px is no wider than the viewport | green | p4shots.mjs scrollWidth log |

### Design check

`just design-check` on the final tree: exit 0, 14 boards, 0 open deviations, 0 stale accepted entries (2 accepted, from v2-result-fidelity). The private manifest marks the TLS boards' shell elements (netray.info ffa6ffa); with their board elements pointed at nothing, the light TLS board reads `no state (no board element on the page)`.

### Visual reading

A reader with no part in the build read 40 captures (DNS idle, `+check`, run, refused run; email evidence view; TLS inspector; IP inspector; a guide; two result pages; 1280 and 390 px, light and dark) beside boards F, G and M. Every page measured `scrollWidth` equal to the viewport. Rows R2–R11 are the claimed fixes, N1–N22 new findings. Recaptured after the fixes: TLS at all four states (the first pass hit the TLS limiter), result, guide, email view.

| id | finding | status | disposition |
|---|---|---|---|
| R2 | builder toggles and select styled; the select keeps the native chevron | green | kept: the chevron is the platform's affordance for a select; border, font and 44 px follow the form field |
| R3 | +check note only on the +check page | green | holds |
| R4 | run actions and evidence view tools at 1280 and 390 | green | holds |
| R5 | refused run: warn notice card, chips, frame | green | holds |
| R6 | email view at 390: explore list, rows card, swipe note, Nothing recorded, aligned cells | green | holds |
| R7 | guide column and stacked tool card | green | holds |
| R8 | check list in one card | green | holds |
| R9 | leaders clear at 1280 on both result pages; at 390 DMARC's leader crosses MTA-STS | green | decision log: the phone plate (358 px, 6.3 px pitch, 10 px labels) is narrower than §5.5's placement plate (8 px pitch); proposal to place at the phone geometry |
| R10 | TLS long values span the grid | green | holds; IP card covered by `polish_wide_facts.rs` (the local IP module has no GeoIP data) |
| R11 | phone header: wordmark, Theme, menu | green | holds |
| N1 | the inspector band's zoomed plate scrolls at 390 with no hint | green | kept: the band scrolls inside its column by v2-inspector-shell's decision; proposal: a swipe note as on evidence tables |
| N2 | builder toggles do not reflect the typed query | green | not this feature: DNS-UI V11, a non-goal |
| N3 | DNS results toolbar buttons 24 px at 1280 | green | kept: v2-dns-ui's toolbar, 44 px at phone width |
| N4 | empty line between title and meta in check rows at 390 | green | kept: check_row's phone structure (board M rows) |
| N5 | record addresses 52 px apart at 390 | green | kept: 44 px tap targets (v2-inspector-shell R29) |
| N6 | "n lines need attention" over a list of every line | green | kept: the inspector lists every line of its band; the heading counts the ones that need attention |
| N7 | serial, SHA-1 and CA issuers broke mid-token in third-width cells | green | rebuild: a token longer than `FACT_TOKEN_CHARS` (32) spans the grid (requirement 10 amended) |
| N8 | protocols grid's ninth cell alone | green | kept: v2-tls-ui decision (extra cells after board G's eight) |
| N9 | three notice styles (TLS notices banner, refusal, unavailable) | green | proposal: one notice component; the TLS banner is v2-tls-ui's |
| N10 | TLS order and missing related box against board G | green | kept: shell order by v2-inspector-shell; the related box needs a host target (board G's live target is an IP) |
| N11 | feeds line and table header fonts differ between views | green | proposal: one feeds-line style across views |
| N12 | scrolling tables reach the card's right edge | green | kept: the overflow edge of a scroll container |
| N13 | swipe note under tables that fit | green | decision log: static note at phone width |
| N14 | table row heights differ | green | kept: linked cells carry 44 px tap targets |
| N15 | gap between meta line and row links at 390 | green | kept: check_row's phone structure and 44 px links |
| N16 | TLS-RPT record breaks mid-word | green | kept: field values wrap anywhere (pre-existing evidence_block rule) |
| N17 | key column width differs per card | green | kept: each card's own label column (pre-existing) |
| N18 | result page export row indented | green | rebuild: first button flush with the content edge |
| N19 | CSP row meta wraps on the result page at 390 | green | kept: result page phone rows (board M); proposal |
| N20 | dangling "· FIRST RUN" in the long-domain run title at 390 | green | proposal: run-title wrapping (v2-result-polish) |
| N21 | guide tool card: 50 px above its heading, 36 px controls at 1280 | green | rebuild: heading margin 0, 44 px field and button |
| N22 | TLS Ports row ends short of the card edge at 390 | green | proposal: port chooser phone layout (v2-tls-ui) |

### Checks to build

- CSS-reading tests: one shared rule reader in `crates/web/tests/it/` that strips comments before splitting selectors; three polish tests broke on a comment in front of a rule (`polish_check_card.rs`, `polish_guides.rs`, `polish_run_actions.rs` each carry their own parser).
- `tests/repo/test_spectrum_module.sh` `expect()`: `printf … | grep -q` under `pipefail` fails on SIGPIPE when grep exits first (one flaky `adlc verify` run on this branch); capture the output first, then grep it.
- A row bound beside every spectrum placement rule (see `specs/solutions/2026-10-22-leader-clear-labels-had-no-solution-in-a-dense-run.md`).
