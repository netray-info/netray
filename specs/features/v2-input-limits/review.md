# Review: v2-input-limits

## 04776edb..b9e8a0c0

### Reader

COUNTS blockers=0 majors=0 minors=1
LENSES Engineering, Security, Testing
MINOR | crates/lens/tests/it/address_domain_routes.rs:88 | C7's "no run recorded" check cannot fail on its own: it reads `history.db` straight after the response, with no bounded poll and no `state.runs.drain()`, which the repo CLAUDE.md requires for lens history reads. | A regression that answers 400 but still starts the run in the background records after the response, so `store.latest("192.0.2.1")` is still `None` at line 88; any regression recording before answering also makes the status 200 and fails the line before first.

```quote crates/lens/tests/it/address_domain_routes.rs:88
    assert!(store.latest("192.0.2.1").await.unwrap().is_none());
```

```quote CLAUDE.md:56
- lens records a run after its response: a test reading `history.db` or the record-error counter after a response polls with a bound or awaits `state.runs.drain()`.
```

Apart from that assertion, no wrong result in the code. Domain rule compiled at END and run on 31 inputs (R1, R2, C4 to C6 as expected; `.`, `1.2.3.4..`, `192.0.2.1..` still refused with the empty-label error). `add_dnssec_types` runs before `QueryPolicy::validate` and the cost in `query_parsed` and `compare_parsed`; `total_queries` and `total_batches` unchanged; all four callers go through these functions; the inspector sends `+dnssec` to the DNSSEC job.

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=5 partial=0 missed=0 contradicted=0

R1 to R5 traced at b9e8a0c0; nothing wrong. `validate` and `is_number` run on `192.0.2.1.`, `[::1]`, `[::1].`, `0x7f.1`, `192.0.2.01`, `x.0xg`, `a1`; lens's result page check runs before any redirect; badge, API v1 domain routes and the evidence page refuse through the same rule; `add_dnssec_types` precedes the policy and the cost in both job functions and skips types already requested.

### Refuted

None: no BLOCKER or MAJOR finding went to the refuter.

### Calibration

None.

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | absence | no new enum or UI/API copy |
| P10 | absence | no check or scoring path changed |
| P12 | absence | no reqwest builder or engine module call changed |
| P13 | absence | no limiter or route added; the DNSSEC types now count against the existing budget |
| P18 | absence | no Client constructed, no config struct touched |
| P26 | absence | no new check id |
| P35 | absence | no specs/rules change |
| P36 | convergence | no generated artefact changed |
| P40 | absence | no module added or removed |

### Summary

Before refutation: reader 0/0/1, spec reader 0/0/0 (REQUIREMENTS met=5 partial=0 missed=0 contradicted=0). After refutation: the same, 0 blockers, 0 majors, 1 minor (C7's store read is not the assertion that can fail; the status assertion before it carries the criterion). verified=0, held=0. Roll call: answered 9, convergence 1, divergence 0, absence 8.
