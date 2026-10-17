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

## Phase 3 — The history is the cache

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R6: a hit is a head entry last seen less than `cache.ttl_seconds` ago, for `/api/check` without options, badge and OG; `[cache] enabled = false` or no store → no hit; `badges.ttl_seconds` only the limiter period; `OG_TTL_SECONDS` goes | green | crates/lens/tests/history_cache.rs, crates/lens/tests/history_cache_badge_og.rs |
| C2 | R7: `HistoryStore::head(domain)` returns id, `last_seen_at`, grade, profile version without a payload column | green | crates/store/tests/head.rs |
| C3 | R8: a V1 hit is answered from moka holding only the V1 rendering of the run that last recorded the head, keyed by entry id, TTL the window; `Identical` replaces it; only the head's id is read; `X-Cache: HIT`, `cached: true`, held snapshot id; `NotStored`/failed record hold nothing; `store_result`/`get_or_compute` go; badge/OG never read moka | green | crates/lens/tests/history_cache.rs |
| C4 | R9: an incomplete run is never a hit on any route; the head it did not replace stays the candidate; a complete recorded run is a hit within the window | green | crates/lens/tests/history_cache.rs, crates/lens/tests/history_cache_badge_og.rs |
| C5 | R10: badge renders a hit from the entry's grade, long Cache-Control only when fresh; after its run: complete → grade long; incomplete with head → head short; incomplete without → unscored short | green | crates/lens/tests/history_cache_badge_og.rs |
| C6 | R11: OG renders a hit from the entry (grade, `last_seen_at`, percentage scored under the entry's profile); new `netray_engine::history` function returns the `Score`, `rescore` its grade; after an incomplete run the head or the unscored card; Cache-Control as R10 | green | crates/engine/tests/score_run.rs, crates/lens/tests/history_cache_badge_og.rs |
| C7 | GIVEN a head last seen 10 s ago with its V1 rendering held WHEN `/api/check` THEN answered without a run, `X-Cache: HIT`, `cached: true`, held snapshot id | green | crates/lens/tests/history_cache.rs |
| C8 | GIVEN a head last seen 301 s ago (window 300) WHEN `/api/check` THEN a run starts | green | crates/lens/tests/history_cache.rs |
| C9 | GIVEN a fresh head but no held rendering (restart) WHEN `/api/check` THEN a run starts | already_implemented | crates/lens/tests/history_cache.rs |
| C10 | GIVEN a rendering held under the previous entry id and a newer head WHEN `/api/check` THEN a run starts, the previous rendering not served | green | crates/lens/tests/history_cache.rs |
| C11 | GIVEN `[cache] enabled = false` and a fresh head WHEN `/api/check` THEN a run starts | already_implemented | crates/lens/tests/history_cache.rs |
| C12 | GIVEN no store WHEN `/api/check` twice 61 s apart (tokio clock) THEN both start runs | green | crates/lens/tests/history_cache.rs |
| C13 | GIVEN a complete run that recorded `Identical` WHEN `/api/check` within the window THEN a hit with the newest run's rendering | green | crates/lens/tests/history_cache.rs |
| C14 | GIVEN a fresh head and a following incomplete run WHEN the next `/api/check` THEN not the incomplete run; the head's hit while fresh | green | crates/lens/tests/history_cache.rs |
| C15 | GIVEN an incomplete run and no entry WHEN any route asks within the window THEN none answers from that run | green | crates/lens/tests/history_cache.rs, crates/lens/tests/history_cache_badge_og.rs |
| C16 | GIVEN failing store writes WHEN a complete run finishes THEN the next request is no hit | green | crates/lens/tests/history_cache.rs |
| C17 | GIVEN a head graded B seen 10 s ago WHEN badge THEN B, long, no run | green | crates/lens/tests/history_cache_badge_og.rs |
| C18 | GIVEN a head graded B seen 2 h ago and the limiter exhausted WHEN badge THEN B, short | green | crates/lens/tests/history_cache_badge_og.rs |
| C19 | GIVEN a head graded B and a fresh run ending incomplete with a hard fail WHEN badge THEN B, short | green | crates/lens/tests/history_cache_badge_og.rs |
| C20 | GIVEN no entry and a fresh run ending incomplete WHEN badge THEN unscored, short | green | crates/lens/tests/history_cache_badge_og.rs |
| C21 | GIVEN no entry and a fresh complete run graded F WHEN badge THEN F, long | green | crates/lens/tests/history_cache_badge_og.rs |
| C22 | GIVEN a head seen 10 s ago WHEN OG THEN the entry's grade, its percentage under its profile, `last_seen_at`; no run | green | crates/lens/tests/history_cache_badge_og.rs |
| C23 | GIVEN a stored run WHEN scored under its own profile THEN the grade equals the entry's grade and `rescore` returns it | green | crates/engine/tests/score_run.rs |
| C24 | GIVEN `head` on a domain with three chained entries THEN the chain head's id, `last_seen_at`, grade, profile version; unknown domain → none | green | crates/store/tests/head.rs |

C9 and C11 held before the phase (a fresh state holds nothing; a disabled cache read nothing).

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1+G2 | 1 | sonnet | 29223 | 37 |
| G3 | 1 | sonnet | 62357 | 114 |

Existing tests whose meaning the spec changes were amended by a test-writer under `ADLC-Test-Change` (sdd_lens_badges_p1_c11 warm cache, admission_metrics c7/c11, incomplete_results c4/c7/c8 and the cached test, history_badge, state.rs holder TTL, og_routes hit and 304, badge_routes cache headers, client_hourly_runs c4): each seeds or waits for a head entry instead of a domain-keyed moka value.

### Review

- AMENDMENT | crates/lens/lens.example.toml:168 | the example still described `OG_TTL_SECONDS` and `badges.ttl_seconds` as freshness windows and advised a 3600 s cache window | affected_phase: 3 | repaired_in_phase: yes — comments now name `[cache] ttl_seconds` as the one window.
- DEFERRED | crates/lens/src/routes.rs:611 | a badge without a run result that renders the head counts no series | Phase 4 (requirement 12: `cache="throttled"`).

### Behavioural verification

```
$ netray lens <lens.dev.toml, no backend network: every run incomplete>
check 200 MISS
check 200 MISS            # an incomplete run is never a hit (R9)
badge 200 public, max-age=300, s-maxage=300
$ sqlite3 history.db "select count(*) from entries"
0
lens_badge_requests_total{cache="failed"} 1
lens_check_requests_total{result="fresh"} 2
```

## Phase 4 — Throttle, budget, slots

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R12: at most one run per domain starts per 60 s from run start, any trigger/option, tokio clock, in memory, pruned; refusal carries `retry_after_s`, `last_run_ago_s`, `last_incomplete`; `/api/check` 429 with `Retry-After`; badge/OG render the head short or `?`, counted `throttled` | green | crates/lens/tests/admission_throttle.rs |
| C2 | R13: a fresh visit/recheck/api run spends one token of a per-client GCRA (quota per hour, burst = quota); 0 builds none and refuses `budget` with 3600; hits and joins spend none; `/api/check` 429; badge/og never spend | green | crates/lens/tests/admission_budget.rs |
| C3 | R14: every fresh run holds one `domain_run_slots` permit for its whole run, taken without waiting; joins take none; none free → `slots`, `retry_after_s` 1; `/api/check` 429 `Retry-After: 1`; badge/OG as R12 | green | crates/lens/tests/admission_slots.rs |
| C4 | R15: order for `/api/check`: validate, `check_limits`, hit, `admit` (join, prefetch, throttle, slot, budget, start); badge/OG: validate, hit, `admit` (join, prefetch, recompute limiter, throttle, slot, start); a refusal spends nothing and starts no minute | green | crates/lens/tests/admission_slots.rs |
| C5 | R16: a `dkim_selectors` run is never a hit nor joins; needs limits, throttle, slot, budget; starts and is bound by the domain's throttle minute | green | crates/lens/tests/admission_throttle.rs |
| C6 | R17: `lens_admission_rejections_total{reason}` counts every refusal (throttle, budget, limit, slots), all registered at zero and checked by `test_lens_admission_metrics.sh`; a refused `/api/check` also counts `rate_limited` | green | crates/lens/tests/admission_slots.rs, tests/repo/test_lens_admission_metrics.sh |
| C7 | R20: `check_limits` and `admit` public in `admission.rs`; `Trigger`, `Refusal` as specified; `admit` never reads the window; `/api/check` passes `visit` | green | crates/lens/tests/admission_slots.rs |
| C8 | GIVEN a run that ended incomplete 10 s ago (paused clock) WHEN `/api/check` THEN 429 `Retry-After: 50`, `reason="throttle"` 1 | green | crates/lens/tests/admission_throttle.rs |
| C9 | GIVEN the same WHEN the clock reaches 60 s after the run started THEN `/api/check` starts a run | green | crates/lens/tests/admission_throttle.rs |
| C10 | GIVEN the same at 59 s THEN still 429 | green | crates/lens/tests/admission_throttle.rs |
| C11 | GIVEN a run started by the badge 10 s ago WHEN `/api/check` THEN throttled | green | crates/lens/tests/admission_throttle.rs |
| C12 | GIVEN a throttled domain with a head graded C WHEN badge THEN C short, `cache="throttled"`; without a head THEN `?` | green | crates/lens/tests/admission_throttle.rs |
| C13 | GIVEN a throttled domain WHEN `/api/check` for another domain THEN a run starts | green | crates/lens/tests/admission_throttle.rs |
| C14 | GIVEN budget 2 WHEN one client starts runs of three domains THEN the third 429 with `Retry-After`, `reason="budget"` 1 | green | crates/lens/tests/admission_budget.rs |
| C15 | GIVEN budget 1 and a fresh head for example.com WHEN the client asks example.com twice then example.net THEN both example.com hits, example.net runs | already_implemented | crates/lens/tests/admission_budget.rs |
| C16 | GIVEN budget 1 and a gated run of example.com by client X WHEN Y asks example.com then example.net THEN Y joins, then Y's example.net runs | already_implemented | crates/lens/tests/admission_budget.rs |
| C17 | GIVEN budget 1 WHEN two clients each start one run THEN both run | already_implemented | crates/lens/tests/admission_budget.rs |
| C18 | GIVEN budget 1 WHEN badges of three domains from one address THEN all three run | already_implemented | crates/lens/tests/admission_budget.rs |
| C19 | GIVEN slots 1 and a gated run of example.com WHEN `/api/check` example.net THEN 429 `Retry-After: 1`, `reason="slots"` 1; badge for example.net shows `?` | green | crates/lens/tests/admission_slots.rs |
| C20 | GIVEN slots 1 and a gated run of example.com WHEN example.com is asked THEN it joins | green | crates/lens/tests/admission_slots.rs |
| C21 | GIVEN slots 1 and a refused example.net WHEN the gate opens and example.net is asked again THEN it runs, not throttled, budget unchanged | green | crates/lens/tests/admission_slots.rs |
| C22 | GIVEN per-IP 1/1 and a fresh head WHEN one client asks twice THEN the second 429, `reason="limit"` 1 | green | crates/lens/tests/admission_slots.rs |
| C23 | GIVEN a dkim run of example.com 10 s ago WHEN dkim `/api/check` THEN 429; WHEN without options THEN throttled too | green | crates/lens/tests/admission_throttle.rs |
| C24 | GIVEN lens started WHEN `/metrics` before any request THEN the four rejection series and `joined` render 0 | green | crates/lens/tests/admission_slots.rs, tests/repo/test_lens_admission_metrics.sh |
| C25 | GIVEN a run ended incomplete 10 s after starting, 40 s ago WHEN `admit(recheck)` THEN `Refused{throttle, 10, Some(50), true}` | green | crates/lens/tests/admission_throttle.rs |
| C26 | GIVEN the same ended complete THEN `last_incomplete: false`; GIVEN a dkim run in flight THEN a run without options is refused `throttle` with `last_incomplete: false` | green | crates/lens/tests/admission_throttle.rs |
| C27 | GIVEN a head seen 10 s ago and no throttle record WHEN `admit(recheck)` THEN a run starts; `visit` too | already_implemented | crates/lens/tests/admission_throttle.rs |
| C28 | GIVEN budget 0 WHEN `admit(visit)` or `admit(recheck)` THEN `Refused{budget, 3600, None, false}`; `/api/check` with no entry THEN 429 `Retry-After: 3600` | green | crates/lens/tests/admission_budget.rs |
| C29 | GIVEN budget 0 WHEN badge of a domain without entry THEN a run starts; `/api/check` with a fresh head and held rendering THEN a hit | already_implemented | crates/lens/tests/admission_budget.rs |
| C30 | GIVEN per-IP 1/1 WHEN `check_limits` twice THEN `Refused{ip_limit}` with `retry_after_s`; global exhausted THEN `global_limit`; both count `reason="limit"` | green | crates/lens/tests/admission_slots.rs |
| C31 | GIVEN slots 1 and a gated run WHEN `admit` for another domain THEN `Refused{slots, 1, None, false}` | green | crates/lens/tests/admission_slots.rs |
| C32 | GIVEN the badge recompute limiter exhausted and a run in flight WHEN badge THEN it joins | green | crates/lens/tests/admission_slots.rs |

C15–C18, C27 and C29 held before the phase (nothing spent a budget, `admit` never read the window); they guard the reordered `admit`.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1+G2 | 3 | sonnet | 79704 | 203 |
| G2 (review repair) | 1 | sonnet | 28642 | 42 |

`admission_throttle_c12` failed on timing in the test: the badge arrived while the incomplete run's task was still recording, so it joined the run instead of meeting the throttle. The test now waits on `state.runs.drain()` after the first response (`ADLC-Test-Change`). Existing tests that meet the throttle (a second run of one domain within the minute) or the moved limiter were amended: a `pass_throttle()` helper advances tokio's clock by `THROTTLE`, other requests use another domain, the dkim join test now expects 429, and `check_rate_limit`'s unit tests read the `RateLimitRejection`.

### Review

- AMENDMENT | crates/lens/src/admission.rs:318 | the badge recompute token was spent before the throttle and slot checks, so their refusals burned it for `badges.ttl_seconds` | affected_phase: 4 | repaired_in_phase: yes — the limiter is taken in `gate` after throttle and slot; R15 and SDD §4.4 amended (Decision log, mandate).
- NIT | crates/lens/src/admission.rs:281 | the budget `retry_after_s` rounds the GCRA wait down (the per-IP and global limiters do the same).
- NIT | specs/sdd/v2.md:607 | the SDD names the prefetch rule that Phase 5 builds.

### Behavioural verification

```
$ netray lens <lens.dev.toml>; /api/check example.com, example.com, example.net
example.com 200
example.com 429 retry-after=44
example.net 200
lens_check_requests_total{result="rate_limited"} 1
lens_admission_rejections_total{reason="throttle"} 1
lens_admission_rejections_total{reason="budget"} 0
lens_admission_rejections_total{reason="limit"} 0
lens_admission_rejections_total{reason="slots"} 0
```
