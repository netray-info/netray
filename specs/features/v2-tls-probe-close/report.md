# Report: v2-tls-probe-close

## Phase 1 — A close is a refusal

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 a close or reset after the written ClientHello with nothing received is refused, closed or closed (reset) | green | crates/tls/tests/it/probe_classify.rs |
| C2 | R2 a partial answer, timeout, garbage, other alert or a failure before the write stays unanswered | green | crates/tls/tests/it/probe_classify.rs |
| C3 | R3 the evidence holds a closed refusal as disabled or refused with answer closed | already_implemented | crates/tls/tests/it/probe_close_evidence.rs |
| C4 | R12 SDD, v2-tls-versions-coop decision log and CHANGELOG state the close rule | test-unwritable | prose: specs/sdd/v2.md S42, S41, §3.7 |
| C5 | no bytes and end of stream: refused, detail closed, no version or suite | green | crates/tls/tests/it/probe_classify.rs |
| C6 | no bytes and reset: refused, detail closed (reset) | green | crates/tls/tests/it/probe_classify.rs |
| C7 | partial record and end of stream: unanswered | green | crates/tls/tests/it/probe_classify.rs |
| C8 | warning alert then end of stream: unanswered | green | crates/tls/tests/it/probe_classify.rs |
| C9 | HTTP text or timeout: unanswered | green | crates/tls/tests/it/probe_classify.rs |
| C10 | connect failure: unanswered naming it | green | crates/tls/tests/it/probe_classify.rs |
| C11 | responder closing after the ClientHello: every probed kind refused closed, TLS 1.3 enabled | green | crates/tls/tests/it/probe_address.rs |
| C12 | responder resetting after the ClientHello: every probed kind refused closed (reset) | green | crates/tls/tests/it/probe_address.rs |
| C13 | failure before the ClientHello was written: unanswered naming it | green | crates/tls/tests/it/probe_classify.rs |
| C14 | canned TLS 1.0 refused closed: tls1_0 disabled, answer closed, legacy_versions pass | already_implemented | crates/tls/tests/it/probe_close_evidence.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 0 | implementer (one function in probe.rs, coded directly) | 0 | 0 |

Tests were written by the implementer directly (four small edits to existing probe tests); C3 and
C14 pass without the change because the module already reads any `Refused` outcome's detail.

### Review

- AMENDMENT (logged, not repaired): a close at accept (per-client connection limit) reads as a
  refusal; accepted residual, stated in S42.
- AMENDMENT (fixed): the CHANGELOG said an unanswered probe makes the check unmeasured; it does
  only when no other probe decides it.
- NIT: the NotSent mapping is untested end to end (STARTTLS ports are privileged).
- NIT (fixed): `probe_close_evidence.rs` reformatted by rustfmt; the phase commit names it.

### Behavioural verification

skipped: the phase changes a library function; `probe_address` runs end to end against the
in-process close and reset responders on 127.0.0.1 (C11, C12). A live target needs DNS, blocked
locally.

