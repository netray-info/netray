# Review: v2-result-page

## ecf44678..ecf2aa01

### Reader

COUNTS blockers=1 majors=2 minors=2
LENSES Engineering, Security, Testing

BLOCKER | crates/web/src/pages/result/mod.rs:387 | Once a page that had no entry finishes its first streamed run, its header button still says "check now" and posts `trigger=visit`. For the whole cache window, pressing it returns the entry that was just stored and starts no run. | `GET /new2.example.com` (no entry) renders the button `value="visit"` "check now" (mod.rs:820) and autostarts a visit. The run is recorded `First` at T. The end fragments replace `result-grade`, `run-line`, `progress`, `result-share`, `spectrum`, `diff`, `issues`, `evidence-*` and `passed`, but never `#recheck` or its button. At T+60 s the user follows the diff note "re-check after your change" and presses the button. That POSTs `trigger=visit`, run.rs:80 finds the head fresh (60 s < 300 s), and run.rs:81 answers `stored_answer`, so no run starts. A fresh GET of the same domain renders `value="recheck"` "re-check now" instead. `result_run_equal.rs` compares only the 13 fragment IDs, so no test sees the stale button.

```quote crates/web/src/pages/result/mod.rs:387
    fn fragments(&self) -> Vec<(String, &str)> {
```
```quote crates/web/src/pages/result/mod.rs:820
    let (trigger, trigger_label) = if shown.recheck {
```
```quote crates/web/src/pages/result/run.rs:81
                return stored_answer(&domain, &entries, sse);
```

MAJOR | crates/web/src/pages/result/mod.rs:129 | When a domain segment gets an unsupported method, the 405 is returned without `secure()`. Under lens it then carries lens's CSP with `'unsafe-inline'` instead of v2-pages' CSP, which requirement 1 promises for every response of these routes. | `POST /shop.example.com` or `GET /shop.example.com/run` against lens returns a 405 with no inner CSP. lens's `security_headers_mw` (keep_inner) then inserts `script-src 'self' 'unsafe-inline'`. `result_page_csp.rs` has no 405 row.

```quote crates/web/src/pages/result/mod.rs:129
    StatusCode::METHOD_NOT_ALLOWED.into_response()
```

MAJOR | crates/lens/src/og/handler.rs:198 | If reading the predecessor fails, the error is logged and swallowed, and the domain card is drawn as if the predecessor were swept: no fixed lines. With a fresh head that card is served with the long cache. On the same error the entry card route answers 500 instead. | `/og/shop.example.com.png` with a fresh head whose predecessor had `http.hsts` fail. `store.get(predecessor_id)` returns an `Err` (for example a pool acquire timeout under load), the plate is built from `card_layout(&entry.run, None)` with no fixed line, and `og_response(.., long=true)` sends `public, max-age=3600, s-maxage=3600, stale-while-revalidate=86400`. The CDN keeps the wrong plate for an hour or more. crates/web/src/entries/routes.rs:162 returns 500 on the same failure.

```quote crates/lens/src/og/handler.rs:198
                tracing::warn!(error = %e, id = %id, "history predecessor lookup failed");
```
```quote crates/web/src/entries/routes.rs:162
    let predecessor = match &entry.predecessor_id {
```

MINOR | crates/lens/tests/result_run_admission.rs:145 | This test breaks the CLAUDE.md rule "never combine `start_paused` or `pause()` with a `HistoryStore`". | `harness` sets `state.history_store = Some(..)`, then the test pauses the clock and runs an admitted `Trigger::Api` run that writes to that store. The test's own comment says the clock auto-advances, so sqlx's acquire timeout can fire and the run goes unrecorded silently. No assertion would notice.

```quote crates/lens/tests/result_run_admission.rs:145
    tokio::time::pause();
```

MINOR | tests/acceptance/result-page/result-page.spec.ts:225 | The final screenshots check reads `target/screenshots/v2-result-page`, which neither the spec nor the recipe ever clears. On any machine that has run the suite once it passes even if a screenshot test is deleted. | Delete the `history ${w.name}` test and run `just acceptance-local result-page/` again. The `history-390.png` and `history-1440.png` left from the earlier run still satisfy both the per-state lookup and `toHaveLength(16)`.

```quote tests/acceptance/result-page/result-page.spec.ts:225
  const files = readdirSync(SHOTS);
```

### Spec reader

