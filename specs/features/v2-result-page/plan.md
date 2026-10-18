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

## Phase 3 — Run route and fragments

### Groups

G1: C1–C14, C16–C19 (crates/web: every criterion goes through `POST /{domain}/run`) · G2: C15 (no lens change expected; Phase 2 wires `LensRuns` and the extractor; verified after G1).

### Plan

G1:
1. `crates/web/src/client.rs` (new, `mod client;` in lib.rs): `pub(crate) fn client_ip(extractor, headers, peer) -> IpAddr` (drops `cf-connecting-ip`, default 127.0.0.1:0), the one copy of `api/limit.rs:15` and `pages/inspect.rs:402` `client` (Rule of Three); both callers delegate.
2. `api/runs.rs` `wants_event_stream` → pub(crate), re-exported in `api/mod.rs`.
3. `templates/components/spectrum.html`: macro `spectrum(view, live)`; band tag `<g id="band-{p}" data-protocol="{p}"{% if band.is_live(live) %} data-live{% endif %}>`; `components.rs`: `SpectrumBand::is_live(&str)`, `spectrum(view)` passes "", new `spectrum_live(view, live)` (fallback: a `live: bool` field with `ADLC-Test-Change` on components.rs:130,145 and drawing_rules.rs:78).
4. `pages/result/views.rs`: `live_run(&[SectionEvent]) -> Run` (the partial run: complete false, grade incomplete); `live_input(stored: Option<&Run>, live: &Run) -> RunInput<'static>` (the overlay; Incomplete/TimedOut → Unmeasured); `trace_view` over `(created_at, Grade)` rows; `refusal_line(&RunRefusal) -> (lead, M)`; `segment_states(&Run)`.
5. `templates/partials/result_{run_line,progress,header,spectrum,diff,issues,evidence,passed}.html`: one partial per fragment id with exactly one root element carrying the id, cut from `pages/result.html`, which then composes them (`{{ parts.x|safe }}`), so GET and fragments are byte-equal. Refusal run line: lead text + `<span data-island="countdown" data-retry-after="M">M</span> s`, no inner div, no extra text. Figure starts `<figure id="spectrum"`, `data-refreshing` after the id. Fragments are trimmed and never carry `data-stream-autostart`.
6. `pages/result/mod.rs`: `Parts` of rendered strings; `stored_parts` (GET, 429 page, fresh shortcut, end-recorded with `recorded_diff_view` over re-read entries); `live_figure`; `not_stored_parts` (grade F stays F else incomplete, `incomplete_run_line`, no share, `Links::Live`, live evidence over the Done run).
7. `pages/result/run.rs` (new): form `trigger` (visit|recheck; else 400), client via `client_ip`, `is_prefetch`; fresh visit → stored end fragments (SSE) or 303; else `start` before any store read; NotStarted → 503 no-store; Refused → SSE 429 Retry-After `[run-line, end]` or 429 full page; Admitted SSE → start, per section (progress, spectrum, issues, evidence-<p>), end group (result-header, spectrum, diff, issues, evidence-dns…ip, passed), end; no keep-alive; Admitted without SSE → drain then 303.

G2: none; `cargo test -p lens --test result_run_admission` after G1.

Decisions (implementer under the operator's autonomy mandate, 2026-10-18):
- After an `Identical` run the end `#diff` fragment carries requirement 12's "unchanged since" note, which the GET page does not, over dropping the note: requirement 12 names the note for the stream's end; requirement 20's equality is tested on `Changed`.
- A missing or unknown `trigger` answers 400, over defaulting to `visit`: the form always sends one of the two values.
- `Recorded(None)` is treated as `NotStored { diff: None }`.
- Requirement 30's §5.2 amendment lands with Phase 8, as the spec lists it.
