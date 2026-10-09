# Report: grade integrity

## Phase 1 — Errored causes

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R1: stream without terminal event → `Err`; multi-line `data:` joined with `\n`; unparseable payload → `Err`; section Errored | green | crates/lens/src/backends/sse.rs |
| C2 | R2: dns, tls, email, ip map their producer's vocabulary explicitly; unknown → Errored; counter `lens_unknown_verdict_total{section}` + warn, also for spectra's decode failure | green | crates/lens/tests/unknown_verdicts.rs |
| C3 | one event, no `done` → `Err` | green | crates/lens/src/backends/sse.rs |
| C4 | stream with `done` → `Ok`, as today | already_implemented | crates/lens/src/backends/sse.rs |
| C5 | two `data:` lines → joined with `\n` | green | crates/lens/src/backends/sse.rs |
| C6 | non-JSON payload → `Err` | green | crates/lens/src/backends/sse.rs |
| C7 | prism, tlsight, beacon, ifconfig goldens with a verdict renamed `"passed"` → section Errored, counter +1 | green | crates/lens/tests/unknown_verdicts.rs |
| C8 | spectra golden with a status renamed → http Errored, counter +1 | green | crates/lens/tests/unknown_verdicts.rs |
| C9 | unchanged goldens → scored as today, no counter (beacon `info` included) | already_implemented | crates/lens/tests/unknown_verdicts.rs |

C4 and C9 passed at the baseline (`6e1721b`) and pin today's behaviour; the others failed there.

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| SSE collector + unknown verdicts | 2 | sonnet | 59904 | 151 |

### Review

- No BLOCKER. The reader checked every value each producer emits against lens's new maps: prism lint (`Ok`, `Warning`, `Failed`, `NotFound`), tlsight `CheckStatus` (pass, warn, fail, skip), beacon `Verdict` (skip, pass, info, warn, fail) in the summary map, category events and sub-checks, ifconfig-rs network types (internal, c2, bot, cloud, vpn, tor, spamhaus, datacenter, residential), and the terminal events (prism `done`, beacon `summary`); no successful stream ends without its terminal event; heartbeats and CRLF are handled.
- AMENDMENT | plan: `map_buckets` stays infallible because `email.rs`'s test module calls it; validation sits in `parse_summary` and also covers category events and sub-checks | affected_phase: 1 | repaired_in_phase: yes
- NIT → acted on as test strength: c5 now also proves the separator is a newline; `unknown_verdicts.rs` gains rows for a tlsight port check (`chain_trusted`) and beacon's scored summary map (`spf`). Without them, reverting the port-check or summary guard stayed green, and Phase 5 would have left the TLS row without a hostname check to rename.
- DEFERRED | the `"Skipped"` guard (`email.rs:133`) never matches beacon's `skipped`; spec non-goal, R4.2.

### Behavioural verification

skipped: no entry point changes in this phase; the backends are driven by the contract-style tests against the real goldens.

## Phase 2 — Incomplete results

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R3: incomplete when a weighted section is Errored, Timeout or Scored with `possible == 0`; NotApplicable excluded; grade `incomplete`; `complete` in the summary; `possible == 0` section status `"error"`; doc comments corrected | green | crates/lens/tests/scoring_regression.rs, crates/lens/tests/incomplete_results.rs |
| C2 | R4: one cache writer refusing incomplete; no snapshot; badge and OG `?` with the short `Cache-Control` | green | crates/lens/tests/incomplete_results.rs |
| C3 | R11: golden projection records `complete`; fixture `no-address-records` | green | crates/lens/tests/lens_golden.rs |
| C4 | email 500 → `incomplete`, `complete:false`, no snapshot id, second request MISS; `scoring_regression.rs:571` moves | green | crates/lens/tests/incomplete_results.rs, scoring_regression.rs |
| C5 | beacon answer with `skip` everywhere → `incomplete`, email status `"error"` | moved: email-scoring (R4.3) | crates/lens/tests/incomplete_results.rs |
| C6 | A/AAAA `NxDomain`, tlsight and spectra error → `incomplete`; `no-address-records` golden | green | crates/lens/tests/incomplete_results.rs, lens_golden.rs |
| C7 | badge first with email 500 → `?`, short `Cache-Control`, then `/api/check` MISS | green | crates/lens/tests/incomplete_results.rs |
| C8 | OG first with email 500 → `?`, short `Cache-Control`, then `/api/check` MISS | green | crates/lens/tests/incomplete_results.rs |
| C9 | every backend unreachable → badge `max-age=300`, as today | already_implemented | crates/lens/tests/badge_routes.rs |
| C10 | email NotApplicable, others scored → letter, complete | already_implemented | crates/lens/tests/scoring_regression.rs |
| C11 | committed lens goldens gain `complete:true`, otherwise unchanged | green | crates/lens/tests/lens_golden.rs |

