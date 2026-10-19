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

## Phase 4 — Stream wiring and countdown

### Groups

G1: C5, C7, C3 (`islands/src/countdown.ts`, `main.ts`) · G2: C6, re-checking C2 and C4 (`partials/result_spectrum.html`, `result_issues.html`, regenerated fixtures) · G3: C1 (`static/pages.css`).

### Plan

G1: `countdown(el)` reads `data-retry-after`, disables `#recheck button[name="trigger"]` while above 0, ticks with `setInterval` 1000 ms writing text and `data-retry-after`, clears and enables at 0; `register("countdown", countdown)` in main.ts; no style writes (gate rule 1).
G2: `<figure id="spectrum" data-island="spectrum"…>`, `<section id="issues" data-island="issues">` (attributes after the id); regenerate `islands/test/fixtures/result-*` with `UPDATE_GOLDEN=1 cargo test -p netray-web --test result_islands_fixtures` (protected: `ADLC-Test-Change` per changed fixture).
G3: `#spectrum[data-refreshing] g[data-protocol]:not([data-live])` and `#result:has(#recheck[data-stream-state="running"]) :is(#diff, #issues, #evidence, #passed)` dimmed with a literal opacity; id and attribute selectors only (gate rule 2).
`islands/dist` is gitignored; rebuild it (`npm run build -w @netray-info/islands`) before cargo tests.

## Phase 5 — Spectrum island

### Groups

G1: C1–C5 (server markup first, then `islands/src/spectrum.ts`; the vitest test reads the regenerated fixture).

### Plan

1. `pages/result/views.rs`: `history_sentences(points: &[(i64, &Run)]) -> Vec<(String, String)>`, oldest first, one per catalogue entry in catalogue order, over `history_sentence`; a check missing from a run counts as `not_tested`.
2. `pages/result/mod.rs`: `Shown.sentences`, `SpectrumPart.sentences`, `spectrum_part(.., sentences)`; computed over stored entries only (`stored_shown` over the chosen entries reversed; `not_stored_shown`/`unrecorded_shown` over the history before the run); `run.rs` `figure` passes the sentences of the stored entries (never a live row, keeps `result-run.sse` deterministic).
3. `partials/result_spectrum.html`: plate, `#spectrum-gutter`, `#spectrum-sentence`, `#spectrum-stepper` (two `button type="button" data-step`), `#trace`, `#legend`, `<ul id="spectrum-sentences" hidden>` — nothing new between `#trace` and `#legend`; no `spectrum__*`/`trace__*` classes in the partial (gate rule 2).
4. `components/spectrum.html`: after the bands, `<g class="spectrum__targets">` with one `<rect class="spectrum__target" data-target="<id>" x=… y="0" height="100%" />` per line of each band's first run (template only; `data-target`, not `data-code`).
5. CSS: `static/spectrum.css` `.spectrum__target` (transparent, pointer events, pitch-wide, centred); `static/trace.css` `.trace__row--hover` (enlarged via the trace row token), `.trace__row--pinned`; `static/pages.css` `#spectrum-stepper` hidden above 480 px, flex with 44 px buttons below.
6. `islands/src/spectrum.ts` (hovered, pinned, selected; shown = hovered ?? pinned ?? 0; toggles `data-shown`, row classes and `aria-pressed`, gutter and sentence text; phone width via `matchMedia("(max-width: 480px)")` skips hover; stepper clamps 0..oldest; targets set the selection on pointerenter/click; never `hidden` or `style`); `register("spectrum", spectrum)` in main.ts.
7. Regenerate `islands/test/fixtures/result-{trace,fresh,stale}.html` and `result-run.sse` (`UPDATE_GOLDEN=1`), protected: `ADLC-Test-Change` per file.

