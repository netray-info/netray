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

## Phase 3 — TLS

### Groups

- G1: C1–C17. Every criterion lands in `translate` (`crates/tls/src/module.rs`) and its inputs.
- Already green at 1f7c778: C6, C8, C9, C11, C13, C14, C16, C17, and the worst-status half of C4.

### Plan

1. **Error codes.**
   - `crates/tls/src/tls/connect.rs`: a handshake timeout becomes `io::Error(TimedOut, "handshake timed out")`.
   - `tls/mod.rs` `error_code`: `TimedOut` maps to `HANDSHAKE_TIMEOUT`.
   - `status_of_error_code` maps `HANDSHAKE_FAILED` to Fail (Rule M), and `HANDSHAKE_TIMEOUT` or an unknown code to Unmeasured.
   - `tests/error_codes.rs` gets the timeout row and a hung-listener pin test. `tests/model_status.rs` maps `HANDSHAKE_FAILED` to Fail. Both carry an ADLC-Test-Change.
2. **Lookup errors.**
   - `routes.rs` `DnsContext.lookup_errors`, with `#[serde(default, skip_serializing_if = "Vec::is_empty")]` so the goldens stay byte-identical.
   - `tls/src/dns/mod.rs` `answered()`: true for a Response or a definite NxDomain.
   - `lookup_caa`, `has_ech_advertised` and `lookup_tlsa` return `Option`, with None meaning the lookup failed. The CAA tree climb stops at a level that did not answer.
   - `inspect` collects the names ("caa", "https", "tlsa").
   - `tests/contract_golden.rs` needs the new field in its struct literal (ADLC-Test-Change).
3. **`check_tls_reachable`.** On Fail, prefer the first `HANDSHAKE_FAILED` message.
4. **`translate`.**
   - `reachable()` replaces `check_status`:
     - V1 Pass stays Pass, with a finding for each timed-out address;
     - otherwise any Fail gives Fail;
     - otherwise any Unmeasured gives Unmeasured;
     - otherwise NotTested.
   - With no handshake, the other 20 checks follow `tls_reachable`: N/A behind a Fail, NotTested behind NotTested, Unmeasured behind Unmeasured.
   - `lookup_errors` overrides `caa_compliant`, `dane_valid`, and `ech_advertised` on port 443.
   - `consistency` and `alpn_consistency` are Unmeasured when fewer than two addresses handshook and at least one timed out.
5. **Docs.** `crates/tls/README.md` lists the codes and `lookup_errors`.

Outside `translate`: the tlsight frontend and `site/api/tls.html` need no change, and the goldens stay byte-identical. The V1 `/api/inspect` keeps its CAA "pass" on a lookup error. That is a V1 defect and out of scope.

## Phase 4 — HTTP

### Groups

- G1: C1–C13. Every criterion goes through `translate` and the new `error` fields.
- Already green at f245b40: C4, C5, C6, C10, C11 and C13.

### Plan

1. **Error types.** `assembler.rs` gets `ProbeError { kind, message }`. The kinds are timeout, refused, reset, tls, blocked and unreachable.
   - The field goes on `InspectResponse`, `HttpUpgrade` and `CorsReport`.
   - It carries `#[serde(default, skip_serializing_if = "Option::is_none")]`, so the goldens stay byte-identical.
2. **Classifying the error.** `request.rs` `probe_error(FetchError)` maps:
   - Blocked or Scheme → blocked;
   - Timeout → timeout;
   - the io source chain: ConnectionRefused → refused; ConnectionReset, Aborted, BrokenPipe or UnexpectedEof → reset; TimedOut → timeout; InvalidData → tls; NetworkUnreachable, HostUnreachable or AddrNotAvailable → unreachable;
   - anything else → reset.
3. **Wiring.** `mod.rs` `TaskResult.error` becomes `Option<ProbeError>`, and `assemble_response` copies it onto the three structs.
4. **`translate`.**
   - `https_redirect`: timeout, blocked or unreachable → Unmeasured, with a finding that names port 80. Refused, reset or tls → Fail.
   - The ten response checks: timeout, blocked or unreachable → Unmeasured, with the probe message as finding. Refused, reset or tls → N/A "no HTTPS response".
   - `cors` follows `cors.error` the same way, with N/A "no CORS probe response".
