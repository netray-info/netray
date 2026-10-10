# Report: lens admission metrics

## Phase 1 — Request, run and zero series

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: `lens_check_requests_total{result}` counts each validated check request once (rate_limited, cache_hit, fresh) | green | crates/lens/tests/admission_metrics.rs |
| C2 | R2: `lens_runs_in_flight` gauge around fresh runs (also on drop); `lens_run_duration_seconds` histogram with buckets 0.5 1 2 5 10 15 20 30 | green | crates/lens/tests/admission_metrics.rs |
| C3 | R4: zero series at startup for check requests (3), badge requests (4), rate-limit hits (4), runs in flight | green | crates/lens/tests/metrics_zero_series.rs |
| C4 | R5: common installs the recorder with per-metric buckets and an after-install hook; services without them unchanged | green | crates/common/tests/metrics_recorder.rs, tests/repo/test_lens_admission_metrics.sh |
| C5 | R6: lens results, goldens and existing metrics unchanged | already_implemented | existing (crates/lens/tests/lens_golden.rs, results tables) |
| C6 | fresh request → result="fresh" 1, others 0 | green | crates/lens/tests/admission_metrics.rs |
| C7 | repeat within TTL → result="cache_hit" 1 | green | crates/lens/tests/admission_metrics.rs |
| C8 | per-IP limit 1, two requests → result="rate_limited" 1 | green | crates/lens/tests/admission_metrics.rs |
| C9 | invalid domain → no lens_check_requests_total increment | green | crates/lens/tests/admission_metrics.rs |
| C10 | in-flight 1 during a held run, 0 after; 0 after the handler future is dropped | green | crates/lens/tests/admission_metrics.rs |
| C11 | one duration observation per fresh run, rendered with R2's buckets, not as a summary | green | crates/lens/tests/admission_metrics.rs, tests/repo/test_lens_admission_metrics.sh |
| C12 | `/metrics` before any request lists the zero series of R4 | green | crates/lens/tests/metrics_zero_series.rs, tests/repo/test_lens_admission_metrics.sh |
| C13 | a service using `serve_metrics` as today renders as before | green | crates/common/tests/metrics_recorder.rs |
| C14 | lens goldens and results tables unchanged | already_implemented | existing (crates/lens/tests/lens_golden.rs, results tables) |

RED: admission_metrics.rs fails 7/7 (no series recorded; C9 through its control request); metrics_zero_series.rs and metrics_recorder.rs fail to compile (`lens::metrics`, `server::metrics_recorder` missing).

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 26022 | 30 |
| G2 | 3 | sonnet (4 tests red from a test defect: metrics-util 0.20's `snapshot()` swaps to 0 on read; the test writer moved reads to an accumulating helper) | 38792 | 68 |
| G3 | 0 | already_implemented | 0 | 0 |

### Reader

| class | at | finding | outcome |
|---|---|---|---|
| AMENDMENT | crates/lens/src/lib.rs:193 | no test covered the startup wiring: reverting to `serve_metrics`, or running `after_install` before the install, kept every test green | repaired in phase: `tests/repo/test_lens_admission_metrics.sh` starts `netray lens` with closed backends and checks the zero series, one fresh run and its buckets |
| DEFERRED | crates/lens/src/routes.rs:930 | badge and OG misses run checks (`invoke_badge_check`, `og/handler.rs:54`) outside `lens_runs_in_flight` and `lens_run_duration_seconds` | per spec: R2 covers the check handler; V2 serves badge and OG from the stored entry |

### Behavioural verification

```
$ netray lens <dev config, snapshots in a temp dir>; curl -s localhost:9095/metrics | grep lens_
lens_check_requests_total{result="cache_hit"} 0
lens_check_requests_total{result="fresh"} 0
lens_check_requests_total{result="rate_limited"} 0
lens_badge_requests_total{cache="failed"} 0
lens_badge_requests_total{cache="hit"} 0
lens_badge_requests_total{cache="miss"} 0
lens_badge_requests_total{cache="throttled"} 0
lens_rate_limit_hits_total{scope="badge"} 0
lens_rate_limit_hits_total{scope="per_ip"} 0
lens_rate_limit_hits_total{scope="global"} 0
lens_rate_limit_hits_total{scope="og"} 0
lens_runs_in_flight 0
$ curl -s -o /dev/null -w '%{http_code}' localhost:8085/api/check/not_a_domain   # passes validation
200
lens_check_requests_total{result="fresh"} 1
```
