# Report: lens result goldens and backend results tables

## Phase 1 — lens result goldens

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R1: stubs serve the committed backend goldens; lens with `lens.production.toml` settings runs `POST /api/check` in-process; projection compared with `tests/fixtures/contracts/lens-<fixture>.json`, rewritten under `UPDATE_GOLDEN=1` | already_implemented | crates/lens/tests/lens_golden.rs |
| C2 | R2: projection = summary fields + per-section status, grade, checks as name+verdict; no messages, durations, cache flag, snapshot id; sorted maps, byte-stable | already_implemented | crates/lens/tests/lens_golden.rs |
| C3 | R3: fixtures `healthy`, `no-mx`, `mx-cname`, each one table entry | already_implemented | crates/lens/tests/lens_golden.rs |
| C4 | R8: no production change beyond a behaviour-neutral exposure | already_implemented | — |
| C5 | `healthy` → projection equals `lens-healthy.json` | already_implemented | crates/lens/tests/lens_golden.rs |
| C6 | `no-mx` → equals `lens-no-mx.json` (amended: today the mail buckets are `skip`, `not_applicable` is empty) | already_implemented | crates/lens/tests/lens_golden.rs |
| C7 | `mx-cname` → equals `lens-mx-cname.json` | already_implemented | crates/lens/tests/lens_golden.rs |
| C8 | two runs write byte-identical files | already_implemented | crates/lens/tests/lens_golden.rs |
| C9 | `UPDATE_GOLDEN` unset and a differing golden → fails and prints the differing fields | already_implemented | crates/lens/tests/lens_golden.rs |

Every test is a pinning test and passed at its baseline commit (`ef55064`, baseline `ca9716a`); C4 holds because the phase changed no production file. Goldens: `lens-healthy.json` C (71.2), `lens-no-mx.json` C (65.0), `lens-mx-cname.json` B (76.6); no section errors in any fixture; two runs are byte-identical.

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| — | 0 | — | 0 | 0 |

No coder ran: nothing was red. The test writer took 49678 tokens, 191 s.

### Review

- AMENDMENT | spec Phase 1 scenario 2 said `not_applicable` names the no-MX mail buckets; today lens marks `email_infrastructure`, `email_transport` and `email_brand_policy` `skip` and leaves `not_applicable` empty. The golden pins today's state; the test asserts a `skip` bucket instead | affected_phase: 1 | repaired_in_phase: yes (scenario reworded). Whether no-MX should become N/A is R4.3's to decide, which moves this row with `ADLC-Test-Change`.
- AMENDMENT | requirement 2 recorded `not_applicable` as a map; the projection keeps the section names only, the reason being prose like messages | affected_phase: 1 | repaired_in_phase: yes
- DEFERRED | `lens-mx-cname.json`: the email section event carries grade F (beacon's own grade, passed through) while `summary.section_grades.email` is B (lens's bucket score). Two grades for one section is today's behaviour, now pinned; the V2 status-word mapping (V1.1) should name which one is the section grade.
- DEFERRED | `cargo clippy -p lens --tests -- -D warnings` fails on three existing errors in `crates/lens/tests/og_routes.rs`; the gate runs clippy without `--tests`, so it is green. Not touched here.

### Behavioural verification

skipped: no entry point changes; `POST /api/check` is driven in-process by the test itself.

