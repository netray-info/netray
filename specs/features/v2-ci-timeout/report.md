# Report: v2-ci-timeout

## Phase 1 — Raise the check job's timeout

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: the check job in ci.yml sets timeout-minutes 45; nothing else in the workflow changes | test-unwritable | verified by the PR's CI run; the spec declines a repo check pinning the value |
| C2 | GIVEN ci.yml WHEN read THEN the check job's timeout-minutes is 45 | test-unwritable | `sed -n 15p .github/workflows/ci.yml` prints `    timeout-minutes: 45` |
| C3 | GIVEN ci.yml before and after WHEN diffed THEN the only changed line is timeout-minutes | test-unwritable | `git diff origin/main -- .github/workflows/ci.yml` shows one changed line |

No test writers spawned: the spec's only behaviour is the hosted runner's job limit, which no local
test reaches, and its decision log declines pinning a tuning value in `tests/repo/test_workflows.sh`.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 (one-line edit, made directly) | 0 | n/a | 0 | 0 |

### Behavioural verification

skipped: no local entry point; the check job's run on the PR and on main is the verification.
`sed -n 15p .github/workflows/ci.yml` → `    timeout-minutes: 45`.