COUNTS blockers=0 majors=2 minors=2
REQUIREMENTS met=29 partial=2 missed=0 contradicted=0
MAJOR | crates/web/src/pages/result/mod.rs:129 | A wrong method on a domain path answers a bare 405 that does not go through `secure()`. lens's outer middleware therefore adds its own CSP, `script-src 'self' 'unsafe-inline'`, to a response of the result routes. | R1, concrete case: `PUT /shop.example.com` (or `GET /shop.example.com/run`) reaches `wrong_method`. `domain_segment` returns `Some`, so the handler returns `StatusCode::METHOD_NOT_ALLOWED.into_response()` with no CSP. lens's `security_headers_mw` then sets `strict_csp_val`, which is `csp_with("'self' 'unsafe-inline'")` (crates/common/src/security_headers.rs:76), because the inner response carries no CSP of its own. `result_page_csp.rs` never tests a 405.
MAJOR | crates/web/src/pages/result/views.rs:409 | The "profile effect" and "domain effect" notes on a rescored grade line never reach the page. Only `DiffView.grade` carries them, and the page renders `DiffView.block`, whose summary joins the bare letters. | R11, concrete case: a diff with `GradeChange{from: B, rescored: Some(C), to: A}`. `stored_shown` calls `views::diff_view(...).block` (mod.rs:510), the summary becomes "B → C → A", and `diff_block.html` prints only `view.summary`. Neither note label is rendered. The test asserts `DiffView.grade`, which the template never reads.
MINOR | justfile:168 | `acceptance-local` now rewrites every existing relative spec argument into an absolute `$PWD/` path. That changes how Playwright filters work for every caller of the recipe, and the spec asks only for the `LENS_EXTRA` pass-through. | Unrequested behaviour, concrete case: `just acceptance-local v2/` used to pass `v2/` as a substring filter. It now passes `<repo>/tests/acceptance/v2`.
MINOR | crates/web/src/pages/result/mod.rs:61 | `index.html` is added to the non-domain segments, so `GET /index.html` goes to lens's SPA fallback. R1's list of non-domain segments does not name it. | Unrequested behaviour, R1's exclusion list, concrete case: `index.html` has a dot and passes `netray_common::domain::validate` syntax, so without this entry it would be a result page.

```quote specs/features/v2-result-page/spec.md:234
   - Every response of these routes carries v2-pages' CSP (its requirement 11), which lens's
```

```quote crates/web/src/pages/result/mod.rs:129
    StatusCode::METHOD_NOT_ALLOWED.into_response()
```

```quote specs/features/v2-result-page/spec.md:297
    - The grade as `B → A`. With `rescored`, two steps: predecessor → re-scored ("profile effect")
      → current ("domain effect").
```

```quote crates/web/src/pages/result/views.rs:409
    let summary = grade
        .iter()
        .map(|s| s.grade.as_str())
        .collect::<Vec<_>>()
        .join(" \u{2192} ");
```

```quote crates/web/src/pages/result/mod.rs:510
        None => views::diff_view(entry.diff.as_ref(), cat).block,
```

```quote crates/web/templates/components/diff_block.html:4
  <p class="diff-block__summary">{{ view.summary }}</p>
```

