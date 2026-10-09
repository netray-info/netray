# Review: grade integrity

## c9a16f4..034cac2

### Reader

COUNTS blockers=0 majors=1 minors=2
LENSES Engineering, Security, Testing

MAJOR | crates/lens/src/routes.rs:1485 | A section that ran fine but has no entry in the scoring profile is reported `"error"` in the summary and in its own event (`routes.rs:1197`), so `overall` is `"error"` while `complete` is true and the grade a letter; at c9a16f4 both said the check-derived status. | `scoring.profile_path` to a profile without `[sections.http]`, spectra configured, `spectra-inspect.json` served: `summary.sections.http = "error"`, `overall = "error"`, the http event `status = "error"`, next to a letter grade.

```quote crates/lens/src/routes.rs:1485
                if !score.sections.contains_key(name)
```

```quote crates/lens/src/routes.rs:1197
        let has_possible = r.checks.iter().any(|c| {
```

MINOR | crates/lens/frontend/src/components/Summary.tsx:84 | The lens frontend no longer reaches its "?" state: the engine returns `incomplete` where it returned `error`, so the card prints "incomplete" in the letter slot and still offers "share & embed"; the badge preview tests only `'error'` too. | Every backend unreachable → grade `incomplete`, score 0: the card shows "incomplete" / "0%" instead of "? / Grade unavailable"; `/badge/<domain>.svg` serves `?`.

```quote crates/lens/frontend/src/components/Summary.tsx:84
  const isError = () => s().grade === 'error';
```

```quote crates/lens/src/scoring/engine.rs:162
            grade: "incomplete".to_string(),
```

MINOR | crates/lens/tests/deadlines.rs:259 | The test cannot fail on "send and stream share one budget": with two separate `timeout` calls in `email.rs` it still passes (headers arrive at once). | Replace both `timeout_at(deadline, …)` in `email.rs` with `timeout(timeout, …)`: elapsed ≈ 1.0 s < 1.6 s, green.

```quote crates/lens/tests/deadlines.rs:259
async fn email_send_and_stream_share_one_timeout_budget() {
```

### Refuted

None. The MAJOR was CONFIRMED (confidence 8): `ScoringProfile::from_toml` validates only hard-fail entries, `compute_score` iterates `profile.sections`, and both status paths gave `"error"` for an Ok section absent from the profile.

### Calibration

| Finding | Refuter | Confidence | Result | Command |
|---|---|---|---|---|
| `routes.rs:1485` unprofiled section reported error | CONFIRMED | 8 | held | `cargo test -p lens --lib summary_section_absent_from_profile_is_not_error` red at `beddffa` (`http` "error", expected "warn") |

Repaired on this branch: `beddffa` (tests), `fix(grade-integrity)` commit after it: an Ok section absent from the profile keeps its check-derived status (inferred from empty weights; a profiled section with an empty checks table reads as absent, and with an incomplete result an unprofiled section can still show "error" — both custom-profile corners, production's embedded profile has all five sections); the lens UI's `Summary` and `GradeBadgePreview` treat `incomplete` like `error` (the first MINOR). The second MINOR (`deadlines.rs:259` cannot tell one budget from two) stays.

### Roll call

| Principle | Answer | Evidence |
|---|---|---|
| P03 | convergence | no new status enum; no codename in UI copy or API output (only `service = "spectra"` in log fields) |
| P10 | convergence | errors, timeouts, unknown verdicts and the hard deadline lead to Errored/Timeout → incomplete; `tls_reachable` never passes a failure |
| P12 | convergence | one `timeout_at` per backend call; `run_wave` under the hard deadline keeps finished sections; budget check at config load |
| P13 | absence | no limiter or route change |
| P18 | convergence | no new `reqwest::Client`; one target policy; `test_config_strict.sh` green |
| P26 | convergence | `tls_reachable` has one emitter (`check_tls_reachable`); the duplicate hsts/https_redirect copy is gone |
| P35 | absence | no `specs/rules/` change |
| P36 | convergence | `site/sitemap.xml` changes with `site/api/lens.html`; goldens with their generators |
| P40 | absence | no module added or removed |

### Summary

Before refutation 0/1/2, after 0/1/2. verified 1, held 1 (repaired on the branch). Roll call: 9 answered, 6 convergence, 0 divergence, 3 absence.
