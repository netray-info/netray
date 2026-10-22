# Report: v2-release-cleanup

## Phase 1 — Image-run budget

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: badge and OG fresh runs take a token of `[admission] image_runs_per_minute` (default 10), a bucket refilled continuously; join, hit and prefetch take none | green | crates/web/tests/it/admission_image_budget.rs |
| C2 | R2: an empty budget answers the head entry or the unscored image, status 200; counted as `image_budget`, registered at zero | green | crates/web/tests/it/admission_image_budget.rs, tests/repo/test_serve_admission_metrics.sh |
| C3 | R3: checked after throttle, slot and recompute limiter, spent only on start; visits never touch it | green | crates/web/tests/it/admission_image_budget.rs |
| C4 | R4: zero refused at load, listed in startup_rejects; example shows the default | green | crates/netray/tests/serve_image_budget.rs, tests/repo/test_check_config.sh |
| C5 | R5: SDD S44 and section 4.4 and the CHANGELOG name the budget | green | specs/sdd/v2.md (this phase), CHANGELOG.md (prose commit at the end) |
| C6 | three distinct badge domains at budget 2: the third is refused, unscored badge, short Cache-Control, counter 1 | green | crates/web/tests/it/admission_image_budget.rs |
| C7 | budget spent, stale head: badge renders the head grade, short Cache-Control | green | crates/web/tests/it/admission_image_budget.rs |
| C8 | budget spent: OG card without head answers 200 image/png | green | crates/web/tests/it/admission_image_budget.rs |
| C9 | after 30 s one more run starts | green | crates/web/tests/it/admission_image_budget.rs |
| C10 | a join takes no token | green | crates/web/tests/it/admission_image_budget.rs |
| C11 | a prefetch takes no token | green | crates/web/tests/it/admission_image_budget.rs |
| C12 | a throttle refusal spends no token | green | crates/web/tests/it/admission_image_budget.rs |
| C13 | a budget refusal spends no recompute token | green | crates/web/tests/it/admission_image_budget.rs |
| C14 | a visit starts with the budget spent | green | crates/web/tests/it/admission_image_budget.rs |
| C15 | zero exits 1 naming admission.image_runs_per_minute | green | crates/netray/tests/serve_image_budget.rs |
| C16 | default 10; env 5 accepted | green | crates/netray/tests/serve_image_budget.rs |
| C17 | the zero series image_budget renders at startup | green | tests/repo/test_serve_admission_metrics.sh |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 admission, config, metrics | 1 | implementer (no coder agent: one group, written in place) | n/a | n/a |

Tests and code were written by the implementer directly, under the operator's autonomy mandate
(2026-10-22); the independent reading of the whole range runs in the review step.

### Behavioural verification

`netray serve` with `[admission] image_runs_per_minute = 1`, no modules, then
`/badge/example.com.svg`, `/badge/example.net.svg`, `/og/example.org.png` on `Host: netray.info`:

```
example.com 200 image/svg+xml; charset=utf-8
example.net 200 image/svg+xml; charset=utf-8
og 200 image/png
netray_runs_total{outcome="incomplete",trigger="badge"} 1
netray_admission_rejections_total{reason="image_budget"} 2
```
