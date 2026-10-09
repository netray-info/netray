# Spec: email scoring

Status: Ready for Implementation
Created: 2026-10-09

## Goal

lens's email section scores what beacon actually sends, proven by contract tests against beacon goldens whose sub-checks match what the check code emits; the hand-made `tests/email_fixtures/` are gone. A beacon timeout makes the email section a Timeout and the result incomplete. A beacon category that did not run makes the email section Errored. A Null-MX domain's infrastructure, transport and brand buckets are N/A, as for no MX. A bucket with neither a Pass, Warn or Fail category nor a routed Warn or Fail finding is Skip ("not applicable"). Every beacon category is bucketed or explicitly excluded; cross-validation findings reach the bucket that owns their topic. A revoked DKIM key is information, not a failure, and a parked domain is not marked down for it.

## Non-goals

- Changing a beacon severity other than: the verdict order (requirement 1), DKIM's `key_revoked` and the DKIM category verdict (requirement 4). beacon's grade counts only Warn and Fail (`crates/beacon/src/quality/mod.rs:25`), so requirement 1 does not move it.
- Making `reject_no_dkim` a Fail (SDD R4.4).
- The lens frontend's rendering of email (V2).
- beacon's 30 s timeout and lens's email budget (grade-integrity R3.3).

## Context and constraints

Line numbers at `2c3bb09`.

- beacon `Verdict` derives `Ord` in the order Skip, Pass, Info, Warn, Fail (`crates/beacon/src/quality/types.rs:21-27`), and a category's verdict is the max of its sub-checks, Pass when it has none (`types.rs:108-123`). So a passing category with an Info hint reads `info`: DMARC `policy_reject` Pass + `no_ruf` Info (`checks/dmarc.rs:178`), BIMI `logo_reachable` Pass + `vmc_present` Info (`checks/bimi.rs:235`), DKIM `rsa_key_ok` Pass + `key_revoked` (Info after requirement 4).
- beacon sends category events only for tasks that finished (`checks/mod.rs:374,563`); a panicked or missing task becomes `skip_result` (`:199-209`, used `:619-629`) only in the summary verdicts. The timeout summary is `Grade::Skipped`, every verdict `skip`, no category events (`:93-119`).
- beacon `Category` has 12 snake_case values (`types.rs:55-68`), no exported list; `NO_MX` is exported (`checks/mx.rs:10`); Null MX is an Info sub-check `null_mx` (`mx.rs:39-54`). cross-validation emits 12 sub-checks (`checks/cross_validation.rs:5-305`); `reject_no_dkim` Warn when `p=reject && spf -all && !dkim_found` (`:199-209`); `spf_mx_coverage` Warn with "only relevant if these MX hosts also send outbound mail" (`:123-171`); the category detail counts its sub-checks (`:48`).
- DKIM: an empty `p=` is `key_revoked` Fail and sets `found_any` (`checks/dkim.rs:119,144,151-158`); no key is `no_dkim` Info (`:213-219`); `null_mx` and `spf_has_dash_all` are known before DKIM runs but not passed (`checks/mod.rs:228,231,391,400,460-478`); `spf_has_dash_all` means a top-level `-all` exists, not "no other mechanism" (`checks/spf.rs:77,100-101`). `make_valid_rsa_p_value` (`dkim.rs:368-377`) does not parse as a key.
- beacon goldens are literal scenarios encoded through the real `SseEvent` path (`crates/beacon/tests/contract_golden.rs:75-379`); some leave out Info sub-checks the check code emits (mx-cname DMARC lacks `no_ruf`, `:294`). `dns::test_support` is `#[cfg(test)]` (`crates/beacon/src/dns/mod.rs:3-4`); `run_all_checks` has a real-timeout test (`checks/mod.rs:739-797`).
- lens email (`crates/lens/src/backends/email.rs`): guard `"Skipped"` (`:133-138`); buckets (`:311-314`), `dnssec` and `cross_validation` dropped; `aggregate_bucket` starts at Pass, Skip ranks like Pass (`:362-392`, `:439-446`); `detect_no_mx` looks only for `no_mx` (`:300-305`); legacy tests through `run_fixture` (`:484-658`).
- Pinned results that move, each with `ADLC-Test-Change` naming its requirement: `crates/beacon/src/checks/dkim_results_table.rs` (C6, C7), `dkim.rs`'s `empty_p_value_reports_key_revoked`, the beacon goldens and their scenarios, `tests/fixtures/contracts/lens-*.json`, `crates/lens/tests/contract_beacon.rs`, `crates/lens/tests/unknown_verdicts.rs:279` (brand on `beacon.sse`), `crates/lens/tests/scoring_regression.rs` email cases if their inputs change.
- The gate is `just adlc-verify`, offline. P26: a fact is computed by one module and referenced elsewhere.

## Requirements

