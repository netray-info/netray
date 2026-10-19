## b919a313..a296a78b

### Reader

COUNTS blockers=0 majors=1 minors=1
LENSES Engineering, Security, Testing
MAJOR | crates/web/src/pages/result/mod.rs:164 | The change makes a recheck POST of a Unicode or full-width domain fail, and the refusal tells the user to fix something that is already fixed. | `POST /b%C3%BCcher.de/run` (form `trigger=recheck`): `validate` now returns `xn--bcher-kva.de`, which no longer equals the segment, so `resolve` refuses with "write the domain in lower case without a trailing dot". Only from hand-made requests or a recheck form rendered before the deploy.
MINOR | crates/dns/tests/jobs.rs:448 | Test `cost_limit_c5_query_above_global_burst_is_refused_and_charges_nothing` cannot detect what its name claims: with per-IP burst 64 and cost 30, a burst comparison moved after the per-IP charge still leaves 34, so the follow-up check of 20 passes. | Move the comparison after the charges: the test stays green while R6 is broken.

```quote crates/web/src/pages/result/mod.rs:164
        "write the domain in lower case without a trailing dot",
```

```quote crates/dns/tests/jobs.rs:448
        limiter.check_query_cost(client(), &[], 20, 1).is_ok(),
```

### Spec reader

COUNTS blockers=1 majors=0 minors=0
REQUIREMENTS met=8 partial=0 missed=0 contradicted=1
BLOCKER | crates/dns/src/security/rate_limit.rs:170 | `check_query_cost` checks the bursts in the order per-IP, per-target, global, but the requirement says per-IP, global, per-target, "in that order", so a request over both the per-target and the global burst names the wrong limit. | R6. `[api.dns] per_target_burst = 5, global_burst = 20`, `POST /dns/query` with 8 types over cloudflare, google and quad9: spec order gives `CostExceedsLimit { cost: 24, limit: 20 }`, the code `{ cost: 8, limit: 5 }`.

```quote specs/features/v2-input-edges/spec.md:91
6. **DNS cost beyond a burst.** `check_query_cost` compares, before the stream check and before any
```

```quote crates/dns/src/security/rate_limit.rs:170
        for (cost, limit) in [
```

Phase 1 (R1-R5) met; Phase 2 R7-R9 met; unrequested behaviour: none found.

### Refuted

None.

### Calibration

| finding | refuter answer | confidence | outcome | command |
|---|---|---|---|---|
| F1 rate_limit.rs:170 order per-IP, per-target, global vs spec per-IP, global, per-target | CONFIRMED (suggests MINOR) | 6 | held | read of the loop at rate_limit.rs:170 against spec.md:91 |
| F2 result/mod.rs:164 POST of a Unicode segment gets an inapplicable hint | CONFIRMED (suggests MINOR) | 8 | held | read of `resolve` at result/mod.rs:143-166 |

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | convergence | adds only `CostExceedsLimit` error variants; no Status enum, template or `site/` file |
| P10 | absence | no check or scoring path changed |
| P12 | absence | no reqwest builder or module entry call |
| P13 | absence | limiter setup unchanged; pre-checks refuse on the existing limiters |
| P18 | convergence | no new client or config struct |
| P26 | absence | no new check id |
| P35 | absence | no `specs/rules/` change |
| P36 | convergence | `Cargo.lock` changes with its source |
| P40 | absence | no engine, registry, profile or catalogue change |

### Summary

Before refutation: reader 0/1/1, spec reader 1/0/0 (REQUIREMENTS met=8 partial=0 missed=0
contradicted=1). After refutation: unchanged, blockers 1, majors 1, minors 1; verified 2, held 2.
Roll call: answered 9, convergence 3, divergence 0, absence 6. Both held findings are fixed on
this branch (loop order per the spec; the POST refusal names the normal form) and the fix range is
read again.

## a296a78b..ee1392e4

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Testing

No wrong result: `check_query_cost` compares per-IP, global, then per-target before any charge;
existing tests keep their numbers under the new order; both new tests fail under the old code; the
POST refusal names the ASCII form for every segment that differs from its validated form.

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=9 partial=0 missed=0 contradicted=0

### Summary

Before and after refutation: reader 0/0/0, spec reader 0/0/0 (met=9). Nothing to refute or verify
(verified 0, held 0). No roll call on the fix range: it changes the order of one comparison and a
refusal text; the roll call of the full range above stands.
