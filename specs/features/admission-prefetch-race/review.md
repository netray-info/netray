# Review: admission-prefetch-race

## 309c6611..fd56f16a

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Testing

No wrong result. The fix moves `drop(deregister)` (crates/lens/src/admission.rs:506) after
`history::record` and `cache::hold` and before the `Recorded` `send_modify`. Traced: the register
mutex is released before the watch write, so a reader that saw `Recorded` finds the run removed;
dropping the guard's receiver early is harmless (`send_modify` updates without receivers); a panic
in `record`/`hold` still deregisters on unwind; DKIM runs have no key; in the new gap history is
recorded and the cache held, so `/api/check` steps 3 and 4, badge, og and the V1 runs callers get
the answer they get after the task returns; joining while recording is unchanged
(`admission_head_race_c2_request_joins_a_recording_run`); the phase NIT on the slot window needs a
run longer than 60 s with full slots and follows the documented gate order, not a wrong result; the
new test cannot deadlock (multi-thread runtime) and fails deterministically without line 506.
Not run: read-only.

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=4 partial=0 missed=0 contradicted=0

The test matches the bug.md loop (rustfmt wraps only). `Done` first; `record` and `hold` awaited
with the run registered (R2); `drop(deregister)` immediately before the `Recorded` write (R1, R2);
`Deregister::drop` and `same_channel` unchanged, guard bound for the whole task (R3); one test
added, no other test file changed (R4). A request after `Recorded` but before `drop(ticket)` used
to join the ended run and now meets the throttle or slot gate, as the decision log states.

### Refuted

None: no BLOCKER or MAJOR finding to refute.

### Calibration

None: the refuter was not called.

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | absence | no enum, UI or API copy changed |
| P10 | absence | no check or scoring path changed |
| P12 | absence | no outbound call or module entry added |
| P13 | convergence | the run's slot ticket is still held until after `Recorded`; no limiter added, removed or bypassed |
| P18 | absence | no reqwest client, no config struct changed |
| P26 | absence | no check id introduced |
| P35 | absence | no rule under specs/rules/ changed |
| P36 | absence | no generated file changed |
| P40 | absence | no module added or removed; crates/engine untouched |

### Summary

Before and after refutation: reader 0/0/0, spec reader 0/0/0 (REQUIREMENTS met=4 partial=0
missed=0 contradicted=0). verified 0, held 0. Roll call: answered 9, convergence 1, divergence 0,
absence 8. Both parts all zero: quote-check skipped.