Decision: a check an entry lacks reads "not tested" in its line's sentence, over leaving the sentence empty: the layout mutes such a line, and every line's target then shows a sentence (implementer under the operator's autonomy mandate, 2026-10-19).

## Phase 6 — Issues and recent islands

### Groups

G1: C1, C3, C4 · G2: C2, C5 · G3: C6 (budget only). G1 and G2 share one line each in `islands/src/main.ts`; they run one after the other.

### Plan

G1: `IssueRow.owner_label` from `owner_label(spec.fix_owner)` (views.rs); `data-owner-label` on `.issue` in `partials/result_issues.html`; `islands/src/issues.ts` (unhide the switch wrapper, regroup the same row nodes in fixed key order, severity → fail before warn → data-order, labels from the rows / values / protocol map, `aria-pressed`, `netray.grouping` in try/catch); `register("issues", issues)`.
G2: `run_date(ts)` and `Shown.date` (stored: the entry's `created_at`; not-stored and unrecorded ends: now), `GradePart.date`; `#result-grade` carries `data-grade` and `data-date` when a grade shows; `data-island="recent"` on `#result`; `islands/src/recent.ts` (fresh page writes at mount; `netray:stream-end` listener re-reads `#result-grade`; skips `incomplete`; dedupe, newest first, at most 8; idle list renders `<li>` with link, grade, date and unhides; no fetch, beacon or cookie); `register("recent", recent)`.
Fixtures regenerated after each group (`UPDATE_GOLDEN=1`; `result-issues.html`, `result-trace.html`, `result-fresh.html`, `result-stale.html`, `result-run.sse`), each under `ADLC-Test-Change`.
G3: none; `budget.test.ts` passes (3.5 KB of 30 KB gzipped today).

## Phase 7 — OG cards on the layout

### Groups

G1: C3, C4 (netray-web) · G2: C6 (lens fonts) · G3: C1, C2, C5 (lens og render and handler, SDD); in that order (the lens tests compile once G3 changes the signatures).

### Plan

G1:
1. `spectrum.rs` `fn px` → pub(crate).
2. `crates/web/src/plate.rs` (new, `pub mod plate;`; outside the spectrum purity guard; no `fn label_width`, no word "Router", no `%`, no newline in output): `CARD_PLATE_WIDTH = 1104`, `CARD_PLATE_HEIGHT = 56.0`; `card_layout(run, pred)` = `spectrum::layout(layout_input_or_current(run), [pred mapped], catalogue(), 1104)`; `card_plate(layout)` = one-line `<g class="plate">`: frame rect (COLOR_DARK_PLATE, border COLOR_DARK_PLATE_BORDER, LINE_HAIR), boundaries (COLOR_DARK_GRID), 54 `<line class="plate__line" data-code data-tone x1=x x2=x y1=56 y2=56*(1-h)>` (pass STATUS_DARK_PASS_MARK/LINE_PASS, warn STATUS_DARK_WARN_MARK/LINE_WARN, fail STATUS_DARK_FAIL_MARK/LINE_FAIL, fixed STATUS_DARK_FIX_MARK/LINE_PASS plus `plate__ring` circle r 3.5, muted STATUS_DARK_UNTESTED_MARK/LINE_HAIR), per run-0 label a `plate__leader` line, a backing rect (COLOR_DARK_BG) and `<text class="plate__label" data-code data-tone font-family="JetBrains Mono" font-size=12>` (warn/fail/fix text tokens) at `label.start` (+4), rows of 14 px above the plate with an 8 px leader; numbers `{:.2}`; inline test: an all-fail run has at most 4 label rows.
3. `entries/routes.rs`: `EntryCard.plate: String`; `entry_card` reads the predecessor after the 304 check (`Ok(None)` swept → None, `Err` → the entry lookup's 500); ETag unchanged.
G2: `crates/lens/assets/fonts/JetBrainsMono-Regular.ttf` and `JetBrainsMono-LICENSE.txt` (OFL-1.1, from the v2.304 release); `og/fonts.rs` loads it, guard 3 faces.
G3:
1. `og/layout.rs`: `PLATE_X = 48`, `PLATE_Y = 72` (labels rise to y ≥ 8; text lines keep their coordinates).
2. `og/render.rs`: `svg_for_grade(.., plate: Option<&str>)`, `svg_for_entry(.., plate: &str)`; `card_svg` inserts `<g transform="translate(48 72)">plate</g>` after the background; `None` is byte-identical; the inline tests pass `None` (`ADLC-Test-Change`).
3. `og/handler.rs`: `entry_card_renderer` passes `&card.plate`; `Card.plate`; `head_card` reads the predecessor and draws the head entry's plate; the admitted-run branch draws the published run's plate over the head run; unscored and incomplete cards stay plain; `compute_etag` appends the plate only when present.
4. `state.rs` doc comment names both fonts.
5. `specs/sdd/v2.md`: §4.6 (:652–653) the spectrum card sentence keeping `grade, domain and recording time`, `V1 layout`, `no score`, `spectrum::layout`, `V4.4`, `v2-result-page`; V4.4 row (:1205) the OG cards' use built by `v2-result-page`, still naming `v2-spectrum`, `v2-result-page`, `v2-inspectors`, `og.png`; §5.4 height bullet "56 px on the OG card".

Decisions (implementer under the operator's autonomy mandate, 2026-10-19):
- The card plate is 56 px high above the existing text lines, over the page's 92 px: a 1104 px plate cannot pass the grade glyph, and the card's text lines keep their places (requirement 29).
- `/og/<domain>.png` after an admitted run answers the plain card (no plate), over that run's plate over the head entry: an admitted run is no entry yet, and when the store records it `Identical` the run-over-head layout drops the fixed lines the head entry shows; requirement 29 draws a plate only for the entry a card shows (revised after the Phase 7 review).
- The domain card's ETag includes the plate; the entry card's stays `"<id>-<letter>"` (an entry is immutable; a swept predecessor's fixed tones may linger for a client holding the old ETag).
- The card's fixed line is the page's hollow 5 px outline with its ring, over a solid 2 px stroke; the card carries no axis group names (its 1104 px plate keeps the page's line geometry, §5.4) (Phase 7 review).

## Phase 8 — Behavioural verification and screenshots

### Groups

G1: C2 (LENS_EXTRA), C4 (SDD) · G2: C1, C3, C5–C10, C12 (Playwright run on the seeded stack and the 390 px CSS fixes it needs). C11 holds already (`history_fixtures.rs`).

### Plan

G1: `justfile` `acceptance-local`: the lens row's extra env appends `${LENS_EXTRA:-}`; `tests/acceptance/playwright.config.ts`: project `result-page` (testDir `./result-page`); `specs/sdd/v2.md`: Phase 4 rows V4.5, V4.6, V4.13 "built by `v2-result-page`", §5.1 the result routes mount on lens's router until `v2-cutover`, §5.2 a stale page's visit posts `trigger=visit` and a fresh visit starts nothing.
G2: `npm ci` in tests/acceptance (no browser install; Chromium revision 1217 is linked locally); run `LENS_EXTRA="LENS_ADMISSION__CLIENT_BUDGET_PER_HOUR=0 LENS_CACHE__TTL_SECONDS=3600" just acceptance-local result-page/`; fix what fails at 390 px in `crates/web/static/pages.css` (and the result partials only where markup must change), including the IP group name spilling past the plate (Decision log, v2-spectrum review); copy the screenshots to the scratchpad.

Decision: the fixtures' grades are set by hand in the fixture files, as v2-inspectors' `example.com.json` is, over scoring them: the seed stores the run as given and no check derives the grade from the statuses (implementer under the operator's autonomy mandate, 2026-10-19).