1. **Verdict order (beacon).** beacon orders verdicts Skip < Info < Pass < Warn < Fail. A category with a Pass sub-check and Info hints is Pass; a category with only Info sub-checks is Info.
2. **Every category is sent (beacon).** For a category whose task did not finish, beacon sends the `skip_result` category event (one sub-check `skipped`) before the summary.
3. **Sends no mail (beacon).** beacon computes once whether a domain declares it sends no mail: Null MX, or an SPF record whose only mechanism is `-all`. When it holds, beacon emits an Info sub-check `sends_no_mail` in `cross_validation` (it does not count as an issue in the category's detail or verdict) and passes the fact to DKIM.
4. **Revoked DKIM (R4.5).** An empty `p=` is `key_revoked` Info and does not count as a usable key (`dkim_found` stays false unless a usable key exists). When beacon found only revoked keys, DKIM is Info for a domain that sends no mail and Warn otherwise, with category detail "only revoked DKIM keys". When no key was found, DKIM stays `no_dkim` Info. A usable key alongside a revoked one gives DKIM Pass.
5. **beacon exports.** beacon exports `Category::ALL`, the cross-validation sub-check names it can emit (`CROSS_VALIDATION_CHECKS`), and constants for `null_mx`, `skipped` and `sends_no_mail`. A beacon test drives `cross_validate` so that every rule fires and asserts each emitted name is in `CROSS_VALIDATION_CHECKS`, and each listed name is emitted.
6. **beacon goldens (R4.1).** The new scenarios carry the sub-checks the check code emits for their records; the existing three gain the `no_ruf` hint they lacked (amended in Phase 1: their FCrDNS per-IP sub-checks and MX/SPF coverage still drift from the check code, a DEFERRED clean-up). beacon's contract test writes `beacon-null-mx.sse` (Null MX, `v=spf1 -all`, `v=DMARC1; p=reject; rua=mailto:d@example.com`, no DKIM key), `beacon-sending-no-dkim.sse` (normal MX, `v=spf1 ip4:192.0.2.0/24 -all`, DMARC `p=reject` with rua, no DKIM key: the only non-pass is `reject_no_dkim` Warn) and `beacon-partial.sse` (one category from `skip_result`). A `#[cfg(test)]` test in the beacon crate writes `beacon-timeout.sse` from the real `run_all_checks` timeout path, regenerated with `UPDATE_GOLDEN=1 cargo test -p beacon --lib timeout_golden` (named in the contract test's header next to the existing command).
7. **No hand-made fixtures (R4.1).** `crates/lens/tests/email_fixtures/` and `run_fixture` are deleted; every lens email test drives the real `EmailBackend` against a golden in `tests/fixtures/contracts/`. `crates/lens/tests/contract_beacon.rs` has one test per beacon golden.
8. **Timeout (R4.2).** lens's guard matches beacon's wire value `skipped`, taken from beacon's export. A beacon timeout makes the email section `SectionError::Timeout`, so the result is incomplete; never NotApplicable.
9. **N/A, neutral and not-run (R4.3).** `null_mx` sets the same three N/A buckets as `no_mx`. `info` maps explicitly to neutral. A bucket's verdict is the worst of its categories' verdicts and the routed findings of requirement 10; a bucket with none of Pass, Warn or Fail is Skip with reason "not applicable". A category whose sub-check is `skipped`, or a summary verdict without a category event, makes the email section Errored. Authentication stays scored.
10. **Every category accounted for (R4.4, SC5).** `dnssec` is on an explicit exclusion list ("scored in DNS section"). `cross_validation` is not scored as a category; its sub-checks are routed by name: authentication `null_mx_spf`, `reject_no_dkim`, `dmarc_rua_auth`, `dmarc_sp_gap`, `spf_mx_coverage`, `sends_no_mail`; infrastructure `fcrdns_mismatch`; transport `mta_sts_*`, `dane_*`; brand `bimi_dmarc_policy`. Routing keeps beacon's verdicts. An unrouted name makes the email section Errored and increments `lens_unknown_verdict_total{section="email"}`. When `sends_no_mail` is present, `reject_no_dkim` and `spf_mx_coverage` are excluded.

## Phase 1 — beacon

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 5, 6

`crates/beacon/src/quality/types.rs`, `crates/beacon/src/checks/{mod,spf,dkim,cross_validation,mx}.rs`, `crates/beacon/tests/contract_golden.rs`, the beacon goldens.

### Test Scenarios

