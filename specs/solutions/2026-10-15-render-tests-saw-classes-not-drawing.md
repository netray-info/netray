---
class: check-passes-without-observing
repeat-of: specs/solutions/2026-10-10-goldens-green-production-regressed.md
---
# Component render tests passed while the spectrum drew wrongly

**What failed.** Phase 3 of v2-design-system tested each component by rendering a fixed view and asserting its root class and the view's values (`crates/web/tests/components.rs`). All passed while the spectrum drew the fixed line as a solid bar without its ring, dropped the group boundaries and put labels inside the plate, and the trace drew a pass as a full block: 13 drawing defects, found only when the reader rendered the components in headless Chromium.
**What worked.** The SDD's drawing rules (§5.4, §5.6) joined requirement 7 (c40911a), with markup tests that resolve each line's width token, the hollow fixed line and its ring, the boundaries and the label rows (`crates/web/tests/drawing_rules.rs`, 4f1f58c). One of them caught a defect the render review had missed: a doubled hairline at every shared group edge (fixed in 5c53c7f).
**How to notice next time.** A component test that would still pass with the component's stylesheet deleted asserts presence, not drawing.
