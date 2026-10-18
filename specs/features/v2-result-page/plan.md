# Plan: v2-result-page

## Phase 1 — Page views from an entry

### Groups

G1: C1–C18 (one production file: `crates/web/src/pages/result/views.rs`; `pages/mod.rs` and `pages/result/mod.rs` only declare modules).

### Plan

G1:
1. `crates/web/src/pages/mod.rs`: `pub mod result;`.
2. `crates/web/src/pages/result/mod.rs`: `pub mod views;`.
3. `crates/web/src/pages/result/views.rs`: helpers (owner word/label/rank, severity word/rank, protocol labels, block module → Protocol); `evidence_href` over `evidence::href`; `issues` (warn/fail only, fixed group order, inside a group severity → fail before warn → catalogue index; body findings joined by " " else catalogue finding); `passed_checks` (pass, not_applicable with section reason, not_tested, unmeasured "could not check"; catalogue order; unknown ids after); `change_label`; `GradeStep`, `DiffView`, `FIRST_STATE`, `diff_view` (checks then blocks, grade steps, " → " summary, engine note); `recorded_diff_view`; `history_sentence`, `issue_since` (UTC MM-DD, " · "); `layout_input` over `Profiles::get`.

Decision: a warn or fail whose id is not in the catalogue (removed by a release) is left out of the issues, which need catalogue texts, and listed after the catalogue checks in the passed-checks details, over a fallback owner group: requirement 7's texts all come from the catalogue (implementer under the operator's autonomy mandate, 2026-10-18).