5. **OpenAPI and frontend.** The OpenAPI schemas and the spectra frontend `types.ts` gain the field, types only.
6. **Tests and docs.**
   - Protected tests that assert `error == "Redirect destination blocked"` now assert on the kind (ADLC-Test-Change). These are `outbound_redirects.rs` and `redirect_results_table.rs`, plus the in-src struct literals.
   - New `tests/probe_error_kinds.rs` drives the classifier over loopback.
   - README gets one sentence.

### Decisions in the plan

- A port-80 redirect that the fetch policy blocks is Unmeasured, per Rule M.
- A CORS probe that is refused, reset or fails TLS gives N/A, not Fail.
- A network-unreachable error on our side gives kind unreachable and so Unmeasured: the answer was not obtained (Rule M). In TLS the same case is NotTested.
- The V1 headline and the V1 `quality.checks` stay unchanged (DEFERRED). An unmeasured `hsts` still shows "HSTS ✗" in the transitional presentation.

## Phase 5 — Email, IP

### Groups

- G1: the IP half of C2/C18 and C17.
  - Add a `[[test]] contract_status` entry in `crates/ip/Cargo.toml`.
  - Add `--test contract_status` in justfile `test-rust-offline`.
- G2: the email half of C1–C18.

### Plan (G2)

1. **The seam.**
   - `DnsLookup` methods return `Result<_, LookupError { record_type, name, cause }>`. `lookup_exists` and DNSSEC stay `bool`.
   - The production resolver gets a private `answer()`: Ok for a Response or a definite NxDomain; Err for SERVFAIL, REFUSED, a timeout, a referral or any other error. It has a serde unit test.
   - `FetchResolver` falls back with `unwrap_or_default`.
   - `test_support` is public under `testing`, with the builders `fail_mx`, `fail_txt`, `fail_a`, `fail_ptr` and `fail_tlsa`.
   - `OutboundFetch.mta_sts_port` and `with_mta_sts_port`.
   - `[[test]]` entries.
2. **Vocabulary.** A sentinel sub-check, `"unmeasured"` with verdict Skip and the cause in its detail. It sits next to SKIPPED, NULL_MX and NO_MX. There is no new `Verdict` variant, so the V1 enum, OpenAPI and the frontend stay the same (P03).
3. **Checks.**
   - mx, spf, dmarc, tls_rpt, bimi, mta_sts: a top-level lookup error emits the sentinel.
   - dkim: the sentinel only when every selector errored.
   - dane and fcrdns: worst-of over the measured hosts and IPs. Errored ones become Info, and the sentinel comes only when all errored.
   - MTA-STS and BIMI fetches: only a timeout gives the sentinel.
   - dnsbl, per zone:
     - any listing → Fail;
     - at least one zone answered → `clean` Pass, plus Info naming each unanswered zone;
     - no zone answered → sentinel.
   - A policy response (127.255.255.x) counts as not answered.
4. **`translate`.**
   - Order: no-MX first (Rule M precedence), then the mx sentinel for all of `MX_DEPENDENT`, then `fold_category`.
   - `fold_category`: its own sentinel → Unmeasured. Otherwise mx with Info only → Pass. For mx and dnsbl on a Pass, the Info details become findings.
5. **Prose** (`CLAUDE.md:61` and `crates/email/CLAUDE.md`) goes in the prose commit after Phase 5.

### Decisions in the plan

- **The sentinel sub-check over a new Verdict variant.** It keeps the V1 wire enum and P03.
- **Errored sub-lookups that would read as a defect become Info**, not Warn, per Rule M. That covers an MX-host address lookup (`mx_no_addr`) and the DMARC rua/ruf authorisation (`rua_auth`).
  - Nested SPF lookups and the MTA-STS pre-flight `lookup_ips` keep `unwrap_or_default`, and are listed as DEFERRED.
- **Cross-validation.** `check_bimi_dmarc_policy` skips its rule when DMARC carries the sentinel. Otherwise an input that was never obtained produces a Warn.
