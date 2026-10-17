# Report: v2-admission

## Phase 1 — Admission config

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 / GIVEN a config without `[admission]` WHEN it loads THEN `client_budget_per_hour` is 30 and `domain_run_slots` is 8 | green | crates/lens/tests/admission_config.rs |
| C2 | GIVEN `[admission] domain_run_slots = 0` WHEN it loads THEN it is refused with a message naming `admission.domain_run_slots` | green | crates/lens/tests/admission_config.rs |
| C3 | GIVEN `client_budget_per_hour = 0`, `= 1` or `domain_run_slots = 1` WHEN it loads THEN it is accepted | green | crates/lens/tests/admission_config.rs |
| C4 | GIVEN `LENS_ADMISSION__CLIENT_BUDGET_PER_HOUR=0` WHEN the config loads THEN `client_budget_per_hour` is 0 | green | crates/lens/tests/admission_config.rs |
| C5 | GIVEN an unknown key under `[admission]` WHEN the config loads THEN it is rejected | green | crates/lens/tests/admission_config.rs |
| C6 | GIVEN `lens.production.toml` (with `[admission]`) WHEN `--check-config` runs THEN exit 0; the `domain_run_slots = 0` startup_rejects row exits 1; `client_budget_per_hour = 0` exits 0 | green | tests/repo/test_check_config.sh |
| C7 | GIVEN a config without `[rate_limit]` WHEN it loads THEN the limits are 10/3/100/20, unchanged | green | crates/lens/tests/admission_config.rs |

C7 held before the phase (defaults 10/3/100/20); its row sits in the same test, which failed to compile until `Config.admission` existed.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 30929 | 91 |

The coder also added `admission: AdmissionConfig::default()` to nine integration-test `Config` literals (one line each, nothing else changed); the plan names them, and they compile only with the field.

### Review

- AMENDMENT | specs/sdd/v2.md:60 | S33 still called the four numbers `netray.toml` values while Phase 1 makes them lens config keys | affected_phase: 1 | repaired_in_phase: yes — S33 amended in this phase's commit (Decision log, mandate), the stale `lens.production.toml:45-49` citation corrected to `:40-44`.

### Behavioural verification

```
$ ./target/debug/netray lens --check-config crates/lens/tests/fixtures/lens.production.toml
config ok: crates/lens/tests/fixtures/lens.production.toml   (rc=0)
$ ./target/debug/netray lens --check-config <fixture with domain_run_slots = 0>
config error: …: invalid configuration: admission.domain_run_slots must not be zero   (rc=1)
```
