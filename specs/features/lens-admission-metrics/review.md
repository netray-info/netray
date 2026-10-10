# Review: lens admission metrics

## 73c3f58..d4d1b69

### Reader

COUNTS blockers=0 majors=0 minors=1
LENSES Engineering, Security, Testing
MINOR | crates/lens/src/routes.rs:33 | Badge and OG misses run `run_check` without the run guard or duration observation. | A scrape during an uncached `/badge/<domain>.svg` run shows `lens_runs_in_flight 0`.

```quote crates/lens/src/routes.rs:33
        run_check(state, domain).await
```

### Summary

0/0/1, nothing to refute. The minor stays deferred: the slot count is for V2 domain runs, and V2 serves badge and OG from the stored entry, so counting V1 badge runs would inflate it (report Phase 1 Reader, DEFERRED).
