## 7e2907895cc408780a6d5c68fd602e51b793bb4d..c1d6cd16a9fa342708baf351f28f9e2527dd0eca

### Reader

COUNTS blockers=1 majors=1 minors=0
LENSES Engineering, Security, Testing
BLOCKER | crates/lens/src/og/handler.rs:125 | After an incomplete run, with no head entry, OG shows the run's scored grade and percentage. R11 asks for "today's unscored card", and the badge shows `?` for the same run, so the two views disagree (SDD §9.1, V2.5). | Take example.com with no history entry. A TLS hard-fail check fails (`tls.not_expired`; v3 profile `crates/engine/profiles/v3.toml:26`) and the email section times out. Under S35 the engine scores that run as grade `F` with `complete = false`. `/og/example.com.png` goes to `admit`, then `run_card = Card::new(&score.grade, ...)` with "F" (`og/handler.rs:121`). `complete` is false and `head_card` is None, so the card rendered shows "F" and "NN.N%". `/badge/example.com.svg` for the same run renders `?` via `stale.unwrap_or("incomplete")` (`routes.rs:617`). report.md C6 records "the unscored card" as green, but no test covers OG after an incomplete run.
MAJOR | crates/lens/src/routes.rs:779 | The head lookup and `admit` are not atomic, so a request that arrives while the domain's run is in flight can get a throttle refusal (429, or `?` on badge/OG) instead of joining the run or getting the hit. | 1) Request R1 for example.com runs `cache::head` while run A is in flight, and the SELECT reads the snapshot before A's `record` commits, so it returns None. 2) Before R1 is polled again, A's task commits, runs `cache::hold` and drops `Deregister`, which removes the map entry (`admission.rs:402`). 3) R1 calls `admit` (`routes.rs:813`). The map is empty, and A's throttle record (`admission.rs:268`) refuses it: `Refused{throttle, retry_after_s ≈ 60 - elapsed}`, so `/api/check` answers 429 with that `Retry-After`. A fresh head and a held rendering exist at that point, and R1 arrived while A was in flight (R2/R3). Badge (`routes.rs:565`) and OG (`og/handler.rs:87`) take the same path and answer `?` or the stale head with the short Cache-Control. The window is narrow, about the commit-to-deregister time, and it comes from the hit-then-`admit` order R15 itself prescribes.

```quote crates/lens/src/og/handler.rs:125
    let card = head_card.as_ref().unwrap_or(&run_card);
```

```quote crates/lens/src/og/handler.rs:121
    let run_card = Card::new(&score.grade, score.overall_percentage, SystemTime::now());
```

```quote crates/lens/src/routes.rs:617
        stale.unwrap_or("incomplete"),
```

```quote crates/engine/profiles/v3.toml:26
hard_fail = ["chain_trusted", "hostname_match", "not_expired", "tls_reachable"]
```

```quote crates/lens/src/routes.rs:779
        && let Some(lookup) = crate::cache::head(&state, &domain).await
```

```quote crates/lens/src/routes.rs:813
        match admit(&state, &domain, client_ip, Trigger::Visit, prefetch) {
```

```quote crates/lens/src/admission.rs:268
    if let Some(record) = throttle.get(&key) {
```

```quote crates/lens/src/admission.rs:402
            runs.remove(&self.key);
```

```quote crates/lens/src/routes.rs:565
    let head = crate::cache::head(&state, &badge_req.domain).await;
```

```quote crates/lens/src/og/handler.rs:87
    let head = crate::cache::head(&state, &domain).await;
```

### Spec reader

COUNTS blockers=0 majors=1 minors=1
REQUIREMENTS met=20 partial=1 missed=0 contradicted=0
MAJOR | crates/lens/src/og/handler.rs:121 | When there is no head entry and the run ends incomplete, OG draws the run's own `score.grade`. For an incomplete run with a hard fail that grade is "F" plus a percentage, so the card is scored, not "today's unscored card". The badge handles the same case by drawing `?` (`stale.unwrap_or("incomplete")`). | R11, case: no history entry for example.com, and the run ends incomplete with a failed hard-fail check. The engine grades that F (S35, `crates/lens/src/score.rs:66`). `/og/example.com.png` then shows an F card with the short Cache-Control, while `/badge/example.com.svg` shows `?`. `incomplete_c8` only checks for a PNG and the short Cache-Control, so it does not catch this.

```quote specs/features/v2-admission/spec.md:186
    in `netray_engine::history` returns the `Score`; `rescore` becomes its grade. After an
    incomplete own or joined run, OG renders the head entry if one exists and today's unscored
    card otherwise. Cache-Control follows requirement 10.
```

