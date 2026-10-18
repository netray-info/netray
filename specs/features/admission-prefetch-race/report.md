# Report: admission-prefetch-race

## Phase 1 — Deregister before Recorded

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: `cargo test -p lens --lib admission::tests::recorded_follows_deregistration` exits 0 | green | crates/lens/src/admission.rs |
| C2 | R2: deregistered after `history::record` and `cache::hold`, right before `Recorded`; joinable through `Done` and recording | green | crates/lens/tests/admission_head_race.rs (`admission_head_race_c2_request_joins_a_recording_run`), crates/lens/src/admission.rs |
| C3 | R3: the `Deregister` drop guard still deregisters a task ending without `Recorded` and never removes a newer run | already_implemented | crates/lens/src/admission.rs (`Deregister::drop` unchanged) |
| C4 | R4: every existing lens test green unchanged; `just adlc-verify` passes | green | cargo test -p lens |
| C5 | Scenario: register held, no `Recorded` while registered; prefetch after the end gets `NotStarted` | green | crates/lens/src/admission.rs |
| C6 | Scenario: a prefetch joins a run in flight (c13, c8 unchanged) | already_implemented | crates/lens/tests/admission_prefetch.rs |

The test is the diagnosis loop verbatim (`bug.md`, `## Loop`), so it was written by the
implementer rather than a test-writer; C1 and C5 share it. Red before the fix (d05fce10):
`the stream ended on Recorded while the run was still joinable`.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | implementer (no coder: one moved statement) | 0 | 0 |

### Behavioural verification

skipped: no entry point shows the race on demand; the reporter's symptom was re-run instead:
the `admission_prefetch` test binary, 60 consecutive whole-file runs after the fix, 0 failures
(before: 2 failures in 30 runs). `cargo test -p lens`: 0 failures.

### Review

Reader over the phase diff: 0 blockers, 0 amendments, 1 NIT (not acted on in a phase):
`crates/lens/src/admission.rs:508` releases the slot (`drop(ticket)`) after `Recorded`, so in a
corner where the throttle has lapsed (record writes stalled past 60 s) and `domain_run_slots = 1`,
a same-domain admit in the window between deregistration and slot release gets `Refused(Slots)`
where it would get a fresh run after the task returns. Before the fix the same window answered a
join of the ended run; the slot ordering is unchanged by this phase.
