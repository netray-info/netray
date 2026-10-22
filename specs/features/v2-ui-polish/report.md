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
