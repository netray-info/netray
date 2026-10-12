# Plan: v2-unmeasured

## Phase 1 — Engine, lens, frontend

### Groups

- G1: C1, C4 (C5, C6 protect)
- G2: C2, C7 (C8, C9 protect); needs G1
- G3: C3, C10 (C11 protects)

### Plan

**G1.** In `crates/engine/src/scoring.rs`, `score_section` returns `None` when a weighted check is `Unmeasured`. `None` already makes the run incomplete and drops the section from `Score::sections`. Update the doc comments. Then drop the row `(Pass, Unmeasured)` from `crates/engine/tests/score.rs` `per_check_status_rules` (ADLC-Test-Change).
`cargo test -p netray-engine --test unmeasured_scoring --test score`

**G2.**
- `crates/lens/src/score.rs`: add `CheckVerdict::Unmeasured`, which maps to `Status::Unmeasured`.
- `modules.rs` `verdict()`: give `Unmeasured` its own arm.
- `routes.rs`:
  - `verdict_str` returns "unmeasured";
  - `section_status_from_checks` excludes it from possible;
  - `snapshot_section_results` grades an Ok section that is missing from `score.sections` "incomplete" instead of "error".
- `snapshot/types.rs`: earned/possible is (0, 0), and `verdict_str` returns "unmeasured".
- Exhaustive mappings, each with its ADLC-Test-Change: `crates/lens/tests/model_status.rs` and `crates/tls/tests/module.rs` `verdict_of`.
- `site/api/lens.html`: add a table of check verdict values.

`cargo test -p lens --test unmeasured_verdict --test model_status --test lens_golden --lib && cargo test -p netray-tls --test module`

**G3.**
- `types.ts`: add `'unmeasured'` to `Verdict`.
- `CheckList.tsx`: an icon and colour for unmeasured, the label "could not check", and no points.
- `Summary.tsx`: `hasUnmeasuredCheck`. The incomplete note shows in the `isError()` branch and in the letter branch, but only when there are unmeasured checks: the badge test with grade incomplete and no checks must stay free of the note.

`cd crates/lens/frontend && npx vitest run src/components/Unmeasured.test.tsx src/components/sdd_lens-badges_p2_c4_summary-badge-btn.test.tsx src/components/CheckList.test.tsx src/components/CheckList.points.test.tsx && npx tsc --noEmit`
