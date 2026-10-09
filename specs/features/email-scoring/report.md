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

## Phase 2 — lens

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R7: no `email_fixtures`, no `run_fixture`; one contract test per beacon golden | green | crates/lens/tests/contract_beacon.rs |
| C2 | R8: guard matches `skipped` from beacon's export; timeout → `SectionError::Timeout` | green | crates/lens/tests/contract_beacon.rs |
| C3 | R9: `null_mx` N/A; `info` neutral; bucket = worst of categories and routed findings; none → Skip "not applicable"; `skipped` or verdict without event → Errored | green | crates/lens/tests/contract_beacon.rs |
| C4 | R10: `dnssec` excluded; cross_validation routed; unrouted → Errored + counter; no-mail excludes `reject_no_dkim` and `spf_mx_coverage` | green | crates/lens/tests/contract_beacon.rs |
| C5 | `rg email_fixtures crates/*/src crates/*/tests` finds nothing | already_implemented | tests/repo or crates/lens/tests/contract_beacon.rs |
| C6 | `beacon-timeout.sse` → Timeout; route → `incomplete` | green | crates/lens/tests/contract_beacon.rs, crates/lens/tests/incomplete_results.rs |
| C7 | `beacon-null-mx.sse` → three N/A buckets; authentication Pass | green | crates/lens/tests/contract_beacon.rs |
| C8 | `beacon-no-mx.sse` → three N/A buckets, as today | already_implemented | crates/lens/tests/contract_beacon.rs |
| C9 | `beacon.sse` → brand Skip "not applicable" (goldens move) | green | crates/lens/tests/contract_beacon.rs, unknown_verdicts.rs, lens goldens |
| C10 | BIMI Info only + routed `bimi_dmarc_policy` Warn → brand Warn | green | crates/lens/tests/contract_beacon.rs |
| C11 | `beacon-partial.sse` → Errored, incomplete | green | crates/lens/tests/contract_beacon.rs |
| C12 | summary verdict without category event → Errored | green | crates/lens/tests/contract_beacon.rs |
| C13 | `beacon-sending-no-dkim.sse` → authentication Warn with `reject_no_dkim`'s detail | green | crates/lens/tests/contract_beacon.rs |
| C14 | every `Category::ALL` bucketed/excluded; every `CROSS_VALIDATION_CHECKS` routed | green | crates/lens/tests/contract_beacon.rs |
| C15 | unrouted cross-validation name → Errored, counter +1 | green | crates/lens/tests/contract_beacon.rs |

C5 (the deletion check) and C8 (no-MX N/A) passed at the baseline (`7cc58cb`); the others failed there. lens takes beacon's names into its own constants, pinned against beacon's exports by `contract_beacon.rs` through a dev-dependency (spec requirement 8 amended).

Lens goldens moved (`ADLC-Test-Change`, requirement 9): `email_brand_policy` pass → skip in every fixture using `beacon.sse` or `beacon-mx-cname.sse`; healthy C 73.9 → C 73.1 (email section D → F: 9/22 → 7/20), mx-cname B 79.4 → B 77.8 (email 17/22 → 10/15), http-only F 45.3 → 44.4, no-address-records and no-weighted-tls stay incomplete (score informational); no-mx unchanged; null-mx new, B 82.8, email A+ on authentication.

Test changes beyond the baseline: `lens_does_not_treat_mx_cname_failure_as_no_mx` asserted transport and brand are not N/A on mx-cname, where both are all-Info; under requirement 9 they are "not applicable", so it now asserts only that no bucket is N/A for "no MX records". Two pinning tests added from the reader (below).

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| lens email | 3 (parts run separately) | sonnet + orchestrator (stale test) | 56802 | 193 |

### Review

- AMENDMENT (fixed) | no test could fail if lens stopped treating `null_mx` as no MX: all-Info buckets were N/A anyway. Added `null_mx_keeps_transport_na_when_a_receiving_check_fails` | affected_phase: 2 | repaired_in_phase: yes
- NIT (acted on) | the `spf_mx_coverage` exclusion had no test; added `sends_no_mail_excludes_spf_mx_coverage`.
- NIT | a Null-MX domain's N/A buckets carry the message "No MX records — email receiving not configured" (its `bucket_na` reason says "null MX"). Not acted on.
- NIT | beacon's timeout and the not-run paths leave no warn-level log naming the category. Not acted on.
- Sound per the reader: summary keys and category events are the same 12 names and every event precedes the summary; no configured transport, infrastructure or authentication setup falls to N/A; all 13 cross-validation names route as specified; email no longer produces `SectionError::NotApplicable`; README matches the code; the golden moves follow the rules.

### Behavioural verification

skipped: driven by `contract_beacon.rs` against beacon's goldens and `incomplete_results.rs` through the routes.