```quote crates/lens/src/og/handler.rs:121
    let run_card = Card::new(&score.grade, score.overall_percentage, SystemTime::now());
    if score.complete {
        return og_response(&state, &req_headers, &domain, &label, &run_card, true);
    }
    let card = head_card.as_ref().unwrap_or(&run_card);
    og_response(&state, &req_headers, &domain, &label, card, false)
```

```quote crates/lens/src/score.rs:66
    /// incomplete. A failed hard-fail check still gives grade `F` when incomplete.
```

MINOR | crates/lens/src/routes.rs:590 | Badge and OG requests that `admit` answers `NotStarted` because of the prefetch header increment `lens_badge_requests_total{cache="throttled"}`. R18 names no metric for a prefetch answer, and R12 ties `throttled` to throttle refusals, so prefetches now inflate the throttle series. | R18, case: `GET /badge/example.com.svg` with `Sec-Purpose: prefetch`, no run in flight and no fresh head. `admit` returns `NotStarted` and the response increments `throttled` through the `stale` branch or `throttled_badge_response`. `/og/example.com.png` does the same at `og/handler.rs:110-111`.

```quote crates/lens/src/routes.rs:590
        Admit::Refused(_) | Admit::NotStarted => {
            return match stale {
                Some(grade) => {
                    metrics::counter!("lens_badge_requests_total", "cache" => "throttled")
                        .increment(1);
                    badge_response(&badge_req, &req_headers, grade, false)
                }
                None => throttled_badge_response(&badge_req, &req_headers),
```

```quote crates/lens/src/og/handler.rs:110
        Admit::Refused(_) | Admit::NotStarted => {
            metrics::counter!("lens_badge_requests_total", "cache" => "throttled").increment(1);
```

```quote specs/features/v2-admission/spec.md:231
    `NotStarted`, `/api/check` answers 503 with error code `PREFETCH_REFUSED` in the
    architecture-rules error format and `Cache-Control: no-store` (mandate default, see Decision
    log); badge
    and OG render the head entry with the short Cache-Control, or `?`. The request spends no budget
```

### Merged

The reader's BLOCKER at `crates/lens/src/og/handler.rs:125` and the spec reader's MAJOR at `:121` are one defect (OG draws an incomplete run's own grade when no head exists); refuted and counted once, at BLOCKER.

### Refutation

- og/handler.rs:121/125 (BLOCKER) — UNCERTAIN, confidence 5: main also drew an incomplete hard-fail run's `F` on the card, so "today's unscored card" reads both ways; the badge/OG mismatch is real. Downgraded to MINOR.
- routes.rs:779 (MAJOR) — CONFIRMED, confidence 7: the head read and `admit` are separate; a request whose head read precedes run A's record commit and whose `admit` follows A's deregistration meets A's throttle record. Kept.

### Calibration

| finding | refuter answer | confidence | result | command |
|---|---|---|---|---|
| og/handler.rs:121/125 OG draws the incomplete run's grade | UNCERTAIN | 5 | held (as MINOR) | read `og/handler.rs:118-126` and `git show 7e29078:crates/lens/src/og/handler.rs` (rendered `score.grade` with no `complete` mapping) |
| routes.rs:779 head read and admit not atomic | CONFIRMED | 7 | held | traced `admission.rs` `run_task` (record → `cache::hold` → `Recorded` → deregister) against `routes.rs:779`/`:813`; no deterministic test: the window is one store commit wide |

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | absence | no new `*Status`/`*Verdict` enum, no UI or serializer text |
| P10 | convergence | an incomplete run never replaces the head, never a hit (SDD §4.4) |
| P12 | convergence | `run_check_streaming` keeps `hard_deadline`; shutdown drain bounded by `HARD_DEADLINE + 10s` |
| P13 | convergence | `domain_run_slots` semaphore for domain runs, separate from per-IP, global and the client budget |
| P18 | convergence | no new `reqwest` use; `AdmissionConfig` has `deny_unknown_fields`; `test_check_config.sh` rows |
| P26 | absence | no new check id or fact |
| P35 | convergence | SDD claims held by `tests/repo/test_sdd_admission.sh`; config rows in `test_check_config.sh` |
| P36 | absence | no generated file changes |
| P40 | absence | no module added or removed; `score_run` uses no module crate |

### Summary

Before refutation: reader 1/1/0, spec reader 0/1/1 (REQUIREMENTS met=20 partial=1 missed=0 contradicted=0); merged 1 blocker, 1 major, 1 minor. After refutation: 0 blockers, 1 major (head read vs `admit` race), 2 minors (OG incomplete-run card; prefetch answers counted `throttled`). Reader after refutation 0/1/1; spec reader after refutation 0/0/1 (REQUIREMENTS met=20 partial=1 missed=0 contradicted=0). verified 2, held 2. Roll call: 9 answered, 5 convergence, 0 divergence, 4 absence.