- GIVEN sub-checks `[Pass, Info]` WHEN a category is built THEN its verdict is Pass; `[Info]` → Info; `[Info, Warn]` → Warn; none → Pass.
- GIVEN a scored domain WHEN beacon grades it THEN the grade equals today's (requirement 1 moves no grade).
- GIVEN a DKIM task that panics WHEN beacon runs THEN a `dkim` category event with sub-check `skipped` is sent before the summary.
- GIVEN Null MX WHEN beacon checks the domain THEN cross_validation carries `sends_no_mail` Info, and its detail and verdict ignore it.
- GIVEN SPF `v=spf1 -all` and a normal MX WHEN checked THEN `sends_no_mail` Info.
- GIVEN SPF `v=spf1 include:_spf.example.com -all` WHEN checked THEN no `sends_no_mail`.
- GIVEN a parked domain (Null MX, `SPF -all`) whose only key has an empty `p=` WHEN DKIM runs THEN `key_revoked` Info, DKIM Info, `found` false (DKIM results table row C6 moves).
- GIVEN a sending domain whose only key has an empty `p=` WHEN DKIM runs THEN `key_revoked` Info, DKIM Warn, detail "only revoked DKIM keys", `found` false (row C7 moves).
- GIVEN a sending domain with no key WHEN DKIM runs THEN `no_dkim` Info, unchanged (row C8).
- GIVEN a valid key WHEN DKIM runs THEN `rsa_key_ok` Pass, unchanged (row C9); `make_valid_rsa_p_value` yields a key that parses.
- GIVEN a valid key and a revoked one WHEN DKIM runs THEN DKIM Pass.
- GIVEN the parked domain with `v=DMARC1; p=reject; rua=mailto:d@example.com` WHEN beacon grades it THEN no DKIM finding lowers it below today's grade for the same records minus the revoked key.
- GIVEN `cross_validate` inputs that fire every rule WHEN run THEN the emitted names equal `CROSS_VALIDATION_CHECKS`.
- GIVEN `UPDATE_GOLDEN=1` WHEN the contract test and the timeout golden test run THEN they write `beacon-null-mx.sse`, `beacon-sending-no-dkim.sse`, `beacon-partial.sse` and `beacon-timeout.sse` (summary grade `skipped`).

## Phase 2 — lens

**Depends on:** Phase 1
**Requirements:** 7, 8, 9, 10

`crates/lens/src/backends/email.rs`, `crates/lens/tests/contract_beacon.rs`, `crates/lens/tests/lens_golden.rs`, the lens goldens.

### Test Scenarios

- GIVEN the tree WHEN searched THEN `rg email_fixtures crates/*/src crates/*/tests` finds nothing.
- GIVEN `beacon-timeout.sse` WHEN lens parses it THEN `SectionError::Timeout`; a route check gives `grade:"incomplete"`.
- GIVEN `beacon-null-mx.sse` WHEN lens parses it THEN infrastructure, transport and brand are Skip with `bucket_na` entries; authentication is Pass (`reject_no_dkim` excluded through `sends_no_mail`).
- GIVEN `beacon-no-mx.sse` WHEN lens parses it THEN the same three buckets are N/A, as today.
- GIVEN `beacon.sse` (BIMI `absent` Info only) WHEN lens parses it THEN brand is Skip "not applicable" (`unknown_verdicts.rs:279` and the lens goldens move, `ADLC-Test-Change` naming requirement 9).
- GIVEN a brand bucket whose BIMI is Info only and a routed `bimi_dmarc_policy` Warn WHEN lens parses it THEN brand is Warn.
- GIVEN `beacon-partial.sse` WHEN lens parses it THEN email is Errored and the result incomplete.
- GIVEN a summary verdict without a category event WHEN lens parses it THEN email is Errored.
- GIVEN `beacon-sending-no-dkim.sse` WHEN lens parses it THEN authentication is Warn with `reject_no_dkim`'s detail among its messages.
- GIVEN beacon's `Category::ALL` and `CROSS_VALIDATION_CHECKS` WHEN the coverage tests run THEN every category is bucketed or excluded and every name is routed.
- GIVEN a golden with a cross-validation name not in the routing table WHEN lens parses it THEN email is Errored and the counter increments.

## Decision log

- beacon computes `sends_no_mail` once and emits it as an Info cross-validation sub-check that lens reads, over lens deriving it (P26) (operator, 2026-10-09).
- An unrouted cross-validation sub-check makes email Errored and is counted, over a counter alone (operator, 2026-10-09); beacon's own test keeps `CROSS_VALIDATION_CHECKS` equal to what `cross_validate` emits (independent reading).
- `beacon-timeout.sse` comes from the real `run_all_checks` timeout path in a beacon lib test with its own regeneration command (operator, 2026-10-09; independent reading).
- beacon orders Info below Pass, over lens re-deriving category verdicts from sub-checks: a passing category with a hint is Pass at the source, and beacon's grade does not move (operator, 2026-10-09).
- beacon sends the `skip_result` event for a category that did not run, and lens also treats a summary verdict without a category event as not run, over lens alone (operator, 2026-10-09).
- `spf_mx_coverage` is excluded with `reject_no_dkim` for a domain that sends no mail: its own detail says it matters only when the MX hosts send (operator, 2026-10-09; AMENDMENT to SDD R4.4).
- A revoked-only key is not a usable key (`dkim_found` false), so `reject_no_dkim` can fire for a sending domain with `p=reject` and `-all` (independent reading).
- One feature for SDD Phase 4, beacon first.

## Open decisions

None.

## Out of scope

- beacon's other severities and its frontend.
- The lens frontend's email card (V2).
