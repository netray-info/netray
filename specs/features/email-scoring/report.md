# Report: email scoring

## Phase 1 — beacon

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R1: verdict order Skip < Info < Pass < Warn < Fail | green | crates/beacon/src/quality/types.rs |
| C2 | R2: a category that did not finish is sent as `skip_result` before the summary | green | crates/beacon/src/checks/mod.rs |
| C3 | R3: `sends_no_mail` computed once (Null MX or SPF only `-all`), Info in cross_validation not counted as an issue, passed to DKIM | green | crates/beacon/src/checks/cross_validation.rs, mod.rs |
| C4 | R4: revoked key Info, not usable; revoked-only Info/Warn with detail; usable + revoked Pass | green | crates/beacon/src/checks/dkim_results_table.rs, dkim.rs |
| C5 | R5: `Category::ALL`, `CROSS_VALIDATION_CHECKS`, name constants; emitted names equal the list | green | crates/beacon/src/checks/cross_validation.rs |
| C6 | R6: goldens match the check code; new null-mx, sending-no-dkim, partial, timeout goldens | green | crates/beacon/tests/contract_golden.rs, crates/beacon/src/checks/mod.rs |
| C7 | `[Pass, Info]` → Pass; `[Info]` → Info; `[Info, Warn]` → Warn; none → Pass | green | crates/beacon/src/quality/types.rs |
| C8 | grades unchanged by the order | already_implemented | crates/beacon/src/quality/types.rs |
| C9 | panicking DKIM task → `dkim` event with `skipped` before the summary | green | crates/beacon/src/checks/mod.rs |
| C10 | Null MX → `sends_no_mail` Info, ignored by detail and verdict | green | crates/beacon/src/checks/cross_validation.rs |
| C11 | `v=spf1 -all` with MX → `sends_no_mail` | green | crates/beacon/src/checks/cross_validation.rs |
| C12 | `v=spf1 include:… -all` → no `sends_no_mail` | green | crates/beacon/src/checks/cross_validation.rs |
| C13 | parked, empty `p=` → key_revoked Info, DKIM Info, found false (C6 row moves) | green | crates/beacon/src/checks/dkim_results_table.rs |
| C14 | sending, empty `p=` → Info, DKIM Warn "only revoked DKIM keys", found false (C7 row moves) | green | crates/beacon/src/checks/dkim_results_table.rs |
| C15 | sending, no key → `no_dkim` Info, unchanged | already_implemented | crates/beacon/src/checks/dkim_results_table.rs |
| C16 | valid key → `rsa_key_ok` Pass; `make_valid_rsa_p_value` parses | green | crates/beacon/src/checks/dkim.rs |
| C17 | valid + revoked → DKIM Pass | green | crates/beacon/src/checks/dkim_results_table.rs |
| C18 | parked domain with DMARC reject+rua → no DKIM finding lowers its grade | green | crates/beacon/src/checks/mod.rs |
| C19 | every cross-validation rule fires → emitted names equal `CROSS_VALIDATION_CHECKS` | green | crates/beacon/src/checks/cross_validation.rs |
| C20 | `UPDATE_GOLDEN=1` writes the four new goldens; timeout summary grade `skipped` | green | crates/beacon/tests/contract_golden.rs, crates/beacon/src/checks/mod.rs |

C8 and C15 passed at the baseline (`bbcb603`) and pin today's behaviour; the others failed there (compile errors on the new API count). Goldens: `beacon-null-mx.sse`, `beacon-sending-no-dkim.sse`, `beacon-partial.sse`, `beacon-timeout.sse` new (the timeout one from the real `run_all_checks` path); `beacon.sse` and `beacon-mx-cname.sse` gain DMARC's `no_ruf` Info; category verdicts unchanged.

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| beacon | 2 | sonnet | 74130 | 165 |
| frontend verdict order | 1 | sonnet | n/a | n/a |
| review fixes (DKIM, SPF) | 2 | sonnet | n/a | n/a |

### Review

- BLOCKER (fixed, second pass) | the revoked-only DKIM branch overwrote the computed verdict, so a `cname_loop` Fail on another selector read Warn (sending) or Info (parked) and the grade rose | affected_phase: 1 | repaired_in_phase: yes (rows C19, C20; a sender gets max(computed, Warn), a no-mail domain keeps the computed verdict)
- BLOCKER (fixed, second pass) | `only_dash_all` counted modifiers (`exp=`, `redirect=`) as mechanisms; `-ALL` was not recognised (RFC 7208 4.6.1) | affected_phase: 1 | repaired_in_phase: yes (`only_dash_all_table`, `has_dash_all_is_case_insensitive`)
- AMENDMENT (fixed) | `beacon-sending-no-dkim.sse` as first written could not come from the check code (IPv4-only MX adds `no_ipv6`; FCrDNS is per IP); the scenario now has two MX hosts with IPv4 and IPv6 and SPF covering both | affected_phase: 1 | repaired_in_phase: yes
- AMENDMENT | the existing literal scenarios (healthy, mx-cname, partial) still drift from the check code (FCrDNS one sub-check per IP, MX/SPF coverage); spec requirement 6 now says so; generating all goldens from the real pipeline is a later clean-up | affected_phase: 1 | repaired_in_phase: no (DEFERRED by spec amendment)
- DEFERRED → fixed with the SPF finding | SPF mechanism names compared case-sensitively.
- NIT (acted on) | C18 now also asserts DKIM Info in the parked run, so a broken hand-off of `sends_no_mail` fails it.
- Also: the beacon frontend's `VERDICT_ORDER` followed the server's new order (test moved).
- Sound per the reader: the verdict reorder moves only `CheckResult::new` and the frontend rollups; skip_result events go only for unfinished categories, before cross-validation and the summary; `sends_no_mail` is first and ignored by the issue count; `found` semantics as specified; the null-MX golden matches the code.

### Behavioural verification

skipped: driven by beacon's pipeline tests (`run_events` with `TestDnsResolver`) and the timeout golden from `run_all_checks`.
