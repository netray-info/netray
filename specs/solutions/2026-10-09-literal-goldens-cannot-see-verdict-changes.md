---
class: check-passes-without-observing
repeat-of: specs/solutions/2026-10-08-golden-with-invented-values-hid-parser-bugs.md
---
# Literal contract goldens cannot detect a verdict change

**What failed.** The security-correctness SDD made "beacon's contract goldens are byte-identical before and after" its proof that a patch changes no verdict. beacon's `contract_golden.rs` builds every result from literals, and BIMI appears only as `absent`, so a change from Warn to Fail in `check_bimi` left every golden unchanged (SDD review @571a061, M1).
**What worked.** Before changing any check code, commit a results table that drives the real `check_bimi`, `check_mta_sts` and spectra's `execute_request` against a stub resolver and local listeners. It must be green against today's code (`crates/beacon/src/checks/{bimi,mta_sts}_results_table.rs`, `crates/spectra/tests/redirect_results_table.rs`). Every later row change then needs an `ADLC-Test-Change` naming its reason. That table caught the dead-CDN BIMI regression in the branch review.
**How to notice next time.** A "no behaviour change" proof that never calls the code it protects is not a proof.
