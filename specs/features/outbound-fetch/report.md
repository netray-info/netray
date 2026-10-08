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

## Phase 2 — Fetch helper

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R1: resolve once, refuse empty set or any disallowed address with typed `Blocked`, connect only to checked addresses | green | crates/common/tests/fetch.rs |
| C2 | R2: reqwest's redirect handling with a custom policy, every hop re-checked, caller sets limit and `Fail`/`ReturnLast`, HTTPS-only, body cap | green | crates/common/tests/fetch.rs, fetch_semantics.rs, fetch_reqwest_semantics.rs, fetch_guards.rs, fetch_env_proxy.rs |
| C3 | `https://127.0.0.1:<p>/` → `Blocked`, 0 connections | green | crates/common/tests/fetch.rs |
| C4 | `https://x@127.0.0.1:<p>/` → `Blocked`, 0 connections | green | crates/common/tests/fetch.rs |
| C5 | `https://[::1]:<p>/` → `Blocked`, 0 connections | green | crates/common/tests/fetch.rs |
| C6 | name stub-mapped to `[]` → `Blocked` | green | crates/common/tests/fetch.rs |
| C7 | name mapped to `[public, 127.0.0.1]` → `Blocked` | green | crates/common/tests/fetch.rs |
| C8 | `pinned.invalid` stub-mapped to an allowed listener → succeeds (proves the pin) | green | crates/common/tests/fetch.rs |
| C9 | allowed hop redirecting to `localhost:<p>` or `http://svc.invalid/` (stub → loopback) → `Blocked`, 0 connections at the target | green | crates/common/tests/fetch.rs |
| C10 | limit 4 with `Fail`: 4 redirects to 200 succeed, 5 fail | green | crates/common/tests/fetch.rs |
| C11 | limit 2 with `ReturnLast`: 3 redirects → the second 3xx is returned with the limit flag | green | crates/common/tests/fetch.rs |

Beyond the eleven criteria, the phase review added `fetch_semantics.rs` (13), `fetch_reqwest_semantics.rs` (4), `fetch_guards.rs` (3) and `fetch_env_proxy.rs` (1): 37 tests in all.

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| fetch helper, hand-written loop | 1 | opus | 56333 | 198 |
| amendments to the loop | 1 | opus | 61633 | 181 |
| redesign on reqwest | 2 | opus (two test-harness fixes by the orchestrator) | 100068 | 306 |
| typed client settings, fragment | 1 | opus (two test-harness fixes by the orchestrator) | 41666 | 85 |

### Review

Four readers over the phase.
- First pass (hand-written loop): 1 BLOCKER (non-ASCII Location → error), 5 AMENDMENTs. Repaired in phase.
- Second pass: 1 BLOCKER (a final 3xx body is read), 5 AMENDMENTs, all about divergence from reqwest. Halted with `blocker`. The operator chose to rebuild on reqwest.
- Third pass (redesign): 2 BLOCKERs, 4 AMENDMENTs. The BLOCKERs were that `base` could inject `resolve` overrides or `unix_socket` around the check, and that the initial fragment leaked into `hops[0]`. Repaired with typed `ClientSettings`, `set_fragment(None)`, and tests for literal hops, `https_only` per hop and env proxies.
- Fourth pass: no BLOCKER. Three AMENDMENTs were repaired in the spec and plan text (requirements 1 and 2, the Location ordering note) or forwarded as Phase 3 notes (userinfo in the initial `Scheme` error).
  - NIT: `rcgen`, `rustls` and `tokio-rustls` are declared per crate rather than in `[workspace.dependencies]`.
- Security boundary: no finding in any pass.

### Behavioural verification

skipped: the helper has no entry point of its own yet; Phase 3 wires it into beacon, spectra and tlsight.
