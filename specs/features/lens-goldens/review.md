# Review: lens result goldens and backend results tables

## e5efabf..788ad89

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Testing

No wrong result found. Checked and found sound: the lens golden harness (every stub matches the path and method lens calls; wave 2 receives 192.0.2.10 from `prism.sse`; cache off and a fresh `AppState` per run; the projection is deterministic, `hard_fail_checks` can only come from TLS; `diff_fields` covers objects, arrays and scalars; the three `lens-*.json` agree with lens's bucket mapping of the beacon goldens); the DKIM table (production calls `check_dkim` without a Null-MX guard and emits it unmodified; only `default._domainkey` is probed; the test key is a 2048-bit RSA SPKI); the port table (`routes.rs:879` passes failed IPs to `assess_port`, which reads only `error.is_none()`; `HANDSHAKE_FAILED` is the only code `inspect_ip` emits; no check reads the clock); the lint table (prism merges every resolver's lookups into `all_lookups` before the lints; mhost 0.11.3's lints ignore `valid_until`); the target policy table (every row agrees with `is_blocked_v4`/`is_blocked_v6`); test collection (no `autotests = false`; the gate's `cargo test --workspace --exclude ifconfig-rs` runs every new file). The reader did not run the tests.

### Refuted

None: no BLOCKER or MAJOR to refute. The anchor check was skipped (COUNTS all zero).

### Calibration

No refuter ran: no finding to answer. verified 0, held 0.

### Roll call

| Principle | Answer | Evidence |
|---|---|---|
| P03 | absence | only tests and goldens added; no enum, template, site or serializer changed |
| P10 | absence | no check or scoring code changed; the tables pin today's branches |
| P12 | absence | no reqwest builder or engine entry added |
| P13 | absence | no serve command, limiter or route changed |
| P18 | absence | no `reqwest::Client` added, no config struct changed |
| P26 | absence | no new check ID; `dkim_results_table` drives the existing `check_dkim` |
| P35 | convergence | contract goldens `lens-*.json` and results tables pin today's results |
| P36 | convergence | the goldens arrive with their generator; `UPDATE_GOLDEN=1 cargo test -p lens --test lens_golden` at 788ad89 leaves `tests/fixtures/contracts` byte-identical (checked by the orchestrator) |
| P40 | absence | no module added or removed |

### Summary

Before refutation 0/0/0, after 0/0/0. verified 0, held 0. Roll call: 9 answered, 2 convergence, 0 divergence, 7 absence.