## Phase 2 — Backend results tables

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R4: beacon DKIM table drives the real `check_dkim` with a stub resolver; records category verdict, sub-checks, detail | already_implemented | crates/beacon/src/checks/dkim_results_table.rs |
| C2 | R5: tlsight port table drives the real `assess_port`; second table records the per-IP error code against a closed local port | already_implemented | crates/tlsight/tests/port_results_table.rs |
| C3 | R6: prism lint table drives mhost's `check_dnssec`/`check_ttl` over recorded `Lookups`, plus prism's `+check` lint assembly | already_implemented | crates/mhost-prism/tests/lint_results_table.rs |
| C4 | R7: target policy table over `is_allowed_target` | already_implemented | crates/common/tests/target_policy_results_table.rs |
| C5 | R8: no production change beyond a behaviour-neutral exposure | already_implemented | — |
| C6 | parked domain, only key empty `p=` → today's `key_revoked` and category verdict | already_implemented | crates/beacon/src/checks/dkim_results_table.rs |
| C7 | sending domain, only key empty `p=` → today's result and detail | already_implemented | crates/beacon/src/checks/dkim_results_table.rs |
| C8 | sending domain, no key at any probed selector → today's `no_dkim` | already_implemented | crates/beacon/src/checks/dkim_results_table.rs |
| C9 | sending domain, one valid key → today's pass | already_implemented | crates/beacon/src/checks/dkim_results_table.rs |
| C10 | every IP failed target-side → today's port verdict and checks | already_implemented | crates/tlsight/tests/port_results_table.rs |
| C11 | IPv6 failed, IPv4 succeeded → today's verdict and checks | already_implemented | crates/tlsight/tests/port_results_table.rs |
| C12 | every IP succeeded → today's verdict and checks | already_implemented | crates/tlsight/tests/port_results_table.rs |
| C13 | closed local port → today's per-IP error code | already_implemented | crates/tlsight/tests/port_results_table.rs |
| C14 | signed zone without RRSIGs → today's `check_dnssec` results | already_implemented | crates/mhost-prism/tests/lint_results_table.rs |
| C15 | one record set with differing TTLs → today's `check_ttl` results | already_implemented | crates/mhost-prism/tests/lint_results_table.rs |
| C16 | same lint line from two resolvers → today's `+check` lint lines | already_implemented | crates/mhost-prism/tests/lint_results_table.rs |
| C17 | each named range, a public 6to4 address, three public addresses → today's `is_allowed_target` answer | already_implemented | crates/common/tests/target_policy_results_table.rs |

Every table is a pinning test and passed at its baseline commit (`5c57d30`, baseline `49ffc9f`); C5 holds because the phase changed no production file (the only non-test line registers `#[cfg(test)] mod dkim_results_table`). Recorded today:
- DKIM: an empty `p=` gives `key_revoked` **Fail** with detail "DKIM key(s) found" for parked and sending domains alike; no key gives `no_dkim` Info; a valid 2048-bit key gives `rsa_key_ok` Pass. `check_dkim` probes only `default._domainkey` in these rows.
- Port quality: every IP failed → Skip, no checks; IPv6 failed and IPv4 ok → Warn, the failed address appears in no check, and only `alpn_consistency` (Skip vs Pass) differs from all-ok. A closed local port → `HANDSHAKE_FAILED`.
- Lints (mhost 0.11.3): a signed zone without RRSIGs warns "DNSKEY present but no RRSIG records found"; a 30 s record warns about low TTL, differing TTLs alone do not warn; two resolvers answering the same zone double the KSK/ZSK count ("Found 2 KSK(s) and 2 ZSK(s)") and list the low-TTL record twice inside one warning.
- Target policy: `0.1.2.3`, `240.0.0.1`, `198.18.0.1`, `192.0.0.8`, `2002:808:808::1`, `8.8.8.8`, `1.1.1.1`, `2606:4700::` are all allowed.

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| — | 0 | — | 0 | 0 |

No coder ran: nothing was red. Test writers: DKIM 41154 tokens/135 s, port 34138/144 s, lints 50226/171 s, target policy 18057/109 s.

### Review

- AMENDMENT | requirement 6 said the lint table builds `Lookups` in the test; mhost 0.11.3's `new_for_test` constructors are `#[cfg(test)]` and `Record` fields are private, so the table builds `Lookups` through mhost's own serde form. Scenario C16 "as prism's `+check` output carries it": prism hands `all_lookups` across every resolver straight to the lints (`crates/mhost-prism/src/api/check.rs:479,488`), so the two-nameserver `Lookups` is that input | affected_phase: 2 | repaired_in_phase: yes. Phase 2b (mhost 0.12) may change the serde form; the table then moves with `ADLC-Test-Change` naming R2.6.
- DEFERRED | the lint duplicates are wider than "duplicate lint lines" (SDD R5.1): over two resolvers the DNSSEC lint double-counts keys, and the TTL lint repeats a record inside one line. R5.1 should pin both rows.
- DEFERRED | `make_valid_rsa_p_value` in `crates/beacon/src/checks/dkim.rs`'s tests does not parse as a key (it yields `key_parse_error`); its tests assert only `found`. The table uses a generated key instead. Worth fixing with R4.5, which touches those tests.
- DEFERRED | the port table's Warn comes from the fixture (an OCSP URL, nothing stapled); C13 runs over IPv4 only.

### Behavioural verification

skipped: no entry point changes; the tables drive the check functions directly.
