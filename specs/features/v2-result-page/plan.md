# Plan: v2-result-page

## Phase 1 — Page views from an entry

### Groups

G1: C1–C18 (one production file: `crates/web/src/pages/result/views.rs`; `pages/mod.rs` and `pages/result/mod.rs` only declare modules).

### Plan

G1:
1. `crates/web/src/pages/mod.rs`: `pub mod result;`.
2. `crates/web/src/pages/result/mod.rs`: `pub mod views;`.
3. `crates/web/src/pages/result/views.rs`: helpers (owner word/label/rank, severity word/rank, protocol labels, block module → Protocol); `evidence_href` over `evidence::href`; `issues` (warn/fail only, fixed group order, inside a group severity → fail before warn → catalogue index; body findings joined by " " else catalogue finding); `passed_checks` (pass, not_applicable with section reason, not_tested, unmeasured "could not check"; catalogue order; unknown ids after); `change_label`; `GradeStep`, `DiffView`, `FIRST_STATE`, `diff_view` (checks then blocks, grade steps, " → " summary, engine note); `recorded_diff_view`; `history_sentence`, `issue_since` (UTC MM-DD, " · "); `layout_input` over `Profiles::get`.

Decision: a warn or fail whose id is not in the catalogue (removed by a release) is left out of the issues, which need catalogue texts, and listed after the catalogue checks in the passed-checks details, over a fallback owner group: requirement 7's texts all come from the catalogue (implementer under the operator's autonomy mandate, 2026-10-18).

## Phase 2 — Server-rendered pages

### Groups

G1: C2–C9, C11–C15, C17–C19 and the web half of C1 (crates/web) · G2: C1, C10, C16, C20, C21 (crates/lens/src/lib.rs); G2 depends on G1.

### Plan

G1:
1. `templates/components/trace.html`: `data-run="{{ loop.index0 }}"` on each row (no new `TraceRow` field: protected struct literals).
2. `src/components.rs`: `line_tone` pub(crate); `src/entries/view.rs`: `grade_tone` pub(crate).
3. `src/pages/errors.rs` + `templates/pages/not_found.html`: optional `reason`, `refused_page(input, reason)` answering 400 with the input refilled.
4. `src/pages/result/views.rs`: `utc_minute`, `trace_view`, `section_outcome`.
5. `src/pages/result/mod.rs`: `ResultState` (store, runs, ttl, ip extractor, fallback), `router` (`/`, `/{domain}`, `/{domain}/history`, `/{domain}/run` placeholder 501), one segment classifier (no dot, root files, `.well-known`, `bimi-logo.svg` → fallback), validation (refused 400, normalised GET 301 / POST 400), result page from `store.entries` (24 shown, layout at `n × line_pitch_min`), history page, idle page; all through `pages::page`/`secure`.
6. `templates/pages/{result,history,idle}.html` on `base.html` with the ids of the HTML contract; no component-owned classes, no inline script, no colour literals, no codenames.

G2:
1. `crates/lens/src/lib.rs` `app()`: with history enabled, merge `result::router(ResultState::new(history, LensRuns, cache.ttl_seconds).with_ip_extractor(..).with_fallback(spa::handler))`, wrapped to drop the inner CORP so lens's `cross-origin` stays (test_header_parity keeps every header of `/` but the CSP).

Decision: `index.html` joins the result router's non-domain segments, over letting it reach `validate` and render "not checked yet" with an autostarting run: lens's SPA serves `/index.html` today, and the spec keeps every non-domain path on its current handler until `v2-cutover` (implementer under the operator's autonomy mandate, 2026-10-18).