C9 and C10 passed at the baseline (`b486ead`); the others failed there. C5 is dropped from this feature (below).

Test changes beyond the baseline commit, all named in the phase commit's trailers:
- `OverallScore` literals in `routes.rs` tests, `og_routes.rs`, `badge_routes.rs`, `sdd_lens_badges_p1_c11_cache_coalesce.rs`, `og_label_bounds.rs` gain `complete` (true, or false for a stubbed `error`/`incomplete` grade).
- `scoring::engine::tests::hard_fail_on_fail_verdict_forces_grade_f` used unweighted dns names (`spf`, `dmarc`), which now make the result incomplete; it uses weighted ones (`caa`, `ns`).
- `routes::tests::cache_hit_returns_x_cache_hit` and `…_in_sync_mode` ran against unreachable backends; that result is now incomplete and never cached, so the second request is a MISS. Cache HIT in SSE and sync mode is pinned by the new `complete_result_is_snapshotted_and_cached` with stubbed healthy backends.
- `incomplete_c5_email_all_skip_is_incomplete_with_error_section` is removed: it passed only because its rewrite dropped the stream's last blank line (so the collector errored); a well-formed all-`skip` beacon answer still scores email Pass, which R4.3 owns.
- `lens-no-address-records.json` generated with `UPDATE_GOLDEN=1` once the code was green.

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| incomplete results | 3 | sonnet + orchestrator (test literals, three adapted tests) | 71953 | 180 |

### Review

