---
class: check-passes-without-observing
repeat-of: specs/solutions/2026-10-20-score-rounding-tests-carried-no-float-error.md
---
# The design gate and the style test both passed an 11 px table that board M draws at 12 px

**What failed.** The compact evidence table used `font-size: var(--font-size-label)` on the phone. `build.rs` emits `labelPhone` (11px) into the 480 px block, so the table rendered at 11 px. `polish_compact_styles.rs` checked the declaration text (`font-size:var(--font-size-label)`), and `just design-check` compared 11 px with board M's 12 px inside its 1 px typography tolerance: both green. The range review found it by rendering tokens.css and the stylesheets in Chromium.
**What worked.** A token of its own, `font.size.evidence` 13px with `evidencePhone` 12px, read by `.evidence-tabs__compact` and `.evidence-tabs__panel .evidence-tabs__compact`; the test asserts the token's two values; the design run reads 12px on M (commit a9999a37).
**How to notice next time.** A style test that names a `var(--…)` without its resolved value, or a gate row whose board and page differ by exactly the tolerance.
**Check to build.** A web test that resolves each compact/board-pinned `var(--font-size-*)` through tokens.json, phone values included, and compares it with the board's px value (crates/web/tests/it, beside polish_compact_styles.rs).
