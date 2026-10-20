# Spec: v2-tls-probe-close

Status: Ready for Implementation
Created: 2026-10-20

## Goal

Three operator decisions of 2026-10-20 on the ClientHello probes of v2-tls-versions-coop (PR #43).
First, a server that closes the connection (FIN or RST) after a probe's ClientHello without sending
anything is refusing the offer, as Windows SChannel does for disabled versions: the probe is
`refused` with the answer `closed`, no longer `unanswered`. Second, probe connections are charged to
the rate limiters the TLS inspection already uses, so one request cannot fan out uncharged. Third, a
hanging server no longer holds a handshake permit: the permit is released after the handshake, and
probes run under their own bounded concurrency and deadline, so queued inspections do not run into
the request timeout.

## Non-goals

- No change to the ClientHello bytes, the probe set, the suite groups or the verdicts of a
  ServerHello or an alert.
- No new configuration key: the probe concurrency reuses `limits.max_concurrent_handshakes`, the
  probe deadline the handshake timeout.
- No new global limiter: the TLS inspection has none today (per-IP and per-target only); lens's
  global limiter charges one per domain run, not per connection.
- No template, stylesheet or island change; no V1 wire or API v1 schema change.
- No change to the existing per-address cost of a request without probes.

## Context and constraints

- `crates/tls/src/tls/probe.rs`: `exchange` connects, runs the SMTP STARTTLS upgrade on 25 and 587,
  writes the ClientHello and reads until `decide` answers; `classify` turns an end without an answer
  into `Unanswered` naming the end (`closed without an answer`, `reset`, `timed out`,
  `connect failed`). Connect and STARTTLS failures share the `Reset`/`ConnectFailed` ends with the
  read phase today.
- Rule M (S34) counts a refused connection, a reset and a TLS alert as answers (`fail`) and a timeout
  as `unmeasured`. v2-tls-versions-coop narrowed it for probes (a close or reset is unanswered);
  this feature takes that narrowing back for a close after the ClientHello was written.
- Rate limits (`crates/tls/src/security/rate_limit.rs`): cost is ports x inspected addresses against
  the per-IP and per-target bursts; a cost above a burst is refused up front with
  `CostExceedsLimit` (v2-input-edges, `400 INVALID_INPUT` in API v1, the inspector job's notice).
  The inspector caps addresses to the smaller burst with the warning
  `rate limit: inspecting N of M IPs to stay within budget`. The module caps to `check_budget` and
  charges its per-target limiter. The inspector job (`crates/web/src/api/jobs.rs`) charges the port
  count on `[api.tls]` at admission and caps the inspection with a per-job limiter of the per-IP
  burst.
- `routes::inspect_port` holds the shared handshake permit (`AppState::handshake_semaphore`, size
  `max_concurrent_handshakes`) across the handshake, validation, live OCSP and the probes.
- `StubTlsBackend` answers no probes unless a test sets canned ones.

## Requirements

1. A probe whose ClientHello was written and whose connection then ends by end of stream before any
   byte arrived is `refused` with detail `closed`; ended by a reset before any byte arrived it is
   `refused` with detail `closed (reset)`. Neither carries a version or suite.
2. A connection that ends after some bytes that decide nothing (a partial record, a warning alert),
   a timeout, bytes that are not TLS, an alert other than a fatal handshake_failure,
   protocol_version or insufficient_security, and any failure before the ClientHello was written
   (connect, STARTTLS, the write itself) stay `unanswered` naming why.
3. The `tls.probes` block of an address whose probe was refused by a close holds the key as
   `disabled` (versions) or `refused` (suite groups) with `<key>_answer` `closed` or
   `closed (reset)`; `tls.protocols`, `tls.legacy_versions` and `tls.weak_suites` treat it as any
   other refusal.
4. `TlsBackend::probe_connections()` answers the connections one probed address costs: the live
   backend 7, `StubTlsBackend` 7 with canned probes and 0 without.
5. One plan decides, for a budget, the addresses inspected and how many of them are probed: with
   cost per port of one per inspected address plus `probe_connections()` per probed address, every
   address is inspected and probed when the whole cost fits; else, when one probed address fits on
   every port, the addresses are capped so that it does and the remaining budget probes further
   addresses in selection order; else no address is probed and the addresses are capped as before.
   The first addresses in selection order are the probed ones. A cap on probes adds the warning
   `rate limit: probing K of N addresses to stay within budget`.
6. The inspector plans against the smaller per-IP and per-target burst and charges the planned cost
   (handshakes plus probe connections) once; a request whose port count alone exceeds a burst is
   still refused up front with `CostExceedsLimit { cost: ports, limit }` and charges nothing.
   Probes never cause a refusal: they are capped.
7. The module plans against the smaller of `check_budget` and its per-target burst and charges the
   planned cost to its per-target limiter.
8. The inspector job's admission charges `[api.tls]` the planned cost of one address on every
   requested port (with the backend's probe connections) instead of the port count.
9. `inspect_port` probes only the addresses the plan marks as probed, and only after a successful
   handshake.
10. The handshake permit is released when the handshake returns, before validation, live OCSP and
    probes.
11. Probes of one address take a permit of a separate probe semaphore (`AppState`, size
    `max_concurrent_handshakes`); the wait for it and the probes together are bounded by the
    handshake timeout, and each probe gets the time left. When that deadline passes, every probe
    kind the address would have sent is `unanswered` `timed out`, and TLS 1.2 and 1.3 keep their
    handshake-derived values.
12. SDD `specs/sdd/v2.md` states the close rule as a decision row refining S34 and S41 and amends
    S41 and §3.7 (probes, cost, permits), in the same commit as the code; the decision log of
    v2-tls-versions-coop carries amendment notes on its close, cost and permit decisions;
    `CHANGELOG.md` `[Unreleased]` names the grade change; `crates/tls/CLAUDE.md` states the cost.

## Phase 1 — A close is a refusal

**Depends on:** none
**Requirements:** 1, 2, 3, 12 (close rule)

`crates/tls/src/tls/probe.rs`, the in-process responder in `crates/tls/tests/it/probe_address.rs`
(close and reset behaviours), SDD S41 and §3.7, the decision row, the v2-tls-versions-coop decision
log, `CHANGELOG.md`.

### Test Scenarios

GIVEN no bytes and an end of stream after the ClientHello WHEN classified THEN the probe is refused with detail closed and no version or suite.
GIVEN no bytes and a reset after the ClientHello WHEN classified THEN the probe is refused with detail closed (reset).
GIVEN a partial record and an end of stream WHEN classified THEN the probe is unanswered.
GIVEN a warning alert followed by an end of stream WHEN classified THEN the probe is unanswered.
GIVEN the text HTTP/1.1 400 or a timeout WHEN classified THEN the probe is unanswered.
GIVEN a connect failure WHEN classified THEN the probe is unanswered naming the connect failure.
GIVEN a responder that reads the ClientHello and closes WHEN probe_address runs after a TLS 1.3 handshake THEN every probed kind is refused with detail closed and TLS 1.3 is enabled.
GIVEN a responder that reads the ClientHello and resets WHEN probe_address runs THEN every probed kind is refused with detail closed (reset).
GIVEN a responder that accepts and closes before reading anything on a STARTTLS port WHEN probed THEN the kind is unanswered.
GIVEN canned probes where TLS 1.0 is refused with detail closed WHEN the module translates THEN tls.probes holds tls1_0 disabled with tls1_0_answer closed, and tls.legacy_versions passes when SSLv3 and TLS 1.1 are refused too.

## Phase 2 — Probes are charged

**Depends on:** Phase 1
**Requirements:** 4, 5, 6, 7, 8, 9, 12 (cost)

`crates/tls/src/backend.rs`, `crates/tls/src/testing.rs`, `crates/tls/src/routes.rs` (plan,
`InspectRequest`, `inspect_port`), `crates/tls/src/inspector.rs`, `crates/tls/src/module.rs`,
`crates/web/src/api/jobs.rs` (`admit_tls`), SDD §3.7, `crates/tls/CLAUDE.md`.

### Test Scenarios

GIVEN one address, one port, seven probe connections and budget 10 WHEN planned THEN one address is inspected and probed at cost 8.
GIVEN two addresses, one port, seven probe connections and budget 10 WHEN planned THEN both are inspected, the first is probed, the cost is 9 and the warning names probing 1 of 2 addresses.
GIVEN four addresses, one port, seven probe connections and budget 10 WHEN planned THEN three are inspected, one probed, and the inspecting warning names 3 of 4.
GIVEN two ports, one address, seven probe connections and budget 10 WHEN planned THEN the address is inspected on both ports, none is probed, and the probing warning names 0 of 1.
GIVEN zero probe connections WHEN planned THEN the plan selects the addresses cap-and-warn selected before and probes all of them.
GIVEN the live backend WHEN asked for probe connections THEN it answers 7; GIVEN the stub without canned probes THEN 0; with canned probes THEN 7.
GIVEN an inspector with per-IP burst 10 over a stub with canned probes and one address WHEN it inspects twice from one client THEN the first is answered and the second is rate limited, as the first charged 8.
GIVEN an inspector with per-IP burst 10 over a stub with canned probes and two addresses WHEN it inspects THEN both addresses are handshaken, only the first is probed, and the warnings name probing 1 of 2.
GIVEN an inspector with per-IP burst 3 WHEN four ports are requested THEN it is refused with CostExceedsLimit cost 4 limit 3 and the limiter still admits cost 3.
GIVEN the module with per_target_burst 2 on 127.0.0.1 WHEN run three times THEN the first two reach the target policy and the third is rate limited (per_target).
GIVEN the inspector job admission with [api.tls] per-IP burst 10 and a probing inspector WHEN one port is admitted twice from one client THEN the second admission is refused with a retry time.

## Phase 3 — Permits and the probe deadline

**Depends on:** Phase 2
**Requirements:** 10, 11, 12 (permits)

`crates/tls/src/state.rs` (probe semaphore), `crates/tls/src/routes.rs` (`inspect_port`),
`crates/tls/src/tls/probe.rs` (`ProtocolProbes::unanswered`), `crates/tls/src/testing.rs` (probe
delay), SDD §3.7.

### Test Scenarios

GIVEN max_concurrent_handshakes 1, a handshake timeout of 1 s and a stub whose probes hang for 10 s WHEN two addresses are inspected THEN both handshakes complete and the inspection returns within 1.8 s.
GIVEN a handshake timeout of 1 s and a stub whose probes hang for 10 s WHEN one address is inspected THEN its probes are unanswered timed out for SSLv3, TLS 1.0, TLS 1.1 and the suite groups, TLS 1.3 is enabled from the handshake, and tls.legacy_versions is unmeasured.
GIVEN a probe semaphore permit held elsewhere for longer than the handshake timeout WHEN an address is probed THEN its probes are unanswered timed out within about one handshake timeout.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate for features 0-7
  (operator, 2026-10-19).
- Independent reading of the Draft skipped: the review step reads the whole range against the spec
  (implementer under the operator's autonomy mandate, 2026-10-19).
- A close (FIN or RST) after the ClientHello with nothing received is `refused` (operator,
  2026-10-20). Why it is not a failed lookup: the handshake on the same address and port just
  succeeded, so the server speaks TLS there; it read a well-formed ClientHello and chose to end the
  connection instead of answering. That is the server's answer to the offer, as an alert is, and
  S34 already counts a reset as an answer. Windows SChannel refuses disabled versions this way, so
  without the rule such servers leave `tls.legacy_versions` unmeasured and the TLS section
  incomplete.
- The close counts only when nothing at all was received and the ClientHello was fully written:
  bytes that decide nothing (a partial record, a warning alert) before the close stay unanswered,
  and failures before the write (connect, STARTTLS, the write itself) are `unanswered` naming the
  phase, because the server never saw the offer (implementer under the operator's autonomy
  mandate, 2026-10-19).
- Answer texts `closed` and `closed (reset)`: the evidence says why the probe counts as refused and
  which close it was (implementer under the operator's autonomy mandate, 2026-10-19).
- Probes are capped, never refused (operator offered both, 2026-10-20): a request's minimum cost
  stays its handshakes, so the up-front `CostExceedsLimit` refusal of v2-input-edges keeps its
  meaning (ports beyond a burst) and its shape; probes that do not fit are dropped with a
  cap-and-warn warning, like addresses (implementer under the operator's autonomy mandate,
  2026-10-19).
- The plan reserves one probed address before filling handshakes: the graded protocol checks
  (`tls.legacy_versions` 5, `tls.weak_suites` 3) need one probed address, while further addresses
  only feed the consistency checks. Consequence at the default burst 10 on one port: at most three
  addresses are inspected (was ten) and one is probed; a single-address host is unchanged. A
  multi-port request with too little budget probes no address, and the two checks are
  not_applicable "not reported" with the warning naming the cap (implementer under the operator's
  autonomy mandate, 2026-10-19).
- Cost counts seven connections per probed address even when the handshake negotiated TLS 1.2 and
  only six run, because the plan is made before the handshake; a handshake that fails still costs
  its address's probes, as a failed handshake costs its handshake (implementer under the operator's
  autonomy mandate, 2026-10-19).
- The cost comes from the backend (`probe_connections`), not a constant: a backend that does not
  probe costs nothing extra, which keeps every limit test over the probe-less stub unchanged
  (implementer under the operator's autonomy mandate, 2026-10-19).
- The inspector job's admission charges one address per port with probes; the job's own per-job
  limiter (burst = the per-IP burst) still caps further addresses without charging `[api.tls]`
  for them. That pre-dates this feature (v2-inspector-shell) and is bounded by the burst; making
  the job charge per address is deferred (implementer under the operator's autonomy mandate,
  2026-10-19).
- No global limiter is added: the TLS inspection has none to charge (implementer under the
  operator's autonomy mandate, 2026-10-19).
- Probe concurrency reuses `max_concurrent_handshakes` as the number of addresses probing at once
  (each up to seven connections) instead of a new config key, and the deadline is the handshake
  timeout covering the wait and the probes; a probe cut by the deadline is unanswered `timed out`
  (Rule M). An inspection then takes at most two handshake timeouts plus queueing for the handshake
  permit alone (implementer under the operator's autonomy mandate, 2026-10-19).

## Open decisions

None.

## Out of scope

- Charging the inspector job per inspected address on `[api.tls]`.
- A probe on/off switch or separate probe config keys.
- Re-weighting `tls.legacy_versions` after the close rule.
