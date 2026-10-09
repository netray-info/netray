---
class: unverified-claim-treated-as-fact
repeat-of: specs/solutions/2026-10-09-one-line-bump-estimate.md
---
# Release notes compiled from feature reports state what the code does not do

**What failed.** The 0.23.0 changelog draft was written from the landed features' `report.md` files. The review of `71f239c..e314332` found two entries the code contradicts: truncated streams are not counted in `lens_unknown_verdict_total` (only `unknown_verdict()` increments it, `crates/lens/src/backends/mod.rs:90`), and a domain with no A or AAAA record gets an errored IP section, not N/A (`crates/lens/src/backends/ip.rs:85`).
**What worked.** A reader asked only "does any entry state something the code does not do", checking every claim against `crates/`; both entries corrected in `2639f8a`, re-read clean.
**How to notice next time.** A changelog entry that joins two behaviours with "and" in one clause, or that generalises a guarded case ("no public address"), was written from a summary, not from the code.
