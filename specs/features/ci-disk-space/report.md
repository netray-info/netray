# Report: ci-disk-space

## Phase 1 — Bound the check job's disk use

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | Req 1: loop command prints `success` for the branch head and the merge commit; no `No space left on device` | test-unwritable | (PR and main CI runs; no local test reaches the hosted runner's disk) |
| C2 | Req 2: check job env `CARGO_PROFILE_DEV_DEBUG: line-tables-only` | green | tests/repo/test_workflows.sh |
| C3 | Req 3: step before `just adlc-setup` removes dotnet, ghc, android and prints `df -h /` | green | tests/repo/test_workflows.sh |
| C4 | Req 4: justfile unchanged; `test-rust` takes its GeoIP branch and ifconfig-rs results show in the log | test-unwritable | (PR CI run log) |
| C5 | Scenario: missing debuginfo bound fails test_workflows.sh | green | tests/repo/test_workflows.sh |
| C6 | Scenario: missing disk step fails test_workflows.sh | green | tests/repo/test_workflows.sh |
| C7 | Scenario: PR CI check job green, `df -h /` twice, netray-ip integration results | test-unwritable | (PR CI run log) |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 ci.yml (C2, C3, C5, C6) | 1 (implementer, no coder: a four-line workflow change) | implementer | - | - |

### Review

- NIT | .github/workflows/ci.yml:29 | on `ubuntu-24.04-arm` `/opt/ghc` and `/usr/local/lib/android` are absent (partner image omits Haskell and Android); only `/usr/share/dotnet` frees space. Kept: harmless, and Req 3 names the paths.
- Note: `timeout-minutes: 30` is tight with a cold cache; the PR run's timing decides.

### Behavioural verification

skipped: the entry point is the hosted runner; C1, C4 and C7 are verified on the PR's CI run before finish.
