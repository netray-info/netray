
## 0b17f07e..db00c1ef

### Reader

COUNTS blockers=1 majors=0 minors=2
LENSES Engineering, Security, Testing
BLOCKER | crates/tls/src/routes.rs:1196 | The probe slot goes to the first planned address before any handshake runs and is never moved. If that address's handshake fails, no address is probed, so `tls.legacy_versions` (weight 5) and `tls.weak_suites` (weight 3) fall back to `not_applicable` "not reported" (crates/tls/src/module.rs:546). This happens even though a working TLS address was inspected and its seven probe connections were charged. | Host example.com, A records 192.0.2.1 (handshake times out or is refused) and 192.0.2.2 (handshake OK, accepts TLS 1.0), port 443, default burst 10, LiveBackend (7 probes). `plan_counts(2,1,10,7)` returns 2 inspected and 1 probed, at cost 9. `select_representative_ips` keeps DNS order, so `probed` covers .1 only. .1 has `version` None and gets no probes. .2 has idx 1, which is not < 1, so `?` returns None. `translate` finds no probed address and uses the fallback: `tls.legacy_versions` is NotApplicable and drops out of the grade. At base 0b17f07e every address that completed a handshake was probed, so the same host got a Fail from .2.
MINOR | crates/tls/tests/inspector.rs:598 | The test named for R10 still passes if the handshake permit is held across validation, OCSP and probes again. | `limits()` gives a budget of 10, so with two addresses the plan probes only one (probed=1, the same as `two_addresses_probe_only_the_first_with_a_warning`). If the `_permit` block scope is reverted to cover the whole task, the probed address holds the single handshake permit for about 1.25 s (deadline plus 250 ms grace). The other address then handshakes at once and is not probed, so elapsed is about 1.25 s, under 1.8 s. No other test sends two probed addresses through one handshake permit.
MINOR | crates/tls/tests/it/module_limits.rs:37 | The only test cited for R7 (report C4, marked `already_implemented` though crates/tls/src/module.rs:227 changes in this range) never reaches the module's plan, so it cannot fail if the module's probe charge is removed. | 127.0.0.1 is a blocked target, so `select_ips` returns Err and the charge falls back to `parsed.ports.len()`. Reverting module.rs:227-228 to `self.check_budget` and `0` (no per-target cap, no probe cost) keeps module_limits.rs and the `select_ips` unit tests (which pass 0 probes) green.

```quote crates/tls/src/routes.rs:1196
                let version = version.as_ref().filter(|_| idx < probed)?;
```

```quote crates/tls/src/module.rs:546
        (Status::NotApplicable, "not reported")
```

```quote crates/tls/tests/inspector.rs:598
        assert!(elapsed < Duration::from_millis(1800), "took {elapsed:?}");
```

```quote crates/tls/tests/it/module_limits.rs:37
        let r = reason(&module, "127.0.0.1").await;
```

```quote crates/tls/src/module.rs:227
            self.check_budget.min(config.limits.per_target_burst),
```

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=12 partial=0 missed=0 contradicted=0
I traced requirements 1 to 12 through the code at db00c1ef and none diverges from its words. Behaviour no requirement names, the job limiter's switch to the smaller `[api.tls]` burst, is in the spec's decision log, so it is not unrequested.

### Calibration

| finding | refuter | confidence | result | command |
|---|---|---|---|---|
| BLOCKER routes.rs:1196 probe slot not moved on a failed handshake | CONFIRMED | 8 | held | cargo test -p netray-tls --features testing --test inspector probe_slots::a_failed_first_handshake_passes_its_probe_slot_on (fails at db00c1ef) |

### Roll call

| principle | answer |
|---|---|
| P03 | convergence |
| P10 | convergence |
| P12 | convergence |
| P13 | convergence |
| P18 | absence |
| P26 | convergence |
| P35 | absence |
| P36 | absence |
| P40 | absence |

### Summary

Before refutation: reader 1/0/2, spec reader 0/0/0 (REQUIREMENTS met=12 partial=0 missed=0
contradicted=0). After: reader 1/0/2, spec reader 0/0/0. verified 1, held 1. Roll call answered 9,
convergence 4, divergence 0, absence 5. The blocker is fixed on this branch (implementer under the
operator's autonomy mandate) and the fixed range is read again.

## db00c1ef..e4d99abb

### Reader

COUNTS blockers=1 majors=0 minors=0
LENSES Engineering, Testing
BLOCKER | crates/tls/src/routes.rs:1176 | No probe or live OCSP check starts until every handshake on the port has returned; a timed-out first address pushes lens's TLS section past its 15 s window (TimedOut), where the previous code returned a measured section. | handshake_timeout_secs 5, quality on (5 s), [backends.tls] timeout_ms 15000; A drops SYNs, B healthy with one probe unanswered: stage 1 ends at 5 s, B's probes run 5 s more, quality against A 5 s more: about 15 s plus resolve.

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=12 partial=0 missed=0 contradicted=0

### Calibration

| finding | refuter | confidence | result | command |
|---|---|---|---|---|
| BLOCKER routes.rs:1176 port barrier delays probes and OCSP | not sent (verified directly) | n/a | held | crates/tls/tests/inspector.rs probe_slot_no_barrier: a 1 s failing first handshake makes the barrier version take about 2 s |

### Summary

reader 1/0/0, spec reader 0/0/0 (met=12). verified 1, held 1. Fixed on this branch: slots without a
port barrier, probe deadline from the address's own handshake.

## e4d99abb..6289638a

### Reader

COUNTS blockers=0 majors=0 minors=1
LENSES Engineering, Testing
MINOR | crates/tls/tests/inspector.rs:882 | The new test's Unanswered verdict rests on the slow address's 1000 ms sleep firing after the good address's 1 s probe deadline, which holds only by timer rounding. | A 2 ms deschedule between the slow sleep and the good deadline lets the slot arrive with about 1 ms left; the honouring stub then returns its Refused answers.

### Spec reader

skipped: the range changes no requirement; the previous reading's spec reader found met=12.

### Summary

reader 0/0/1, fixed in the next commit (the slow handshake takes 1200 ms). verified 0, held 0.
