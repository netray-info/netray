# Review: v2-release-cleanup

## f7934cd1..39b8b536

### Reader

COUNTS blockers=0 majors=0 minors=1
LENSES Engineering, Security, Testing
MINOR | crates/web/tests/it/admission_image_budget.rs:39 | The three tests named for the badge and OG refusal path still pass with the image budget deleted, so the 200 / `?` / short Cache-Control answer to a budget refusal (R2, report C6–C8) is never actually exercised. | The helper `state()` registers only `common::dns_incomplete()`, so any run that does start ends incomplete. Without the budget block in `gate`, `GET /badge/example.net.svg` is admitted, the run ends incomplete and the handler answers `badge_response(stale.unwrap_or("incomplete"), false)`, which renders `?` with `max-age=300`; with the seeded head it renders `B`; the OG handler answers 200 image/png. None of the three asserts the run was refused.

```quote crates/web/tests/it/admission_image_budget.rs:39
        Registry::new().with(common::dns_incomplete()),
```

```quote crates/web/src/pages/badge.rs:161
        stale.unwrap_or("incomplete"),
```

```quote crates/web/src/badge/mod.rs:15
    grade == "error" || grade == "incomplete"
```

```quote crates/web/src/pages/og.rs:163
    let card = head_card.as_ref().unwrap_or(&run_card);
```

Traced and sound: the budget order in `gate` (peek, recompute limiter, take, under the throttle mutex), joins and prefetches before `gate`, visits never touching it; `count_rejection` and the zero series; the bucket maths; the config (`deny_unknown_fields`, zero refused, `apex_config`, the `startup_rejects` needle); label and logo (`frame.origin()`; gone.html overrides the header); the legacy test over every `Now` row; crate docs spot-checked for dns, tls, http, email, ip, common.

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=11 partial=0 missed=0 contradicted=0

All 11 requirements traced at 39b8b536. Notes without findings: with the budget empty, a request the recompute limiter would also refuse counts as `image_budget` (the order requirement 3 prescribes); each CHANGELOG pointer sits after the Keep a Changelog preamble, before the first entry; `crates/netray/tests/agreement.rs` hard-codes `image_runs_per_minute: 20` in a test helper; `serve.rs` logs the key at startup.

### Refuted

None: no BLOCKER or MAJOR, so no refuter ran.

### Calibration

No refuter answers.

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | convergence | the badge and OG label lens becomes netray; no codename added under crates/web |
| P10 | absence | no check or scoring path changed |
| P12 | absence | no outbound client or module entry call added |
| P13 | convergence | the image budget is its own limiter for badge and OG runs; domain runs and API keep theirs |
| P18 | convergence | no reqwest client added; test_config_strict.sh passes with the new key |
| P26 | absence | no check id added |
| P35 | absence | no specs/rules file changed |
| P36 | absence | no generated file changed |
| P40 | absence | no module added or removed |

answered 9, convergence 3, divergence 0, absence 6; no unproven line.

### Summary

Before refutation: reader 0/0/1, spec reader 0/0/0 (REQUIREMENTS met=11 partial=0 missed=0 contradicted=0). After refutation: the same, 0 blockers, 0 majors, 1 minor. verified 0, held 0. Roll call: answered 9, convergence 3, divergence 0, absence 6.
