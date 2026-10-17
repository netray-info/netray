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

## Phase 2 — One run per domain, recorded off the request path

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R2: every run without options goes through one per-domain coordinator keyed by `cache_key`; a request for a domain in flight joins it (same `CheckOutput`, facts blocks, snapshot id) and starts none; `dkim_selectors` runs never join nor are joined | green | crates/lens/tests/admission_join.rs |
| C2 | R3: the run executes in a spawned task that outlives every requester; forwards section events; snapshot when complete; builds `Run` once; publishes; then records (moved out of `check.rs`); publishes the record; then deregisters; a request between publish and deregistration joins; no V1, badge or OG response waits on the store | green | crates/lens/tests/admission_record.rs |
| C3 | R4: runs injected through `badge_check_fn` go through the same task and are recorded, without facts blocks | green | crates/lens/tests/admission_record.rs |
| C4 | R5: a joining request counts `result="joined"` (registered at zero, checked by `test_lens_admission_metrics.sh`); `fresh` only for requests that started a run; for `/api/check` runs the task holds `RunGuard` and `observe_run` | green | crates/lens/tests/admission_join.rs, crates/lens/tests/admission_metrics.rs, tests/repo/test_lens_admission_metrics.sh |
| C5 | R21: a `RunStream` yields `Section` per finished section (joiner: finished first, then live), one `Done` (output, facts, snapshot id, `Run`), one `Recorded(Option<Recorded>)`, then ends; dropping it never cancels the run | green | crates/lens/tests/admission_stream.rs |
| C6 | GIVEN a gated `/api/check` run of example.com WHEN a second `/api/check` arrives before the gate opens THEN one run executes, same section payloads and snapshot id, `joined` 1, `fresh` 1 | green | crates/lens/tests/admission_join.rs |
| C7 | GIVEN the same gated run WHEN a badge request arrives THEN it joins: one run executes | green | crates/lens/tests/admission_join.rs |
| C8 | GIVEN a gated run of example.com WHEN `/api/check` for example.net arrives THEN a second run executes | already_implemented | crates/lens/tests/admission_join.rs |
| C9 | GIVEN a gated run without options WHEN a `dkim_selectors` request arrives THEN a second run executes; a request without options during the dkim run does not join it | already_implemented | crates/lens/tests/admission_join.rs |
| C10 | GIVEN a gated run whose requester's future is dropped WHEN the gate opens THEN the run completes and `history.db` holds its entry; `lens_runs_in_flight` is 1 until the run ends, then 0 | green | crates/lens/tests/admission_record.rs |
| C11 | GIVEN the store's write lock held by the test WHEN a complete run finishes THEN the response arrives before the lock is released | green | crates/lens/tests/admission_record.rs |
| C12 | GIVEN a published result whose record is pending WHEN a request for the domain arrives THEN it gets the published result and no second run executes | green | crates/lens/tests/admission_record.rs |
| C13 | GIVEN a `badge_check_fn` mock returning a complete run WHEN the badge route runs it THEN `history.db` holds one entry | green | crates/lens/tests/admission_record.rs |
| C14 | GIVEN a run with `dkim_selectors` WHEN it completes THEN `history.db` holds no new entry | already_implemented | crates/lens/tests/admission_record.rs |
| C15 | GIVEN two sequential `/api/check` requests with no run in flight between them THEN `fresh` 2, `joined` 0 | already_implemented | crates/lens/tests/admission_join.rs |
| C16 | GIVEN a five-section run whose DNS and TLS sections have finished WHEN a caller joins through `admit` THEN its stream yields DNS and TLS first, then the other three, then `Done`, then `Recorded` | green | crates/lens/tests/admission_stream.rs |
| C17 | GIVEN a started run WHEN its stream is read to the end THEN exactly one `Section` per configured section, one `Done`, one `Recorded`, in that order | green | crates/lens/tests/admission_stream.rs |
| C18 | GIVEN the store's write lock held WHEN a complete run finishes THEN the stream yields `Done` before release and `Recorded(Some(First))` after | green | crates/lens/tests/admission_stream.rs |
| C19 | GIVEN an incomplete run THEN `Recorded` carries `NotStored`; no store THEN none; failing writes THEN none | green | crates/lens/tests/admission_stream.rs |
| C20 | GIVEN a `Done` from `admit` with trigger `recheck` WHEN compared with the `Run` the store recorded THEN equal | green | crates/lens/tests/admission_stream.rs |

C8, C9, C14 and C15 held before the phase; their tests stay as regression guards.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 77456 | 172 |
| G1 (review repairs) | 1 | sonnet | 31013 | 49 |

The one red test after the coder's run, `admission_c12_…`, was a test defect (it parsed an SSE body as JSON, then compared hash-ordered JSON as text); it was corrected to compare the SSE data lines as JSON, under `ADLC-Test-Change`. `history_record.rs` c1_c8, c3_c9, c5 and c12 were amended for the record that now follows the response (bounded polls; c12 reads `Recorded(NotStored)` through `admit`), c10 and c11 wait before asserting absence.

### Review

- AMENDMENT | crates/lens/src/admission.rs:175 | the spawned run task lost the request's tracing span (logging-rules C1) | affected_phase: 2 | repaired_in_phase: yes — the task runs under the starting request's span (Decision log, mandate).
- AMENDMENT | crates/lens/src/admission.rs:265 | graceful shutdown dropped pending run tasks and their records | affected_phase: 2 | repaired_in_phase: yes — `Runs::drain` on shutdown, bounded by the hard deadline + 10 s (Decision log, mandate).
- DEFERRED | crates/lens/src/routes.rs:590 | the badge recompute limiter still precedes the join | Phase 4 (requirement 15).
- DEFERRED | crates/lens/src/routes.rs:807 | the `Refused` branch counts neither `rate_limited` nor a rejection reason | Phase 4 (requirement 17).
- NIT | crates/lens/src/routes.rs:606 | a badge run that ends without a result counts `throttled`, not `failed`.

### Behavioural verification

```
$ netray lens <lens.dev.toml on free ports>; two concurrent
  curl /api/check/example.com?stream=false
b 200 MISS
a 200 MISS
$ curl :metrics/metrics
lens_check_requests_total{result="joined"} 1
lens_check_requests_total{result="fresh"} 1
lens_runs_in_flight 0
```