- BLOCKER → AMENDMENT (resolved by scope) | a well-formed beacon answer with `skip` in every category gets a complete letter with email Pass: `aggregate_bucket` starts at Pass and ranks Skip like Pass (`crates/lens/src/backends/email.rs:362,440`). The spec's C5 scenario and decision-log claim were wrong; C5 moves to the email-scoring feature (R4.2/R4.3), the spec's non-goal and decision log now say so | affected_phase: 2 | repaired_in_phase: yes (spec amended, test removed)
- AMENDMENT (fixed) | C5 passed through the Errored path because `all_skip` dropped the final blank line | affected_phase: 2 | repaired_in_phase: yes (test removed)
- AMENDMENT (fixed) | `crates/lens/README.md` scoring section (lens's SCORING SYNC RULE) now describes `Errored`, incomplete results, `complete` and `snapshot_id` | affected_phase: 2 | repaired_in_phase: yes
- AMENDMENT (fixed) | no test reached the sync-mode cache-hit branch after `cache_hit_returns_x_cache_hit_in_sync_mode` moved to MISS; the control test now checks a JSON HIT | affected_phase: 2 | repaired_in_phase: yes
- DEFERRED | with a custom scoring profile that lacks a registered section, that section reports `"error"` while the result stays complete and graded (`routes.rs:1192`, `:1479`); production uses the embedded profile with all five sections.
- Sound per the reader: only `store_result` and `get_or_compute` write the cache and both refuse incomplete; snapshots only for complete results; badge/OG `?` with `max-age=300`; moka `and_compute_with` coalesces a complete burst and never blocks other keys; no-MX stays complete; old snapshots render unchanged.

### Behavioural verification

skipped: the routes are driven in-process with stubbed backends by `crates/lens/tests/incomplete_results.rs`; no deployment-shaped entry point beyond those.

## Phase 3 — Deadlines

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R5: one `timeout_ms` per backend call over connect, send and body; email honours its `timeout_ms`; hard deadline keeps finished sections; deadline passed into the check function | green | crates/lens/tests/deadlines.rs |
| C2 | R6: config load rejects `max(wave-1) + ip ≥` hard deadline; production fixture, dev and example at 15000/2000 | green | crates/lens/src/config.rs, tests/repo/test_check_config.sh |
| C3 | 1 s deadline, 5000 ms backends, dns in 100 ms, email never → dns scored, email Timeout, incomplete, < 1.5 s | green | crates/lens/tests/deadlines.rs |
| C4 | email headers then stall, 1000 ms → Timeout ≤ 1.2 s | green | crates/lens/tests/deadlines.rs |
| C5 | tlsight headers then stalled body, 1000 ms → Timeout ≤ 1.2 s | green | crates/lens/tests/deadlines.rs |
| C6 | email `timeout_ms = 3000` → client uses 3000 ms | green | crates/lens/tests/deadlines.rs |
| C7 | `--check-config` on 20000/2000 → exit 1 naming the budget | green | tests/repo/test_check_config.sh |
| C8 | production fixture, dev, example → exit 0 | already_implemented | tests/repo/test_check_config.sh |

C8 passed at the baseline (`d55a1e1`) and guards the config edits; the others failed there.

Test changes beyond the baseline commit: `crates/lens/tests/deadlines.rs` is rustfmt-formatted, and its C5 also asserts `SectionError::Timeout`; `crates/lens/src/config.rs` gains `backend_timeout_budget_edges` (sum equal to the deadline refused, 1 ms less loads, `1e20` → `u64::MAX` refused without overflow), which failed with "attempt to add with overflow" before the fix.

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| deadlines | 3 | sonnet (fmt of the test file by the orchestrator) | 54988 | 145 |
| budget overflow (review fix) | 1 | sonnet | 14692 | 25 |

### Review

- AMENDMENT (fixed) | the budget sum used `+`: `timeout_ms = 1e20` (u64::MAX) panicked in debug and wrapped to an accepted budget in release | affected_phase: 3 | repaired_in_phase: yes (`saturating_add`, test `backend_timeout_budget_edges`)
- NIT | C4 cannot tell one budget from two (headers arrive at once); no test stalls the dns stream or the http body. Not acted on.
- NIT (acted on) | C5 now asserts Timeout.
- NIT (acted on) | budget edges (`>=`, configured email) pinned by `backend_timeout_budget_edges`.
- Sound per the reader: `run_wave` keeps finished sections and records every registered backend once; wave 2 still gets DNS's addresses; unconfigured http/email stay out of the budget; each backend has one `timeout_at` over connect, send and body (per address for ip), mapped to `SectionError::Timeout`; no new config key; no shipped config relies on the old fixed 15 s for email.

### Behavioural verification

`netray lens --check-config` (via `tests/repo/test_check_config.sh`): 20000/2000 → exit 1 naming timeouts and the hard deadline; the production fixture, `lens.dev.toml` and `lens.example.toml` → exit 0. `tests/repo/test_smoke_services.sh` starts lens with `lens.dev.toml`.

## Phase 4 — TLS reachability

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R7: tlsight `tls_reachable` Pass/Fail/Skip; lens weight 10 and hard-fail | green | crates/tlsight/tests/port_results_table.rs, crates/lens/tests/lens_golden.rs |
| C2 | R8: local connect errors → `NOT_TESTED_FROM_HERE`, not counted for `tls_reachable` | green | crates/tlsight/src/tls/mod.rs |
| C3 | R12: lens goldens `http-only` and `no-weighted-tls` from real tlsight output | green | crates/tlsight/tests/contract_golden.rs, crates/lens/tests/lens_golden.rs |
| C4 | every IP refused → `tls_reachable` Fail | green | crates/tlsight/tests/port_results_table.rs |
| C5 | IPv6 `NOT_TESTED_FROM_HERE`, IPv4 ok → Pass, verdict from IPv4 | green | crates/tlsight/tests/port_results_table.rs |
| C6 | every IP ok → Pass; the table's three rows move | green | crates/tlsight/tests/port_results_table.rs |
| C7 | only `NOT_TESTED_FROM_HERE` → Skip | green | crates/tlsight/tests/port_results_table.rs |
| C8 | ENETUNREACH/EHOSTUNREACH/EADDRNOTAVAIL → `NOT_TESTED_FROM_HERE`; refused stays `HANDSHAKE_FAILED` | green | crates/tlsight/src/tls/mod.rs |
| C9 | closed local port → `HANDSHAKE_FAILED`, as today | already_implemented | crates/tlsight/tests/port_results_table.rs |
| C10 | `tlsight-unreachable.json` → lens grade F, `hard_fail_checks` has `tls_reachable`; `http-only` golden | green | crates/lens/tests/lens_golden.rs |
| C11 | `no-weighted-tls` → incomplete, TLS `"error"` | green | crates/lens/tests/lens_golden.rs |
| C12 | healthy tlsight golden with `tls_reachable` Pass → lens goldens' TLS scores move | green | crates/lens/tests/lens_golden.rs |

C9 passed at the baseline (`3425629`); the others failed there. Goldens regenerated from the producers after the code was green: `tlsight-inspect.json` (gains `tls_reachable` Pass), `tlsight-unreachable.json` and `tlsight-not-tested.json` (new, from the real `assess_port`), `lens-healthy.json` C 71.2 → C 73.9 (TLS C → B), `lens-no-mx.json` C 65.0 → 67.8, `lens-mx-cname.json` B 76.6 → 79.4, `lens-http-only.json` F 45.3 with `tls_reachable` in `hard_fail_checks`, `lens-no-weighted-tls.json` incomplete with TLS `"error"`.

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| tlsight + lens profile + goldens | 2 | sonnet (fmt of a test file by the orchestrator) | 48684 | 148 |
| review fixes | 1 | sonnet | n/a | n/a |

### Review

- BLOCKER (fixed, second pass) | `EHOSTUNREACH` was mapped to `NOT_TESTED_FROM_HERE`; Linux raises it in SYN_SENT for an incoming ICMP host-unreachable, host-prohibited or packet-filtered (`icmp_err_convert`), so an HTTPS-less host behind firewalld's default reject would read Skip → incomplete instead of F. Now `HANDSHAKE_FAILED`; spec requirement 8 amended; SDD R5.3 lists it wrongly (AMENDMENT for the planning session) | affected_phase: 4 | repaired_in_phase: yes
- AMENDMENT (fixed) | tlsight's `ValidationSummary` showed "pass" for a port whose only check is `tls_reachable: skip`; `qualityVerdict` now needs a pass to say pass (vitest added) | affected_phase: 4 | repaired_in_phase: yes
- AMENDMENT (fixed) | lens README "Hard failures" table lacked `tls_reachable` (SCORING SYNC RULE) | affected_phase: 4 | repaired_in_phase: yes
- NIT (acted on) | `tls_reachable` had no fix text, guide link or label in lens (`fix_for`, `guide_url_for`, snapshot labels, frontend `CHECK_LABELS`/`CHECK_DESCRIPTIONS`); now covered, and the hard-coded list test includes it.
- NIT (acted on) | `site/api/lens.html` and `crates/tlsight/README.md` name the new check (the README also gains the two certificate checks it was missing; count 23).
- NIT (acted on) | a pinning test now fails if connect.rs stops boxing the `io::Error`.
- Sound per the reader: nothing reads the dropped "connection failed:" prefix; refused, timeouts and TLS alerts stay `HANDSHAKE_FAILED`; `assess_port` keeps `compute_verdict` and consistency for reachable ports; http-only → F needs both weight and hard-fail; Skip-only → incomplete as intended.

### Behavioural verification

skipped: the TLS paths are driven by `assess_port` tables, the closed-port `inspect_ip` test and the lens goldens; a live HTTPS-less target is not reachable offline.
