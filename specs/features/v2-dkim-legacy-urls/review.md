
## e4200ce27beb64441af5cf2c7a21707ab8054f7c..c6e2df62723a993f7864794078adfbe61ebb5075

### Reader

COUNTS blockers=0 majors=0 minors=3
LENSES Engineering, Security, Testing
MINOR | crates/web/tests/it/result_selectors.rs:233 | Test C7 cannot fail when a visit with an empty `dkim_selectors=` skips the fresh-history answer, which is the plain-path behaviour the test is named for. | Mutate `crates/web/src/pages/result/run.rs:89` to `if trigger == RunTrigger::Visit && raw_selectors.is_none()`. Then `trigger=visit&dkim_selectors=` over a fresh head skips `stored_answer` and calls `start_with(&[])`. The stub records `(Visit, Some([]))`, the stream answers 200, and the assertion still passes.
MINOR | crates/web/src/selectors.rs:19 | The page and API limit is fixed at 5 and ignores the configured `[modules.email.dkim] max_user_selectors`. With a configured value below 5, a list the email module refuses is admitted as a run instead of answering 400. | `max_user_selectors = 3`: `POST /api/v1/domains/example.com/runs?dkim_selectors=a,b,c,d` spends a budget token, a slot and the throttle minute, and the email section ends incomplete. The spec's Decision log defers this case ("production sets 5").
MINOR | specs/sdd/v2.md:946 | The SDD rows for `?d=` keeping `dkim_selectors`, and for the result page starting runs through `start_with`, landed in e22a61e2; the code they describe arrived in c6e2df62. | A bisect landing on e22a61e2 reads a contract the code does not meet. The report lists this as DEFERRED.

```quote crates/web/tests/it/result_selectors.rs:233
                .all(|(_, s)| s.as_ref().is_none_or(|v| v.is_empty())),
```

```quote crates/web/src/selectors.rs:19
    if tokens.len() > MAX_DKIM_SELECTORS {
```

```quote specs/sdd/v2.md:946
| `netray.info/?d=<domain>` (lens's query parameter) | `netray.info/<domain>`; a non-empty `dkim_selectors` is kept as `?dkim_selectors=` | 301 |
```

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=12 partial=0 missed=0 contradicted=0

All 12 requirements traced through the code at c6e2df62 with no divergence. Two behaviours beyond the numbered requirements, not counted: the canonical-domain redirect keeps the whole query (named in the Phase 3 decision log), and `query_redirect` trims the selector value (matching the target page's trimming).

### Refuted

None: no BLOCKER or MAJOR finding, no refuter spawned.

### Calibration

No refuter answers.

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | convergence | no codename in templates, pages or the OpenAPI fixture; `"lens"` only as a legacy slug mapped to `/guide/scoring`; no new status enums |
| P10 | absence | no new error or timeout branch; selector parse errors are 400s, not verdicts |
| P12 | absence | no new outbound client or engine entry; the selector run reuses `admit_dkim` |
| P13 | convergence | `start_with` passes `check_limits`, slot and budget; email inspect keeps its limiter and slots |
| P18 | absence | no client constructions or config structs added |
| P26 | convergence | no new check IDs; DKIM still emitted by the email module only |
| P35 | convergence | the `start_with` override rule arrives with `tests/repo/test_domain_runs_selectors.sh` |
| P36 | convergence | the OpenAPI golden changed with its utoipa sources |
| P40 | absence | no module added or removed |

### Summary

Before refutation: reader 0/0/3, spec reader 0/0/0. After refutation: unchanged, blockers 0, majors 0, minors 3 (reader); spec reader `REQUIREMENTS met=12 partial=0 missed=0 contradicted=0`. verified 0, held 0. Roll call: 9 answered, 5 convergence, 0 divergence, 4 absence.
