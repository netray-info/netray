---
class: verdict-broader-than-the-check
repeat-of: specs/solutions/2026-10-08-check-config-verdict-broader-than-validate.md
---
# beacon's verdict order made a passing category read "info"

**What failed.** beacon's `Verdict` derived `Ord` as Skip, Pass, Info, Warn, Fail, and a category's verdict is the max of its sub-checks. A configured, passing category with an Info hint (DMARC `policy_reject` + `no_ruf`, BIMI `logo_reachable` + `vmc_present`) therefore read `info`. SDD R4.3's rule "a bucket with only Info is not applicable" would have dropped such setups out of lens's grade; the spec's independent reading found it before code.
**What worked.** beacon orders Skip < Info < Pass < Warn < Fail (`crates/email/src/quality/types.rs`); a passing category with hints is Pass at the source, only an Info-only category is Info. beacon's grade counts only Warn and Fail, so it did not move; the beacon frontend's `VERDICT_ORDER` followed.
**How to notice next time.** A rollup by `max` over an enum whose derived order puts an advisory value above success.
