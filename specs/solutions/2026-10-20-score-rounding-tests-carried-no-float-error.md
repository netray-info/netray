---
class: check-passes-without-observing
repeat-of: specs/solutions/2026-10-18-page-scrolled-at-390-px-behind-green-router-tests.md
---
# The score's rounding tests passed while the page showed numbers that contradicted the letter

**What failed.** The result page showed `score N / 100` with `overall_percentage.round()`; its test fixture scored 95.71, far from any threshold, so it passed while 89.7 rendered "score 90" under a B. The review's fix, `floor()`, passed the same fixture while an exact 80 (79.99999999999999 after `scoring.rs`'s division) rendered "score 79".
**What worked.** One pure function, `views::score_number` (`crates/web/src/pages/result/views.rs`): floor after a 1e-9 guard, capped below the threshold of the grade above the letter unless a hard fail set the grade, table-tested at the edges in `crates/web/tests/it/fidelity_score_number.rs` (89.663, 79.99999999999999, 89.99999999999999, 92.4 under F).
**How to notice next time.** A number derived from a float and compared with integer thresholds needs fixtures at the thresholds and at float-error values, not one comfortable middle value.
