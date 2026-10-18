# Bug: admission-prefetch-race

Written by `adlc feature start --bug`. The first four sections are yours; diagnose fills the last three.

## Symptom

lens test `admission_prefetch_c13_admit_joins_or_answers_not_started_never_refuses`
(`crates/lens/tests/admission_prefetch.rs`) fails intermittently, about 1 run in 7 of the whole
test file: after the first run's stream has ended, the "throttled" prefetch `admit` is expected to
answer `NotStarted` and instead answers `Admitted` (it joined the finished run).

## Expected

Once a run's stream has ended on `Recorded`, `admit` with `prefetch = true` for that domain answers
`NotStarted` (and a normal admit meets the throttle), every time.

## Reproduction

cargo test -p lens --test admission_prefetch   (repeat; red about 1 in 7 under parallel tests)

## Environment

main at 309c6611, macOS, multi-thread tokio runtime; the race needs the run's task and the reader
on different worker threads.

## Loop

The reporter's reproduction is a race and is not red on demand: `admission_prefetch_c13` alone,
100 runs, 0 failures; the whole file, 30 runs, 2 failures; a scratch loop of 200 start/finish/prefetch
rounds, red 1 time in 40 file runs. A deterministic loop needs a seam inside the crate: a unit test
in `crates/lens/src/admission.rs` (`mod tests`) that holds the run register's lock (`state.runs.lock()`,
private) while it reads the run's stream. The run's task cannot deregister while the lock is held,
so whether `Recorded` can be observed while the run is still registered is decided by the order of
the two statements, not by scheduling. The probe (removed again after diagnosis):

```rust

    /// A run whose stream has ended on `Recorded` is no longer in the register, so a prefetch
    /// that comes after it gets `NotStarted` instead of joining the finished run. The test holds
    /// the register's lock while it reads the run: the run's task cannot deregister meanwhile, so
    /// the order of deregistration and `Recorded` is observed, not raced.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    // Holding the register's std mutex across the reads is the point: it blocks deregistration.
    #[allow(clippy::await_holding_lock)]
    async fn recorded_follows_deregistration() {
        let mut state =
            AppState::new(crate::routes::tests::test_config_with_rate_limit(1000, 1000)).unwrap();
        state.badge_check_fn = Some(Arc::new(|domain: String| {
            Box::pin(async move {
                CheckOutput {
                    domain,
                    sections: HashMap::new(),
                    score: crate::score::OverallScore {
                        sections: HashMap::new(),
                        overall_percentage: 80.0,
                        grade: "B".to_string(),
                        hard_fail_triggered: false,
                        hard_fail_checks: vec![],
                        not_applicable: HashMap::new(),
                        complete: true,
                    },
                    duration_ms: 1,
                }
            })
        }));
        let client = IpAddr::from([127, 0, 0, 1]);
        let Admit::Admitted(mut stream) = admit(&state, "example.com", client, Trigger::Badge, false)
        else {
            panic!("the first admit starts a run");
        };

        let register = state.runs.lock();
        let recorded = tokio::time::timeout(Duration::from_millis(500), async {
            while let Some(event) = stream.next().await {
                if matches!(event, RunEvent::Recorded(_)) {
                    return true;
                }
            }
            false
        })
        .await
        .unwrap_or(false);
        let registered = register.contains_key(&cache_key("example.com"));
        drop(register);
        assert!(
            !(recorded && registered),
            "the stream ended on Recorded while the run was still joinable"
        );

        while stream.next().await.is_some() {}
        assert!(
            matches!(
                admit(&state, "example.com", client, Trigger::Badge, true),
                Admit::NotStarted
            ),
            "a prefetch after the run's end: NotStarted"
        );
    }
```

Command, run twice from the repository root:

```
CARGO_INCREMENTAL=0 cargo test -p lens --lib admission::tests::recorded_follows_deregistration
```

Run 1:

```
thread 'admission::tests::recorded_follows_deregistration' (18985995) panicked at crates/lens/src/admission.rs:586:9:
the stream ended on Recorded while the run was still joinable
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 178 filtered out; finished in 0.20s
```

Run 2:

```
thread 'admission::tests::recorded_follows_deregistration' (18992672) panicked at crates/lens/src/admission.rs:586:9:
the stream ended on Recorded while the run was still joinable
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 178 filtered out; finished in 0.20s
```

## Hypotheses

1. **held** — `run_task` deregisters only when it returns (the `Deregister` guard's drop), after
   `tx.send_modify(|s| s.recorded = ...)`, so a caller can see `Recorded` while the run is still in the
   register. Refuted if moving the deregistration before the `Recorded` publication leaves the loop
   red. Probe: `drop(deregister)` before that `send_modify`; the loop ran green twice
   (`test result: ok. 1 passed`, 0.71 s: the 500 ms bound elapsed because `Recorded` now waits on
   the held lock, as it must).
2. **refuted** — the prefetch reaches `gate` and passes because the throttle record is missing.
   Refuted by a read: `admit_uncounted` returns `NotStarted` for a prefetch before `gate` runs
   (`if prefetch { return Admit::NotStarted; }`); the `Admitted` answer can only come from the
   register lookup above it.
3. **refuted** — a newer run under the same key replaces the finished one, and the prefetch joins
   that newer run. Refuted by the loop: it starts one run only, and the register still held a key
   for it after `Recorded` (`registered == true` in the failing assertion); the c13 failure's run
   count stays 1.
4. **refuted** — `cache::hold` or `history::record` publishes `Recorded` early. Refuted by a read:
   both are awaited before the one `send_modify` that sets `recorded`; no other write to `recorded`
   exists (`grep -n "recorded =" crates/lens/src/admission.rs`: one hit).

## Root cause

The run's task publishes `Recorded` and only then, when it returns, drops its `Deregister` guard,
which takes the run out of the register. A reader that has consumed the whole stream can call
`admit` in that gap; `admit_uncounted` finds the run still registered and joins it, so a prefetch
gets `Admitted` (a joined, already ended stream) instead of `NotStarted`.

```quote crates/lens/src/admission.rs:462
    let _deregister = key.map(|key| Deregister {
```

```quote crates/lens/src/admission.rs:505
    tx.send_modify(|s| s.recorded = Some(recorded));
```

```quote crates/lens/src/admission.rs:360
    if let Some(rx) = runs.get(&key) {
```