## Phase 2 — Probes are charged

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R4 probe_connections: live 7, stub 7 with canned probes, 0 without | green | crates/tls/tests/it/probe_budget.rs, crates/tls/tests/inspector.rs |
| C2 | R5 one plan decides inspected and probed addresses for a budget | green | crates/tls/tests/it/probe_budget.rs |
| C3 | R6 the inspector plans against the smaller burst and charges the planned cost once | green | crates/tls/tests/inspector.rs |
| C4 | R7 the module plans against check_budget and its per-target burst and charges the plan | already_implemented | crates/tls/tests/it/module_limits.rs |
| C5 | R8 the inspector job admission charges the one-address plan | green | crates/web/tests/it/inspect_tls_probe_cost.rs |
| C6 | R9 inspect_port probes only the planned addresses after a successful handshake | green | crates/tls/tests/inspector.rs |
| C7 | R12 SDD and crates/tls/CLAUDE.md state the cost | test-unwritable | prose: specs/sdd/v2.md §3.7, crates/tls/CLAUDE.md |
| C8 | one address, one port, budget 10: inspected and probed at cost 8 | green | crates/tls/tests/it/probe_budget.rs |
| C9 | two addresses, budget 10: both inspected, first probed, cost 9, probing 1 of 2 | green | crates/tls/tests/it/probe_budget.rs |
| C10 | four addresses, budget 10: three inspected, one probed, inspecting 3 of 4 | green | crates/tls/tests/it/probe_budget.rs |
| C11 | two ports, one address, budget 10: none probed, probing 0 of 1 | green | crates/tls/tests/it/probe_budget.rs |
| C12 | zero probe connections: cap-and-warn as before | green | crates/tls/tests/it/probe_budget.rs |
| C13 | probe_connections of live and stub backends | green | crates/tls/tests/it/probe_budget.rs, crates/tls/tests/inspector.rs |
| C14 | inspector burst 10 with probes: the second inspection is rate limited | green | crates/tls/tests/inspector.rs |
| C15 | inspector two addresses with probes: both handshaken, first probed, warning | green | crates/tls/tests/inspector.rs |
| C16 | inspector burst 3, four ports: CostExceedsLimit 4 over 3, nothing charged | already_implemented | crates/tls/tests/inspector.rs |
| C17 | module per_target_burst 2: two runs reach the policy, the third is rate limited | already_implemented | crates/tls/tests/it/module_limits.rs |
| C18 | inspector job admission burst 10 with probes: the second admission is refused | green | crates/web/tests/it/inspect_tls_probe_cost.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 0 | implementer (plan in routes.rs, wired into inspector, module, jobs; coded directly) | 0 | 0 |

### Review

- AMENDMENT (repaired in phase): the inspector job's per-job limiter used the per-IP burst while
  admission planned against the smaller burst, so with a smaller per-target burst the job ran
  probes admission never charged; the job now plans against the smaller burst.
- Everything else traced: the plan's three branches, the inspector's single charge (it also drops
  the old double per-IP charge when per-target refused), the module's budget, inspect_port's
  probed index.

### Behavioural verification

skipped: the entry points (`POST /api/v1/tls/inspect`, the inspect page, lens runs) need DNS for a
live target, blocked locally; the inspector, the module and the job admission run end to end over
the stub backend in `crates/tls/tests/inspector.rs`, `crates/tls/tests/it/module_limits.rs` and
`crates/web/tests/it/inspect_tls_probe_cost.rs`.

## Phase 3 — Permits and the probe deadline

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R10 the handshake permit is released when the handshake returns | green | crates/tls/tests/inspector.rs |
| C2 | R11 probes take a separate probe permit; wait and probes bounded by the handshake timeout; past it unanswered timed out | green | crates/tls/tests/inspector.rs |
| C3 | R12 SDD §3.7 states the permits and the deadline | test-unwritable | prose: specs/sdd/v2.md §3.7 |
| C4 | one handshake permit, probes hanging 10 s, two addresses: both handshakes, done within 1.8 s | green | crates/tls/tests/inspector.rs |
| C5 | probes hanging 10 s: probed kinds unanswered timed out, TLS 1.3 enabled, legacy_versions unmeasured | green | crates/tls/tests/inspector.rs |
| C6 | probe permit held elsewhere: probes unanswered timed out within about one timeout | green | crates/tls/tests/inspector.rs |
| C7 | review: answers a backend returns at the time left are kept | green | crates/tls/tests/inspector.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 0 | implementer (permit scope and probe deadline in routes.rs, coded directly) | 0 | 0 |

### Review

- BLOCKER (fixed, second pass green): one outer timer at the deadline raced the per-probe timers
  and wiped answered probes when one hung (94 of 100 runs in the reviewer's repro). The deadline
  now bounds the permit wait only; probes run on the time left with a 250 ms grace. C7 pins it
  with a stub that returns its answers just after the time left.
- NIT (fixed with C7): the stub ignored its timeout.
- Checked: the handshake permit is block-scoped; from_handshake matches the replaced code.

### Behavioural verification

skipped: the permit and deadline paths need a hanging live server behind a resolvable name (DNS
blocked locally); they run end to end through the inspector over the stub in
`crates/tls/tests/inspector.rs`.
