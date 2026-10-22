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
| AMENDMENT | the caller's own IP run kept the old notice | repaired in phase: same card, `role="status"` and warn tone; it keeps the caller's address as h1 and has no example chips (the caller page has no input to fill) |
| NIT | C14's link half renders no links in the stub | kept: the template puts every row link inside the card by construction; recorded |
| NIT | row links touched the card's edge at phone width | fixed: inset by `--space-4` inside the card |

## Phase 3 — Evidence view, facts grid and guides

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R8 the email evidence view at 390 px: explore links in one section named by inspector and query, row links inside the row card, swipe note under wide tables at phone width, Nothing recorded for an empty block, row cells on one baseline | open | |
| C2 | R10 a facts cell longer than FACT_WIDE_CHARS spans the grid, decided by one pure Rust function | open | |
| C3 | R11 a guide article reads in a 72ch column; at phone width a tool card stacks its field above a full-width submit | open | |
| C4 | S the stored email view lists Explore live links named DNS shop.example.com MX, TLS mx1.shop.example.com:25, DNS _dmarc.shop.example.com TXT, DNS _mta-sts.shop.example.com TXT and no bare Explore live link | open | |
| C5 | S a check row's block links sit inside the row's card element in the evidence section | open | |
| C6 | S a three-column table block carries a swipe note after its table; a two-column one none | open | |
| C7 | S email.bimi with no fields reads Nothing recorded. | open | |
| C8 | S a facts cell of 41 characters is wide, 40 is not, a chips cell whose joined names pass 40 is wide | open | |
| C9 | S a TLS certificate grid with a long SAN list marks that cell data-wide | open | |
| C10 | S a guide article sits in a .guide-article column with a 72ch maximum | open | |
