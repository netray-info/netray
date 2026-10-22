---
class: structurally-unsatisfiable-gate
repeat-of: specs/solutions/2026-10-20-parallel-test-writers-wrote-unsatisfiable-tests.md
---
# A spectrum label rule "no leader crosses a box" had no solution on a dense run

**What failed.** v2-ui-polish requirement 9 as drafted asked that no label box in a lower row cover a higher label's leader, with greedy line-order placement. The first build placed labels by the cover relation and the property test failed; the next, right to left, passed `spectrum_leaders.rs` but `plate::tests::an_all_fail_run_has_at_most_four_label_rows` failed: on an all-fail card every box is wider than the pitch, each label must sit above its neighbour, and the band grew to dozens of rows. Boxes pushed in from the right edge made pairs that cover each other's leader, which no row order clears.
**What worked.** Right-to-left placement, clear of leaders, decided per cluster of intersecting boxes; a cluster that would need more rows than three and than its line-order placement keeps the line order (`crates/web/src/spectrum.rs` `place_cluster`), and edge-shifted boxes are exempt. SDD §5.5 point 2 and the requirement were amended in the same phase.
**How to notice next time.** A geometric "never" over every input meets a bounded resource (rows, height): write the property test over random dense input before the spec is approved.
**Check to build.** A property test over random dense label sets that asserts a bound on rows beside every new placement rule, in `crates/web/tests/it/` (the existing four-row bound lives only in `plate.rs` unit tests).
