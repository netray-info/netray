# Review: v2-ci-timeout

## 3ba0f1a9..c5fd20cc

### Reader

COUNTS blockers=0 majors=0 minors=1
LENSES Engineering, Testing
MINOR | CLAUDE.md:48 | After this change, the project CLAUDE.md still says the check job has a 30-minute timeout, and the range does not touch it. | At END, `.github/workflows/ci.yml:15` reads `timeout-minutes: 45` but `CLAUDE.md:48` still reads "25m51s of its 30-minute timeout".

```quote CLAUDE.md:48
- CI's check job compiles the workspace tests twice (offline suite, then `test-rust` with GeoIP data); a cold-cache run took 25m51s of its 30-minute timeout.
```

```quote .github/workflows/ci.yml:15
    timeout-minutes: 45
```

The workflow change itself is sound: one line, in the only job, the job-level key. No step-level timeout and no `timeout` wrapper in the justfile would still stop `just check` at 30 minutes. `tests/repo/test_workflows.sh` pins no timeout value and prints `ok` at END. The report.md criteria cells contain no `|`, every status is `test-unwritable`.

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=1 partial=0 missed=0 contradicted=0

```quote specs/features/v2-ci-timeout/spec.md:29
1. The `check` job in `.github/workflows/ci.yml` sets `timeout-minutes: 45`; nothing else in the
```

```quote .github/workflows/ci.yml:15
    timeout-minutes: 45
```

The workflow diff has one hunk, `timeout-minutes: 30` to `45` under `check:`, the only job. No other file outside `specs/features/v2-ci-timeout/` changed.

### Refuted

None: no BLOCKER or MAJOR finding, no refuter spawned.

### Calibration

None.

### Roll call

| principle | answer |
|---|---|
| P03 | absence: diff touches only ci.yml and the feature's specs |
| P10 | absence: no check or scoring path changed |
| P12 | absence: the CI job timeout is not an outbound call or engine deadline |
| P13 | absence: limiter setup not changed |
| P18 | absence: no Rust change |
| P26 | absence: no check ID added |
| P35 | convergence: no rule in specs/rules/ changed; the timeout is a tuning knob, not a shape rule |
| P36 | absence: no generated file changed |
| P40 | absence: no module added or removed |

### Summary

Before refutation 0/0/1, after 0/0/1 (reader 0/0/1; spec reader 0/0/0, REQUIREMENTS met=1 partial=0 missed=0 contradicted=0). verified 0, held 0. Roll call: answered 9, convergence 1, divergence 0, absence 8. The minor (CLAUDE.md:48 still names the 30-minute limit) goes to finish.
