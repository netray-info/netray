# Report: one outbound fetch policy

## Phase 1 — Results table

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R7: a results table drives the real `check_bimi`, `check_mta_sts` and spectra's redirect follower against a stub resolver and local listeners, recording sub-check, verdict and grade (beacon) or final status, limit flag, hops and verdicts (spectra); green against today's code | green | crates/beacon/src/checks/{bimi,mta_sts}_results_table.rs, crates/spectra/tests/redirect_results_table.rs |
| C2 | BIMI logo host does not answer → `logo_unreachable` Warn | green | crates/beacon/src/checks/bimi_results_table.rs |
| C3 | BIMI logo host NXDOMAIN → today's sub-check, verdict, grade | green | crates/beacon/src/checks/bimi_results_table.rs |
| C4 | BIMI logo host resolves to `10.0.0.1` → `logo_ssrf_blocked` Fail | green | crates/beacon/src/checks/bimi_results_table.rs |
| C5 | BIMI final redirect target resolves to `10.0.0.1`, answering 200 and answering 404 → today's result | green | crates/beacon/src/checks/bimi_results_table.rs |
| C6 | BIMI intermediate hop resolves to `10.0.0.1`, chain ends in public 200 → today's result | green | crates/beacon/src/checks/bimi_results_table.rs |
| C7 | BIMI logo behind 4 and behind 5 redirects → today's result | green | crates/beacon/src/checks/bimi_results_table.rs |
| C8 | BIMI logo URL with `127.0.0.1`, `x@127.0.0.1`, `[::1]` → today's result | green | crates/beacon/src/checks/bimi_results_table.rs |
| C9 | beacon resolver error on a public logo host → today's result | green | crates/beacon/src/checks/bimi_results_table.rs |
| C10 | BIMI redirect to IP literal `https://10.1.2.3/l.svg` answering 200 → today's result | green | crates/beacon/src/checks/bimi_results_table.rs |
| C11 | MTA-STS policy host `[]` with system-resolvable public host, `10.0.0.1`, public address; policy endpoint answering 301 → today's result | green | crates/beacon/src/checks/mta_sts_results_table.rs |
| C12 | spectra: 3 redirects to 200; loop at `max_redirects` 10; redirect to `http://localhost:<p2>/`; redirect to a name resolving to `[public, 10.0.0.1]` → today's status, limit flag, hops, verdicts | already_implemented | crates/spectra/tests/redirect_results_table.rs |

C12 passed before any change (pinning). C10 uses a redirect to `https://127.0.0.1:<p>/l.svg` instead of `10.1.2.3`, which the client cannot intercept without contacting it. The spectra row "name resolving to `[public, 10.0.0.1]`" is omitted: spectra resolves later hops through the system resolver, so the row is not deterministic today; Phase 3 adds it with the helper's stub resolver. Rows 1 and 2 of the spectra table use `localhost` hops, which Phase 3's policy refuses; Phase 3 moves them to stub-resolved names (`ADLC-Test-Change`, harness only).

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| beacon client builders | 1 | sonnet | 25048 | 23 |

### Review

- AMENDMENT | spec requirement 7 said "committed green against today's code before any production change", but the tables need the two extracted client builders | affected_phase: 1 | repaired_in_phase: yes (requirement 7 now names the behaviour-neutral extraction)
- NIT | `crates/beacon/src/state.rs:35` doc comment says "five-hop"; the policy follows at most 4 redirects. Not acted on: Phase 3 deletes this builder.
- No BLOCKER: both builders are token-identical to the previous inline chains.

### Behavioural verification

skipped: no callable entry point changes; the phase is a test table plus a behaviour-neutral extraction.
