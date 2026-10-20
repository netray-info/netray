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
