## e3d455fe..6944dc35

### Reader

COUNTS blockers=0 majors=1 minors=2
LENSES Engineering, Security, Testing
MAJOR | crates/engine/src/history.rs:62 | For every v3 to v4 diff, the step labelled "profile effect" always equals the old grade, so any letter change caused by the three new checks shows up as "domain effect". That contradicts SDD §4.3 ("added by release ... not as changes of the domain") and the new CHANGELOG.md:10 claim that the diff "separates the profile's effect from the domain's". | Take a stored v3 run with DNS failing dnssec, caa, ns and ns_delegation (19/35) and every other check passing: overall 90.86, grade A. Re-scored under v4 it is still 90.86 and A, because `score_section` skips the three v4 ids the v3 run does not carry, and v4 keeps v3's other weights, section weights and thresholds. The first v4 run of the same, unchanged server adds `tls.legacy_versions` fail (TLS 1.0 was already accepted), `tls.weak_suites` warn (CBC) and `http.coop` warn (no header). TLS drops to 104.5/111 and HTTP to 23.5/24, overall 88.39, grade B. The result page renders "A → A (profile effect) → B (domain effect)" for a domain that changed nothing.
MINOR | crates/engine/tests/it/profile_v4.rs:89 | The test builds its expected value with the same `rescore` the code uses, on a run whose grade is the same under v3 and v4, so it cannot tell "re-scored under v4" from "re-scored under v3". | Change history.rs:62 to `rescore(prev, Profiles::get(prev.profile_version).unwrap())`. The prev run holds only `tls.tls_reachable` and `tls.tls_version`, both Pass, so it scores 100% (A+) under both profiles and the assertion still passes.
MINOR | crates/http/src/module.rs:455 | `http.coop` passes on a response that browsers treat as `unsafe-none`, because only the first header line is read (`headers.get`). The decision log defers this case. | A response sends `Cross-Origin-Opener-Policy: same-origin` twice. Fetch joins the lines into "same-origin, same-origin", the structured-field item parse fails, and the browser applies `unsafe-none`. netray reads only the first line and reports Pass.

```quote crates/engine/src/history.rs:62
        (prev.profile_version != cur.profile_version).then(|| rescore(prev, cur_profile)),
```

```quote crates/engine/tests/it/profile_v4.rs:89
        Some(rescore(&prev, Profiles::get(4).unwrap()))
```

```quote crates/http/src/module.rs:455
    let token = value.trim().split(';').next().unwrap_or("");
```

```quote crates/http/src/inspect/security.rs:158
    match headers.get(name) {
```

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=13 partial=0 missed=0 contradicted=0
LENSES requirements (Phases 1-3, R1-R13)

All 13 requirements met; no findings.

### Refuted

None.

### Calibration

| finding | refuter answer | confidence | outcome | command |
|---|---|---|---|---|
| MAJOR history.rs:62 grade split attributes the new checks to the domain | CONFIRMED | 8 | held | traced `score_section` (unweighted ids skipped) and `rescore(prev, v4)` on a run without the three ids: equal to its v3 grade, so the profile step equals `from` |

Remedy: the code is the re-scoring design (a stored run cannot be re-scored on checks it never measured); the false claims were corrected in SDD S41, `CHANGELOG.md` and the spec decision log, commit 01366b64. The two MINOR findings stay: the `profile_v4.rs` diff test cannot tell v3 from v4 re-scoring, and a second COOP header line is not seen (deferred in the decision log).

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | convergence | probe `Verdict` stays inside the tls crate and maps onto `netray_model::Status`; no codenames in the guide template |
| P10 | convergence | an unanswered probe gives `Unmeasured`; a missing or unreadable COOP warns, never passes |
| P12 | convergence | every probe exchange runs under `tokio::time::timeout`; no reqwest client added |
| P13 | absence | no limiter or route change in the range |
| P18 | convergence | raw sockets only to policy-admitted addresses; no `reqwest::Client`, no new config struct |
| P26 | convergence | each new check id has one emitter and one catalogue entry |
| P35 | absence | no `specs/rules/` change |
| P36 | convergence | goldens and island fixtures change with their sources; no hand-edited generated file |
| P40 | convergence | engine source change is `profile.rs` only; no module crate path in the engine |

### Summary

Before refutation: reader 0/1/2 (blockers/majors/minors), spec reader 0/0/0 (`REQUIREMENTS met=13 partial=0 missed=0 contradicted=0`). After refutation: reader 0/1/2, spec reader 0/0/0. verified 1, held 1 (the MAJOR, remedied in documentation by 01366b64). Roll call: 9 answered, 7 convergence, 0 divergence, 2 absence.
