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

## Phase 2 — Facts, S31, DNS

### Groups

- G1: C1, C2, C3, C5–C12. It shares `engine/src/lib.rs`, `dns/src/module.rs`, `dns/src/api/check.rs`, `dns/src/testing.rs` and `dns/Cargo.toml`. It is built in five slices:
  - a: C1;
  - b: C2 and C5–C8;
  - c: the S31 goldens;
  - d: C3, C9 and C10 in `translate`;
  - e: C3, C11 and C12, the NS checks.
- G2: C4 and C13, the contract table. It changes no production file and depends on G1.

### Plan

**a. The definite negative.**
- `Facts.no_address_definite` (`crates/engine/src/lib.rs`).
- `facts_from_lookups` sets it from a helper, `definite_negative(lookups, rt)`. That helper requires every A or AAAA lookup to be an `NxDomain` with `is_definite()`.
- `crates/engine/tests/run.rs` gets the new field in its `Facts` literal (ADLC-Test-Change).

**b. The short-circuit in the engine.**
- The `Module` trait gets `needs_address_records()`, which defaults to `needs_addresses()`.
- When the facts are definite, `run` returns `NotApplicable { reason: "no A or AAAA record" }` for such modules without calling them.
- HTTP and TLS set the flag to true.
- Lens: the guard in `modules.rs` that produces `NoDnsResults` skips a definite negative.

**c. The goldens.**
- `contract_golden.rs` `nx_lookups` records NOERROR, and `prism-no-address.sse` is regenerated.
- `dns/tests/module.rs:168` expects the flag.
- The `Incomplete` stub in lens `tests/common/mod.rs` mirrors the flag for Ip, Http and Tls.
- `lens_golden_no_address_records` asserts a letter grade and N/A for ip, http and tls.
- `lens-no-address-records.json` and `lens-full-no-address-records.json` are regenerated with `UPDATE_GOLDEN=1`. Each of these changes carries an ADLC-Test-Change.

**d. `translate`.**
- New `Verdict::Unmeasured`.
- `batch_error`: a batch with no lookups, or with every lookup an `Error` or a non-definite `NxDomain`, counts as errored. Its finding names the record type, the cause and the resolvers.
- A const table maps each check to its input types: caa→CAA, cname_apex→CNAME+A, dnssec→DNSKEY+DS, dnskey_algorithm→DNSKEY, dnssec_rollover→DNSKEY+DS, https_svcb→HTTPS, ns/ns_lame/ns_delegation→NS.
- `ttl` is unmeasured only when every batch that is present errored.
- On `STREAM_TIMEOUT`, unreported IDs become unmeasured. A stream that ends normally keeps "not reported".

**e. The NS checks.**
- `check_ns_lame_delegation` and `check_ns_delegation_consistency` return `NsCheck { results, silent }`. A timeout leaves that server out and names it in `silent`. The V1 Warning stays when nobody answered.
- New `CheckEvent::Unanswered { category, silent, none_answered }`. `stream_check_events` skips it. `translate` maps `none_answered` to unmeasured, and otherwise appends the findings.
- `check_ns_outbound_tests.rs` reads `.results` (ADLC-Test-Change).
- `testing::{NsStub, ns_check_outcomes}` runs against a stub `Resolve` and `RawSend`.
- `checks.toml` drops "or reached" from `dns.ns_lame`.
- `crates/dns/Cargo.toml` gets `[[test]]` entries for `ns_timeouts` and `contract_status`, both with `required-features = ["testing"]`.

### Decisions in the plan

- `ttl` is unmeasured only when every batch errored; one REFUSED batch does not cost it the check. This follows from "measured parts count" (operator 2026-10-12, per-address decision).
- An NS host that does not resolve stays a measured Warn, because `netray_common::fetch` turns every resolver error into an empty set. AMENDMENT A4 (operator 2026-10-12).