```quote specs/features/v2-result-page/spec.md:671
`netray-store` example), the `LENS_EXTRA` pass-through in `acceptance-local`, `tests/acceptance/result-page/*.spec.ts`, any CSS
```

```quote justfile:168
            *) case "$prev" in --*) ;; *) [ -e "${arg%%:*}" ] && specs[$i]="$PWD/$arg" ;; esac ;;
```

```quote crates/web/src/pages/result/mod.rs:61
    "index.html",
```

```quote specs/features/v2-result-page/spec.md:225
   - A first segment without a dot is never a domain, and neither are §5.1's root files
```

### Refutation

Merged: the 405 CSP finding (reader MAJOR, spec reader MAJOR at crates/web/src/pages/result/mod.rs:129) counts once.

| finding | refuter | confidence | result |
|---|---|---|---|
| F1 BLOCKER mod.rs:387 stale check button after a first run | CONFIRMED | 8 | kept |
| F2 MAJOR mod.rs:129 405 without v2-pages' CSP | CONFIRMED | 7 | kept |
| F3 MAJOR og/handler.rs:198 swallowed predecessor error on the domain card | UNCERTAIN | 5 | downgraded to MINOR |
| F4 MAJOR views.rs:409 rescored grade notes never rendered | CONFIRMED | 9 | kept |

### Calibration

| finding | refuter answer | confidence | held | command |
|---|---|---|---|---|
| F1 | CONFIRMED | 8 | held | read `Parts::fragments` (mod.rs:387): no `#recheck` or button id; `run.rs:80-81` answers `stored_answer` for a fresh visit |
| F2 | CONFIRMED | 7 | held | read `wrong_method` (mod.rs:129): bare 405, no `secure()`; lens `security_headers_mw` keep_inner inserts its own CSP |
| F4 | CONFIRMED | 9 | held | read `diff_view` summary (views.rs:409) and `diff_block.html:4`: notes only in `DiffView.grade` |

### Roll call

| principle | answer |
|---|---|
| P03 | convergence |
| P10 | absence |
| P12 | absence |
| P13 | convergence |
| P18 | convergence |
| P26 | absence |
| P35 | absence |
| P36 | convergence |
| P40 | absence |

### Summary

Before refutation: reader 1/2/2, spec reader 0/2/2 (REQUIREMENTS met=29 partial=2 missed=0 contradicted=0). After refutation and merging: 1 blocker, 2 majors, 5 minors; verified 3, held 3. Roll call: 9 answered, 4 convergence, 0 divergence, 5 absence. The blocker is fixed on this branch (implementer under the operator's autonomy mandate: a blocker stops only when it survives a second pass), followed by a second reading of the fix range.

## ecf2aa01..6cc28017

### Reader

COUNTS blockers=0 majors=1 minors=0
LENSES Engineering, Security, Testing
MAJOR | crates/web/src/pages/result/run.rs:131 | When a visit is answered from a fresh stored entry, the response streams only `Parts::fragments()` and leaves out the new `recheck-button` fragment, so a page that started with no entry shows the entry's grade but keeps its `value="visit"` "check now" button, and every click on it inside `ttl_seconds` replays the stored answer instead of starting a run. | Domain with no history. `GET /shop.example.com` renders `Shown::none`, so the button is `value="visit"` "check now", and autostart is on. The autostart POST is refused, for example on the per-IP budget, and a countdown starts. During the countdown another client's run records an entry. The countdown ends and the user clicks "check now", which posts `trigger=visit`. `is_fresh` is now true, so the request goes to `stored_answer`. `stored_parts` builds parts with `recheck: true` and a `value="recheck"` button, but only the 13 ids of `fragments()` plus `end` are sent. The island replaces the grade, run line, share and other parts, and the header button stays `visit`/"check now". The next click posts `visit` again, `is_fresh` is still true, and the stored answer comes back again; no run starts until the TTL lapses or the page is reloaded. This breaks spec requirement 23 (the button value is `recheck` once there is an entry) and requirement 20 (a reload of the same state renders "re-check now"). The run path at `run.rs:368-371` now sends the button and this sibling path does not. The protected test `c14` in `result_run_stream.rs` fixes the stored answer's id list without the button, so no test catches this.

```quote crates/web/src/pages/result/run.rs:131
                .fragments()
```

### Spec reader

COUNTS blockers=0 majors=2 minors=0
REQUIREMENTS met=3 partial=2 missed=0 contradicted=0
MAJOR | crates/web/src/pages/result/mod.rs:108 | The 405 fix (F2) only covers domain paths. `/` is still `get(idle)` with no fallback and no `secure` layer, so `PUT /` (or `OPTIONS /`) gets axum's own 405 with no CSP, and lens's `security_headers_mw` then adds its own policy, `script-src 'self' 'unsafe-inline'`. | Req 1, case: `PUT /` on lens returns 405 with `script-src 'self' 'unsafe-inline'`
MAJOR | crates/web/src/pages/result/run.rs:128 | The F1 fix pushes the `recheck-button` fragment only in `Live::finish`. `stored_answer`, which handles a visit that finds a fresh entry, sends `parts.fragments()` without it, so a page rendered with no entry keeps its "check now" / `visit` button while it now shows a stored entry, and every click lands on the fresh entry again and never re-checks. | Req 6, case: `GET /new.example.com` renders with no entry; another client then records a run; the autostart `POST trigger=visit` gets the stored fragments, but the button stays `value="visit"`

```quote crates/web/src/pages/result/mod.rs:108
        .route("/", get(idle))
```

```quote crates/common/src/security_headers.rs:76
    let strict_csp = csp_with("'self' 'unsafe-inline'");
```

```quote crates/web/src/pages/result/run.rs:128
    match stored_parts(domain, entries, false, None, None) {
```

```quote crates/web/src/pages/result/run.rs:368
            if parts.recheck {
```

### Refutation

Merged: the stored-answer button finding (reader MAJOR run.rs:131, spec reader MAJOR run.rs:128) counts once.

| finding | refuter | confidence | result |
|---|---|---|---|
| G1 MAJOR run.rs:128 stored answer omits the recheck-button fragment | CONFIRMED | 8 | kept |
| G2 MAJOR mod.rs:108 `/` 405 without v2-pages' CSP | CONFIRMED | 7 | kept |

### Calibration

| finding | refuter answer | confidence | held | command |
|---|---|---|---|---|
| G1 | CONFIRMED | 8 | held | read `stored_answer` (run.rs:128-135): `parts.fragments()` only; `Parts::fragments` (mod.rs:389-404) lists no `recheck-button` |
| G2 | CONFIRMED | 7 | held | read `router` (mod.rs:106-113): `/` has `get(idle)` and no `.fallback(wrong_method)` |

### Roll call

Unchanged from the first reading of this branch (fix range only touches the result router and its templates): 9 answered, 4 convergence, 0 divergence, 5 absence.

### Summary

Before refutation: reader 0/1/0, spec reader 0/2/0 (REQUIREMENTS met=3 partial=2 missed=0 contradicted=0). After refutation and merging: 0 blockers, 2 majors, 0 minors; verified 2, held 2. The first reading's blocker did not survive this second pass. Both majors are fixed on the branch before finish.
