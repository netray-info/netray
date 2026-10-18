# Spec: v2-result-page

Status: Ready for Implementation
Created: 2026-10-17

## Goal

On the process that serves the apex (lens until `v2-cutover`), `GET /<domain>` renders the domain's
latest history entry as full HTML in the order of SDD §5.7, and neither starts a run nor writes the
store. With JavaScript on, a page whose entry is stale or missing posts `/<domain>/run`. v2-pages'
`stream` island then replaces elements by ID with SSE fragments that the full page's own templates
render, section by section, with grade and diff last. When admission refuses the run, the page shows
S26's reason and a countdown instead. The `spectrum` island switches the trace, replay, pin and run
stepper between states the server computed in advance. `/<domain>/history` lists every entry. The
open issues list each warn and fail once, with its finding, fix hint, a link into v2-inspectors'
evidence view of the same entry and a guide link. Their grouping by owner, severity or protocol is
kept in browser storage. `/` is the idle page, and its recent domains live only in the browser. The
OG cards `/r/<id>/og.png` and `/og/<domain>.png` draw the entry's spectrum from the same
`spectrum::layout`. At 390 px the header, the diff and the issues need no sideways scroll, and every
target in them is at least 44 px. A Playwright run against `just acceptance-local` writes screenshots
of every state at 390 px and 1440 px. This feature covers SDD criteria V4.5, V4.6 and V4.13, and
V4.4's "used by the OG card" (`specs/sdd/v2.md`, Phase 4 table; operator decision X4, 2026-10-17).

## Non-goals

- Evidence views at `/<domain>/<protocol>` and `/r/<id>/<protocol>`, explore-live links and the
  inspectors (V4.7): `v2-inspectors`. This feature links to the evidence views and reuses their
  section renderer for a run that has no URL.
- `base.html`, the CSP, `/static/*`, the islands scaffold and its `stream` island, static pages,
  the `frontend-rules.md` rewrite and the codename check (V4.8–V4.12, `v2-pages`; X3).
- The spectrum geometry and label placement (V4.4, `v2-spectrum`). This feature only maps stored
  runs onto it and renders it in the page, the trace and the OG card.
- Admission, run join, throttle and history as cache (V2.6, `v2-admission`). This feature calls its
  public `check_limits` and `admit` through `DomainRuns::start` (requirement 19). `/r/<id>`, its V1-layout OG card and its 410 (V2.7, `v2-entries`).
  API v1 and the `RunCheck` findings and evidence references (V3.x, `v2-api`; X2).
- Host routing, `netray serve`, deleting the lens SPA, the `lens.` redirects and §5.1's
  reserved-path test (V5.1, V5.3, `v2-cutover`).
- The §9.2 metrics (V5.4) and the cross-view agreement test (V5.5).
- IDNA U-label input. The page accepts what `validate_domain` accepts (default, not interviewed).
- A release. The work ends in a local `0.30.0` (operator, 2026-10-17).

## Context and constraints

- SDD `specs/sdd/v2.md`, all of which this spec builds on:
  - §5.1 (routes, reserved paths), §5.2 (a visit), §5.3 (streaming table), §5.4–§5.6 (spectrum,
    labels, trace and replay; §5.4: "the OG card renders the same geometry with resvg"), §5.7 (page
    order and states), §5.8 (open issues).
  - §4.3 (diff classes), §4.4 (refused runs, S26/S26a; re-check bypasses the window, never the
    throttle), §6.1 (the run line wording "run of … · entry <id>"; an incomplete run's evidence has
    no URL).
  - §8.3 (components), §8.4 (islands).
  - S17 (5-minute cache window), S19 (a visit), S20, S21 (trace of 24 entries, the table holds
    all), S22, S25 (own streaming island, strict CSP), S26, S26a (every refusal named with a
    time), S35 (F in an incomplete run).
- Operator decisions of 2026-10-17 (`DECISIONS.md`): X1 routers and API v1 live in `netray-web`,
  and lens mounts netray-web's routers ahead of its SPA fallback until the cutover; the first
  feature that needs it lifts the Phase 4a guards of `tests/repo/test_web_crate.sh` under
  `ADLC-Test-Change`. X2 `RunCheck` findings and evidence references come from `v2-api` Phase 1.
  X3 the `stream` island is v2-pages'. X4 this feature switches the OG card to `spectrum::layout`.
  Build order (one order in every spec): lane A `v2-history` → `v2-admission` → `v2-entries` →
  `v2-api`; lane B `v2-spectrum` ∥ `v2-pages`, parallel to lane A; lane C, after the operator's UI
  review at midday 2026-10-18, `v2-inspectors` (its Phase 2, `evidence::href` and
  `evidence::section`, before this feature's Phase 1), then this feature; then `v2-cutover`. Lanes
  A and B are merged when this feature starts.
- Today the apex is lens. Its router ends in `.fallback(spa::handler)` (`crates/lens/src/lib.rs:152`),
  which serves the SPA shell (`crates/lens/src/spa.rs:22`, `render_apex_html`).
- History API as built by `v2-history` (`crates/store/src/lib.rs`):
  - `HistoryStore::latest` (:208), `get` (:214) and `entries` (:224, newest first, all entries).
  - `record` (:153) returns `Recorded::{First, Identical, Changed{diff}, NotStored{diff}}` (:91).
  - `Entry` (:104) carries `id`, `domain`, `created_at`, `last_seen_at`, `predecessor_id`,
    `profile_version`, `grade`, `fingerprint`, `run`, `diff` and `engine_version`.
- Model types:
  - `Run`, `RunSection`, `SectionKind` and `RunCheck` (`crates/model/src/run.rs:12-45`). After
    `v2-api` Phase 1, `RunCheck` carries `findings: Vec<String>` and `evidence: Vec<BlockId>`
    (`#[serde(default)]`, outside the fingerprint and the diff; X2), as the live `CheckResult` does
    (`crates/model/src/lib.rs:169-174`). Entries recorded before that carry neither.
  - `BlockId::module()` names a block's owner (`crates/model/src/evidence.rs:89`).
  - `Diff`, `ChangeClass` (14 classes), `BlockChangeKind` and `GradeChange{from, rescored, to}`,
    plus `engine_changed` (`crates/model/src/diff.rs:13-75`).
- The catalogue's `CheckSpec` has `title`, `finding`, `fix_hint`, `fix_owner`, `severity`, `guide`
  and `short` (`crates/engine/src/catalogue.rs:18-30`). Guide paths are `/guide/<topic>`, and
  v2-pages keeps that segment and amends §5.1 (operator, 2026-10-17).
- Components exist once, typed against view structs (`crates/web/src/components.rs`): `GradeView`,
  `RunLineView`, `ProgressRayView`, `SpectrumView`, `TraceView`, `CheckRowView` (with `since`,
  `fix`, `evidence_href` and `guide_href`), `DiffBlockView`, `GroupingSwitchView`, `QueryInputView`.
  The gate (`crates/web/src/gate.rs`) scans every new template, stylesheet and island; islands may
  not write a style (v2-pages requirement 7). Identifier-valued CSS properties go into
  `IDENTIFIER_PROPERTIES` (CLAUDE.md).
- The engine sends each finished section on a channel (`crates/engine/src/lib.rs:261`, `run`, with
  `SectionEvent` at :217). lens collects the sections today (`crates/lens/src/check.rs:112`).
- lens's OG cards render with resvg over a font database holding only Inter
  (`crates/lens/src/og/fonts.rs:5-6`); the layout's 0.62 em label measure assumes a mono face.
- lens config is loaded with the env prefix `LENS_` and `__` between section and key
  (`crates/lens/src/config.rs:463-475`).
- `tests/repo/test_header_parity.sh` starts lens and expects, for `GET /` and `GET /health`, the
  shared header set with lens's CSP `script-src 'self' 'unsafe-inline'` (`:39`, `:74-90`). Once
  the result router answers lens's `/`, that CSP is v2-pages' strict one (requirement 1), so the
  script changes under `ADLC-Test-Change` (requirement 31).
- Test data: `example.com`, its subdomains, and `netray.info` only (§9.1). The gate is
  `just adlc-verify`: offline, no browser. Islands are tested with vitest and happy-dom in the gate
  (§8.4). The 390 px and screenshot checks run in Playwright outside the gate (`just
  acceptance-local`, `justfile:113`).
- The operator's midday review on 2026-10-18 covers lane B (`v2-spectrum`, `v2-pages`) and opens
  lane C; it precedes this feature's screenshots. The operator approves this feature's
  user-visible copy from its own Phase 8 screenshots, whenever they exist: the feature halts for
  that approval before `finish`. Under the autonomy mandate this halt is the one that waits for the
  operator, because copy approval is the operator's (DECISIONS.md, result-page).
- Amend `specs/sdd/v2.md` in the same commit as code that changes what it says (CLAUDE.md).

**Interfaces of the sibling features this one builds on** (read from their aligned drafts of
2026-10-17, 17:46; the planner reads the merged code):

- **`v2-pages`.**
  - `netray_web::pages::router() -> axum::Router` under `crates/web/src/pages/`, with no fallback,
    no `/` route and no `/robots.txt`; lens merges it ahead of `spa::handler` (its requirements 8,
    33). The 404 fallback and the `GET /?d=` handler are exported as their own handlers (its
    requirements 14, 15). `test_web_crate.sh` then allows a `Router` only under
    `crates/web/src/entries/`, `pages/` and `api/` (its requirement 8).
  - `templates/base.html` with `styles`, `header`, `main` and `scripts` blocks; header, footer,
    `/static/app.css`, `/static/islands.js` and the speculation rules (its requirement 9). Page
    styles go into `static/pages.css` (its requirement 10).
  - The CSP with `script-src 'self' 'sha256-…'` (its requirement 11); lens's security-headers
    middleware leaves a CSP the inner response carries in place (its requirement 33).
  - `templates/pages/not_found.html` (a `query_input` of action `/` and name `d`) and
    `server_error.html` (its requirement 14).
  - `GET /?d=<value>`: 301 to `/<domain>` for a plausible value, 400 with the input refilled
    otherwise; full validation is left to the result route (its requirement 15).
  - The islands package `crates/web/islands/` (esbuild, `src/main.ts` mounting each registered
    island on every `data-island` element once, vitest with happy-dom, the 30 KB gzipped test,
    gate rule 1 refusing style writes; its requirements 4–7).
  - The `stream` island (X3; its requirement 28) on a `<form data-island="stream">`: it posts the
    form's fields to `data-stream-url` with `Accept: text/event-stream`, adding `trigger` (the
    submit button's value when it is named `trigger`, else the value of `data-stream-autostart`);
    it posts once on mount only when the form carries `data-stream-autostart="<trigger>"`, waiting
    out `document.prerendering`; it applies `fragment` `{"id", "html"}` events by replacing the
    element with that ID (scripts removed, `mount()` run over the inserted nodes) whatever the HTTP
    status, so a `429` refusal's fragment shows; `end` ends the stream. The form carries
    `data-stream-state` (`running`, `done`, `error`), and the island dispatches
    `netray:stream-fragment` and `netray:stream-end`; consumer behaviour builds on those.
  - The server half `netray_web::stream::fragment(id, html)` and `stream::end()` (its requirement
    29): every stream route emits its events through them.
- **`v2-admission`** (its requirements 1, 6, 13, 15, 18, 20, 21).
  - `check_limits(&state, client) -> Result<(), Refusal>` (per-IP and global), and
    `admit(&state, domain, client, trigger, prefetch) -> Admit` in `crates/lens/src/admission.rs`,
    running join, prefetch rule, throttle, slot, budget, start. `Admit` is `Admitted(RunStream)`
    (started or joined), `Refused(Refusal)` or `NotStarted` (prefetch). A caller with no cache step
    calls `check_limits` before `admit`; lens's `DomainRuns::start` (v2-api) is that caller.
  - `Trigger` is `visit | recheck | api | badge | og`; `visit`, `recheck` and `api` spend the
    budget. `admit`
    never reads the cache window: the window is the caller's step, and a re-check skips it, so a
    re-check bypasses the window and never the throttle (§4.4).
  - `Refusal { reason: throttle | budget | ip_limit | global_limit | slots, retry_after_s,
    last_run_ago_s (throttle only), last_incomplete }`.
  - A `RunStream` yields one `Section(netray_engine::SectionEvent)` per finished section (the
    engine's event: `protocol` and an outcome carrying the section's checks, each a
    `netray_model::CheckResult` with findings and evidence references, and its blocks, or the
    not-applicable, incomplete or timed-out outcome; a joiner first gets those already finished),
    then `Done` (the published result and the `Run`), then `Recorded(Option<Recorded>)`.
  - The cache window is `[cache] ttl_seconds` (300 s): a hit is `now - last_seen_at <
    ttl_seconds`. `netray_common::prefetch::is_prefetch(&HeaderMap)` decides the prefetch flag.
  - `[admission] client_budget_per_hour` (30; 0 is valid and refuses every budgeted run with
    `retry_after_s` 3600), set by `LENS_ADMISSION__CLIENT_BUDGET_PER_HOUR`.
  - `/og/<domain>.png` renders the head entry: grade, percentage and `last_seen_at` (its
    requirement 11).
- **`v2-api`.**
  - Phase 1 adds `RunCheck.findings: Vec<String>` and `RunCheck.evidence: Vec<BlockId>`
    (`#[serde(default, skip_serializing_if = "Vec::is_empty")]`, outside the fingerprint and the
    diff; X2), copied by `lens::history::build_run`.
  - The trait `netray_web::api::DomainRuns` (its requirement 1), defined by v2-api with
    netray-web's own types because netray-web cannot depend on lens, and implemented by lens:
    - `validate(domain) -> Result<String, DomainRefusal>`, over `lens::input::validate_domain`,
      codes `DOMAIN_INVALID` and `DOMAIN_BLOCKED`;
    - `start(domain, client, trigger: RunTrigger, prefetch) -> RunStart`, lens's `check_limits`
      then `admit`: `RunTrigger` is `Visit | Recheck | Api`; `RunStart` is `Admitted(RunEvents)`,
      `Refused(RunRefusal)` or `NotStarted`; `RunRefusal` is `{reason: Throttle | Budget | IpLimit |
      GlobalLimit | Slots, retry_after_s, last_run_ago_s, last_incomplete}`; `RunEvents` yields
      `Section(netray_engine::SectionEvent)`, `Done(Run)` and `Recorded(Option<Recorded>)`. It reads
      no cache window.
    - `ApiState`'s client-address extractor (lens's `ip_extractor`, `CF-Connecting-IP` dropped),
      which keys every per-client limit (its requirement 9).
- **`v2-entries`.** `entries::router(EntriesDeps)` serves `/r/<id>` (history first, then a live V1
  snapshot, 410 for a well-formed ID in neither) and `/r/<id>/og.png`, whose card renderer lens
  passes in `EntriesDeps` and draws in the V1 layout without a score line (its requirements 8, 10,
  15). `entries::privacy_router()` serves `/privacy`. lens mounts both ahead of `spa::handler`.
- **`v2-spectrum`.**
  - `netray_web::spectrum::layout(current, older, catalogue, width)`: each run as its per-check
    statuses (`CheckId` → `Status`) with the `&Profile` it was scored with, `older` newest first;
    per run its per-section states (`Protocol` → `spectrum::SectionState`: `Measured`,
    `NotApplicable`, `Unmeasured`; a caller maps `Incomplete` and `TimedOut` to `Unmeasured`), and
    per run each line's tone, height, labels and row count; x shared by every run (its requirement
    10). One conversion builds the view from a layout (its requirement 15); the page renders the
    placement computed at `n × linePitchMin` (operator, 2026-10-17).
  - Bands and per-run states (its requirement 18): one `<g id="band-<protocol>"
    data-protocol="<protocol>">` per protocol; inside it one `<g class="spectrum__run"
    data-run="<k>">` per run (k = 0 current, then newest first) and one `spectrum__labels`
    `data-run="<k>"` set per run; the shown run's groups and label set carry `data-shown` (the
    stylesheet hides `[data-run]:not([data-shown])`); a band's run group whose `SectionState` for
    that protocol is `Unmeasured` (never `NotApplicable` or `Measured`) carries
    `data-unmeasured`.
  - Left to this feature: mapping a stored `Entry` onto statuses with profiles, the island, the
    trace's interaction, and group-name placement at 390 px.
- **`v2-inspectors`** (its I8 and requirements 2–4, 6, 7, 9).
  - The evidence views `GET /r/<id>/<protocol>` and `GET /<domain>/<protocol>`; every block carries
    `id="ev-<block id>"` in its owner's view, and another protocol's view links there.
  - `netray_web::evidence::href(entry_id, protocol, block: Option<&BlockId>)` (`/r/<id>/<protocol>`
    plus `#ev-<block id>`), the one source of the result page's evidence links.
  - `netray_web::evidence::section(run, protocol, run_line) -> EvidenceView` with its partial
    `templates/partials/evidence_section.html`, for an incomplete run's evidence, which no URL
    reaches (§6.1).
  - The inspector pages at `/inspect/<tool>` for DNS, TLS, HTTP and IP.
  - Its acceptance seed (its requirement 23): `just acceptance-local` runs `cargo run -p
    netray-store --example seed`, which records every JSON run fixture of
    `tests/acceptance/fixtures/history/` through `HistoryStore::record` into lens's
    `LENS_HISTORY__DB_PATH` before lens starts, at times relative to the seed run's clock
    (requirement 27's fixture contract), and a Rust test holds every fixture run to
    the catalogue; the pages are reached at `LOCAL_LENS_URL`.

## Requirements

1. `netray_web::pages::result::router(ResultState) -> axum::Router` (`crates/web/src/pages/result/`)
   serves `GET /`, `GET /{domain}`, `POST /{domain}/run` and `GET /{domain}/history`, inside the
   `pages/` directory that `test_web_crate.sh` allows (X1). `ResultState` holds the `HistoryStore`,
   the `DomainRuns` implementation of requirement 19, the client-address extractor lens passes (as
   v2-api's `ApiState` holds it) and the cache window.
   - A first segment without a dot is never a domain, and neither are §5.1's root files
     (`robots.txt`, `sitemap.xml`, `favicon.svg`, `favicon.ico`), the segment `.well-known` (§5.1's
     `/.well-known/` prefix) and `bimi-logo.svg` (served at the apex by `v2-cutover`, its
     requirement 3). The result router matches none of them, so they keep lens's handlers.
   - `GET /` with `d` is answered by v2-pages' exported `?d=` handler (its requirement 15); without
     `d` it answers the idle page (requirement 24).
   - lens merges the result router ahead of `spa::handler`, beside v2-pages', v2-entries' and
     v2-api's routers (X1), so lens's `/` and `/<domain>` become these pages. Every other path keeps
     today's handler until `v2-cutover`.
   - Every response of these routes carries v2-pages' CSP (its requirement 11), which lens's
     middleware leaves in place (its requirement 33), so `script-src` holds neither
     `'unsafe-inline'` nor `'unsafe-eval'`. This changes lens's `GET /` CSP, which
     `test_header_parity.sh` pins (requirement 31).
2. The domain segment passes `DomainRuns::validate` (v2-api; lens implements it with
   `lens::input::validate_domain`).
   - A refused segment answers `400` with v2-pages' not-found template, the reason and the input
     refilled.
   - A valid segment whose normalised form differs (upper case, trailing dot) answers a GET with
     `301` to `/<normalised>`, and a POST with 400 (default, not interviewed).
3. `GET /{domain}` renders the latest entry as full HTML on v2-pages' `base.html` in the order
   header (grade, domain, run line, re-check now, share) → spectrum with trace and legend → since
   last check → open issues → evidence → passed checks in a closed `<details>`.
   - It never calls admission, never starts a run, and writes nothing to the store.
   - Without an entry it answers 200 with "not checked yet" and a check button (default, not
     interviewed).
4. The run line of a stored entry reads "run of <YYYY-MM-DD HH:MM> UTC · entry <id>" (§6.1), with
   `created_at` as v2-inspectors' run line uses it. The page root carries `data-state`:
   - `fresh` when `now - last_seen_at < [cache] ttl_seconds` (v2-admission requirement 6, S17);
   - `stale` when it is older;
   - `none` without an entry.
5. The header's share control is a link to `/r/<id>` (v2-entries). It is absent without a stored
   entry and for an incomplete run (default, not interviewed).
6. "Re-check now" and the check button are a form posting `trigger=recheck` (or `visit`) to
   `/<domain>/run`.
   - A POST without `Accept: text/event-stream` runs or joins to the end, then answers `303` to
     `/<domain>`.
   - When refused, it answers `429` with `Retry-After` and the full page carrying requirement 21's
     line (default, not interviewed).
7. The open issues hold every warn and fail check of the shown run exactly once, and no other
   status. Each row is a `check_row` with:
   - the catalogue `title`; as body the check's stored `findings` (X2), or the catalogue `finding`
     when it has none; the catalogue `fix_hint`;
   - an evidence link (requirement 8);
   - the catalogue `guide` as its guide link.
8. An issue's evidence link stays in the same run and points into v2-inspectors' evidence views.
   - A check of a stored entry with evidence references links to
     `evidence::href(id, <owner>, Some(<first block id>))`, that is `/r/<id>/<owner>#ev-<block id>`,
     where `<owner>` is `BlockId::module()` (the view that renders the block; v2-inspectors I8).
   - Without references, as in entries recorded before `v2-api` Phase 1, it links to
     `evidence::href(id, <protocol of the check>, None)`.
   - In a run that was not stored (an incomplete run; it has no URL, §6.1) it links to
     `#ev-<block id>` inside the page's evidence section (requirement 14), or to
     `#evidence-<protocol>` without references.
9. The server renders the issues grouped by `fix_owner`, under §3.5's fixed labels.
   - Owner groups follow the `FixOwner` order dns_provider, registrar, certificate_provider,
     web_server, mail_provider, hosting (default, not interviewed).
   - Severity groups run critical → low, protocol groups DNS, TLS, HTTP, email, IP.
   - Inside a group: severity first, fail before warn at equal severity (§5.8), then catalogue
     order.
   - Every row carries `data-owner`, `data-severity`, `data-protocol`, `data-status` and
     `data-order`.
10. The `issues` island, registered in v2-pages' `main.ts`, regroups the rows in place from those
    attributes, with no request.
    - It stores the choice under the `localStorage` key `netray.grouping` (default, not
      interviewed), with every access in try/catch, and applies a stored choice on load.
    - The `grouping_switch` is rendered `hidden` and unhidden by the island. The pressed option
      carries the marker (R43).
11. The diff block renders a stored `Diff`:
    - One row per `CheckChange`, labelled by its class: fixed, improved, new, worse, now
      applicable, no longer applicable, into/out of not tested, into/out of not measured, added,
      removed, added by release, removed by release. Each row is toned and names the check's title.
    - One row per `BlockChange`: added, removed, or changed with its item count.
    - The grade as `B → A`. With `rescored`, two steps: predecessor → re-scored ("profile effect")
      → current ("domain effect").
    - A note when `engine_changed`.
    - An entry without a diff reads "first state recorded — re-check after your change" (R19). An
      entry whose predecessor was swept still shows its stored diff (§4.5).
12. After a fresh run, the diff block shows:
    - `Changed`: its diff.
    - `Identical`: the latest entry's stored diff, plus the note "unchanged since <created>"
      (default, not interviewed).
    - `First`: requirement 11's R19 line.
    - `NotStored`: its diff marked "incomplete, not recorded", or "not recorded: this run is
      incomplete" when there is none (R19).
13. The passed-checks `<details>` lists the pass checks, then the not_applicable checks (with their
    section's reason), the not_tested checks and the unmeasured checks ("could not check", S34).
    Every catalogue check of the run thus appears exactly once, either in the open issues or in
    the details (default, not interviewed).
14. The evidence section holds one element per protocol, `id="evidence-<protocol>"`.
    - For a stored entry it is a link to v2-inspectors' view `/r/<id>/<protocol>` with the
      section's outcome (measured, or not applicable or incomplete with its reason). No block is
      rendered in the page.
    - During a stream, and at the end of a run that was not stored, it is v2-inspectors'
      `evidence::section(run, protocol, run_line)` partial over the live run, the same markup as
      the stored view. Before `Done` the live run is a partial `Run` that one function of
      `views.rs` assembles from the `Section` events received so far (sections, checks with their
      findings and evidence references, blocks; not complete, grade `incomplete`); after `Done` it
      is the `Done` event's `Run`.
    - When the run recorded an entry (`First`, `Identical`, `Changed`), the end fragments replace
      each element with the stored entry's link (the `id` the `Recorded` variant carries; for
      `Identical`, the latest entry's).
15. The spectrum and trace come from `spectrum::layout` (v2-spectrum) over the latest entry and at
    most the 23 entries before it (24 in all, S21), newest on top.
    - Each entry's `run.checks` maps onto `CheckId` → `Status`, and its `run.sections` onto
      `Protocol` → `SectionState` (`SectionKind::Measured` → `Measured`, `NotApplicable` →
      `NotApplicable`, `Incomplete` and `TimedOut` → `Unmeasured`; v2-spectrum requirement 10),
      with `Profiles::get` of the entry's `profile_version`; this mapping is one function in
      `netray-web`, used by the page, the trace and the OG card.
    - The page renders v2-spectrum's bands and per-run groups (its requirement 18): `band-<protocol>`
      groups, one `data-run` group and label set per shown entry, `data-shown` on run 0's groups
      and label set, `data-unmeasured` on a band's run group exactly when that run's section is
      `Unmeasured`. The trace rows carry the same `data-run` index.
    - The result template wraps the spectrum component in `<figure id="spectrum">`, the element
      the stream's spectrum fragments replace (requirement 19).
    - A legend names pass, warn, fail, fixed and muted.
16. The server computes one history sentence per line over the shown entries and carries it as a
    data attribute.
    - It collapses consecutive equal statuses into spans of `MM-DD` dates of `created_at`, the
      last as "since", e.g. "warn 09-19 to 09-24 · pass since 10-08" (§5.6).
    - An issue's `since` is the start of its current span when that start lies after the oldest
      shown entry. Otherwise it is omitted (default, not interviewed).
17. The `spectrum` island holds exactly three values, the hovered run, the pinned run and the
    selected line, and writes only `data-*`, `aria-*`, class attributes and text. It shows a run by
    moving `data-shown` to that run's `data-run` groups and label set (v2-spectrum requirement 18).
    - On desktop, hovering a trace row enlarges it, shows its date and grade in the gutter and
      shows that run's precomputed lines and labels. Leaving the row returns to
      the pinned or current run.
    - A click pins a run (marker, `aria-pressed`); a second click releases it. Hovering a line
      shows its sentence.
    - At the phone width (`layout.phoneMax`, 480 px), a run stepper ‹ › with 44 px buttons
      replaces hover, and tapping a line's full-height target shows its sentence.
18. `GET /{domain}/history` lists every entry, newest first. Its columns:
    - the entry, linked to `/r/<id>`;
    - created and last seen;
    - grade and profile version;
    - changes: the count of check changes in the stored diff, or "first" (default, not
      interviewed).

    Without an entry it answers the 404 page with "not checked yet" and a link to `/<domain>`.
19. `POST /{domain}/run` with `Accept: text/event-stream` reaches lens through
    `DomainRuns::start(domain, client, trigger, prefetch)`, which v2-api defines and lens
    implements as v2-admission requirement 20 asks of a caller without a cache step:
    `check_limits(client)` first, then `admit` (v2-api requirement 1). This feature only calls it:
    `trigger` is `RunTrigger::Visit` or `RunTrigger::Recheck` from the form field, `prefetch` is
    `netray_common::prefetch::is_prefetch` over the request headers, and the client is the address
    `ResultState`'s extractor yields, the one v2-admission's per-IP limiter keys on. The answer is
    `RunStart::Admitted(RunEvents)`, `Refused(RunRefusal)` or `NotStarted`.
    - `trigger=visit` with a fresh entry (`now - last_seen_at < [cache] ttl_seconds`) starts
      nothing: the route answers the stored entry's end fragments, then `end`. `trigger=visit`
      without a fresh entry and `trigger=recheck` call `start`; a re-check thus bypasses the window
      and never the throttle (§4.4).
    - The `RunEvents`' `Section` events become the section fragments, `Done` the end fragments,
      and `Recorded` decides the diff (requirement 12) and the evidence links (requirement 14); the
      route reads on to `Recorded` before `end`. A joiner's stream starts with the sections already
      finished, so it needs no special case.

    `NotStarted` (a prefetch, v2-admission requirement 18) answers `503` with
    `Cache-Control: no-store`. Admitted or joined, the route streams `fragment` events `{"id",
    "html"}` per §5.3 through `stream::fragment`, then `stream::end()` (v2-pages requirement 29):
    - **start:** the run line "running", the progress ray with five segments pending, and
      `#spectrum` carrying `data-refreshing` (dimmed by CSS).
    - **each section:** its ray segment, the whole `<figure id="spectrum">` re-rendered with the
      sections finished so far, the open issues known so far and its evidence element
      (requirement 14). The fragment never sends a bare SVG `<g>`: the stream island parses
      fragments in a `<template>` in the HTML namespace (v2-pages requirement 28), where an SVG
      child parsed outside its `<svg>` breaks. The re-sent figure keeps `data-refreshing`; its
      current run is the latest stored entry's run (no run without an entry) with each finished
      section's checks and `SectionState` replaced by the live ones, one function of `views.rs`,
      and those sections' bands carry `data-live`.
    - **a section that timed out or errored:** its segment incomplete and, in the re-sent figure,
      its band's current run group `data-unmeasured`.
    - Section fragments are provisional: they render from the `SectionEvent`s, which lens's
      adaptation to the `Done` run reshapes (`NotTested` → `Skip` → `not_applicable`,
      `crates/lens/src/modules.rs:393` and `crates/lens/src/score.rs:38`; the IP section as
      `NoDnsResults`, `modules.rs:408-415`; facts blocks, `crates/lens/src/history.rs:87`). Issue
      links in them point into the page's live evidence elements (`#ev-<block id>`, or
      `#evidence-<protocol>` without references).
    - **end:** the header, the run line, `#spectrum` without `data-refreshing`, the trace, the
      diff block, the open issues, the evidence elements and the passed checks, every one
      re-rendered from the `Done` event's `Run` and the `Recorded` outcome, whatever the section
      fragments showed. The open issues' evidence links then follow requirement 8 as the GET page
      renders them: `evidence::href(<entry id>, …)` when `Recorded` carries an entry id, the
      in-page anchors for `NotStored`.
20. Every fragment is rendered by the full page's templates. For a run state, the HTML of a
    fragment equals the element with the same ID in the page templates rendered for that state.
21. A refused run (S26, S26a) answers `429` with `Retry-After` and one `fragment`. That fragment
    replaces the run line with the reason and a countdown (`data-retry-after` in seconds), then the
    stream ends.
    - Throttle after an incomplete run: "a run N s ago was incomplete; a new run is possible in
      M s" (§4.4).
    - Throttle after a complete run: "checked N s ago; a new run is possible in M s" (default, not
      interviewed).
    - Budget or per-IP limit: "too many checks from your address; try again in M s".
    - Global limit or slots: "netray is busy; try again in M s".
    - The stored entry stays visible, and "re-check now" stays disabled until the countdown ends.
22. An incomplete run's end fragments show:
    - the grade `incomplete`, or F when a hard fail failed (S35);
    - the finished sections, with unmeasured bands for the rest;
    - no share link, the evidence of requirement 14, and the diff per requirement 12.
23. The page wires v2-pages' `stream` island (X3, its requirement 28):
    - The re-check form is `<form data-island="stream" data-stream-url="/<domain>/run">` with a
      submit button named `trigger` whose value is `recheck` (`visit` for the check button without
      an entry). The template writes `data-stream-autostart="visit"` on it only for a stale or
      missing entry.
    - A `countdown` island (this feature, registered in `main.ts`) ticks `data-retry-after` every
      second, writing only text and attributes, and enables re-check at 0.
    - CSS dims the bands of `[data-refreshing]` that lack `data-live`, and the page while the form's
      `data-stream-state` is `running`; no island writes a style.
    - The `spectrum`, `issues` and `countdown` islands mount on the nodes the end fragments insert,
      through the stream island's `mount()`; `recent` writes on `netray:stream-end`.
24. `/` is the idle page (§5.7):
    - The `query_input` with action `/` and name `d` (v2-pages requirement 15 answers it with a 301
      to `/<domain>`, and the result route validates; requirement 2).
    - The recent list (requirement 25).
    - The examples `example.com` and `netray.info` (default, not interviewed).
    - The tagline partial and one scope sentence (default, not interviewed).
    - Links to the four inspectors at `/inspect/dns`, `/inspect/tls`, `/inspect/http` and
      `/inspect/ip` (v2-inspectors; apex-relative, valid across the cutover).
25. The `recent` island:
    - When a page shows a final grade (a `fresh` page, or a stream's `end`), it writes `{domain,
      grade, date}` to the `localStorage` key `netray.recent`: newest first, one per domain, at
      most 8 (default, not interviewed).
    - The idle page renders that list with grade and date.
    - It sends nothing and sets no cookie. When storage fails, the list stays hidden.
26. At a 390 px viewport:
    - The document does not scroll sideways.
    - The header, the diff block and the open issues each have `scrollWidth ≤ clientWidth`.
    - Every link and button in them and in the run stepper has a hit box of at least 44 × 44 px.
    - Only the spectrum plate may scroll sideways (R26), and the group names on its axis do not
      overlap (left to this feature by v2-spectrum).
27. `just acceptance-local` seeds lens's history store before lens starts with v2-inspectors' seed
    (its requirement 23: `cargo run -p netray-store --example seed`, which calls
    `HistoryStore::record` over `tests/acceptance/fixtures/history/`). This feature adds no second
    mechanism, only run fixtures to that directory, in the seed's fixture contract:
    - Each fixture is a JSON object `{"domain": <string>, "age_seconds": <u64>, "run": <Run>}`.
      The seed reads its clock once (`seed_now`, Unix seconds) and records each fixture with
      `now = seed_now - age_seconds`, in file order (sorted file names), so `created_at` and
      `last_seen_at` are relative to the seed run and "fresh" holds on any day. A fixture with an
      absolute time field is refused, naming the file. Entries of one domain are recorded oldest
      first: a later file of the same domain has a smaller `age_seconds`, else the seed fails
      naming both files.
    - `shop.example.com`: three entries (`age_seconds` 1209600, 604800, 0), the latest fresh,
      with fixed, new and worse changes and warns and fails across owners, its checks carrying
      findings and evidence references.
    - `new.example.com`: one entry, `age_seconds` 0.
    - `stale.example.com`: one entry, `age_seconds` 7200 (last seen two hours ago).

    v2-inspectors' fixture test holds every fixture run, these included, to the catalogue, each ID
    exactly once. The seed writes `LENS_HISTORY__DB_PATH`, beside v2-inspectors' `example.com`
    entry, and the pages are reached at `LOCAL_LENS_URL`. The recipe appends the optional
    `LENS_EXTRA` (`KEY=VALUE …`) to lens's environment.
    The seed and its fixture test belong to v2-inspectors; this contract is the one its seed
    implements (flagged to v2-inspectors, 2026-10-17).
28. A Playwright spec under `tests/acceptance/result-page/`, run as
    `LENS_EXTRA="LENS_ADMISSION__CLIENT_BUDGET_PER_HOUR=0 LENS_CACHE__TTL_SECONDS=3600" just
    acceptance-local result-page/`, works at 390 × 844 and 1440 × 900. A budget of 0 refuses
    every visit or re-check run with `budget` (v2-admission requirement 13). A window of 3600 s
    keeps the `age_seconds` 0 entries fresh for the whole run and the 7200 s entry stale.
    - It asserts requirement 26.
    - It asserts that a JS-off stale page shows the stored grade and sends no POST.
    - It writes `target/screenshots/v2-result-page/<state>-<width>.png` for these states:
      - idle with recent domains;
      - `shop` fresh;
      - `shop` with a pinned run (desktop) or a stepped run (phone);
      - issues grouped by severity;
      - `new` (first state);
      - `stale` refused (the S26 line);
      - `stale` with JS off;
      - the history table.

      It also saves `shop`'s entry card `/r/<id>/og.png` as `og-shop.png`.
29. The OG cards draw the entry's spectrum from `spectrum::layout` (X4, §5.4). `/r/<id>/og.png`
    (v2-entries) and `/og/<domain>.png` (v2-admission requirement 11) render, for the entry they
    show, the layout over that entry and its predecessor (`HistoryStore::get(predecessor_id)`, none
    when swept, so no fixed line), at the card's plate width of 1104 px (default, not interviewed).
    - One function in `netray-web` turns a layout run into the card's SVG plate, from the same
      geometry the page's template renders (P26). lens's entry card renderer, which it passes in
      `EntriesDeps` (v2-entries requirement 15), and its domain card draw that plate and rasterise
      with resvg.
    - Labels are set in a mono face added to lens's OG font database (JetBrains Mono Regular,
      OFL-1.1, its licence committed beside it; default, not interviewed), so the 0.62 em measure
      holds.
    - The card's text lines stay as v2-entries and v2-admission define them; the unscored card
      (no entry) is unchanged.
30. `specs/sdd/v2.md` is amended:
    - the Phase 4 table marks V4.5, V4.6 and V4.13 as built by `v2-result-page`, and V4.4's OG card
      use as built here;
    - §5.1 notes that the result routes mount on lens's router until `v2-cutover`;
    - §5.2 notes that a stale page's visit posts `trigger=visit` and a fresh visit starts nothing;
    - §4.6 replaces v2-entries' "until v2-result-page switches it to `spectrum::layout` (V4.4)"
      (v2-entries requirement 18) with the spectrum card.

31. Under `ADLC-Test-Change: tests/repo/test_header_parity.sh -- lens's / is netray-web's result
    page with v2-pages' strict CSP`, the script's lens row expects for `GET /` the CSP that lens
    answers for a v2-pages route (`GET /guide/`) on the same instance, and asserts that its
    `script-src` holds neither `'unsafe-inline'` nor `'unsafe-eval'`. Only the CSP of lens's `/`
    changes: lens's `/health`, every other header of `/`, `/docs`, the preflight and the other
    services' rows keep today's expectations.

## Phases

## Phase 1 — Page views from an entry

**Depends on:** `v2-api` merged (X2), `v2-spectrum` merged, `v2-inspectors` merged (its Phase 2, I8, at least)
**Requirements:** 7, 9, 11, 12, 13, 15 (the run mapping), 16

`crates/web/src/pages/result/views.rs` (new): pure functions from `Entry`, `Run`, `Diff`,
`Recorded` and the catalogue to the component views: issues with grouping attributes and evidence
hrefs, diff rows, passed and other checks, history sentences, and the `Run` → statuses-with-profile
mapping for `spectrum::layout`. No I/O, table-tested.

### Test Scenarios

GIVEN a run with two warns and one fail on different owners WHEN issues are built THEN there are three rows, each once, grouped by owner, fail before warn at equal severity, each with title, body, fix hint, guide href and evidence href.
GIVEN a warn check with stored findings WHEN its row is built THEN the body is the findings; GIVEN one without findings THEN it is the catalogue `finding`.
GIVEN entry `Ab3dE5fG` whose `tls.caa_compliant` warns and references `dns.caa` WHEN its row is built THEN the evidence href is `/r/Ab3dE5fG/dns#ev-dns.caa`; GIVEN a check without references THEN it is `/r/Ab3dE5fG/<its protocol>`.
GIVEN a not-stored run with the same check WHEN its row is built THEN the href is `#ev-dns.caa`.
GIVEN a run with pass, not_applicable, not_tested and unmeasured checks WHEN built THEN none is an issue, and each appears exactly once in the passed-checks list.
GIVEN a diff holding each of the 14 `ChangeClass` values WHEN rendered THEN each yields one row with its label.
GIVEN a `GradeChange` with `rescored` WHEN rendered THEN the grade line has two steps, and without `rescored` it has one.
GIVEN an entry without a diff WHEN rendered THEN the block reads "first state recorded — re-check after your change".
GIVEN `Recorded::NotStored { diff: None }` WHEN rendered THEN the block reads "not recorded: this run is incomplete".
GIVEN statuses warn, warn, pass, pass over four entries WHEN the sentence is built THEN it reads "warn <d1> to <d2> · pass since <d3>".
GIVEN an entry with `profile_version` 3 WHEN mapped for the layout THEN every check's status is carried and the profile is `Profiles::get(3)`; GIVEN sections `measured`, `not_applicable`, `incomplete` and `timed_out` THEN their states are `Measured`, `NotApplicable`, `Unmeasured` and `Unmeasured`.

## Phase 2 — Server-rendered pages

**Depends on:** Phase 1, `v2-pages` merged, `v2-entries` merged, `v2-api` merged (`DomainRuns`)
**Requirements:** 1, 2, 3, 4, 5, 14, 15, 18, 24, 31

`crates/web`: the templates `pages/result.html`, `pages/history.html` and `pages/idle.html` on
`base.html`, the result router with `GET /` (delegating `?d=` to v2-pages' handler); `crates/lens`
merges the router ahead of `spa::handler`. Tests use a temporary `HistoryStore` and drive the router
(CLAUDE.md: test invalid input through the router).

### Test Scenarios

GIVEN a stored entry for `shop.example.com` WHEN `GET /shop.example.com` THEN the HTML holds header, spectrum, diff, issues, evidence and details in §5.7 order, the run line names the entry's id, and the store and admission saw no call.
GIVEN an entry last seen 10 minutes ago WHEN the page renders THEN `data-state="stale"` and the stream form carries `data-stream-autostart="visit"`; GIVEN 1 minute ago THEN `fresh` and no autostart; GIVEN none THEN `none`, with "not checked yet", a check button and autostart.
GIVEN a stored entry WHEN the page renders THEN each `evidence-<protocol>` element links `/r/<id>/<protocol>` and no `evidence_block` markup is present.
GIVEN `GET /Shop.Example.com.` THEN it answers 301 to `/shop.example.com`; GIVEN `GET /1.2.3.4` THEN it answers 400 with the reason `DomainRuns::validate` gave.
GIVEN a stored entry WHEN the page renders THEN `#spectrum` holds five `band-<protocol>` groups, one `data-run` group per shown entry in each, and `data-shown` only on run 0's groups and label set; GIVEN an older entry whose email section timed out and whose TLS section is not applicable THEN that run's `band-email` group carries `data-unmeasured` and its `band-tls` group does not.
GIVEN lens's app WHEN `GET /robots.txt`, `GET /r/abcdefgh` and `GET /api/check/example.com` THEN none is answered by the result router.
GIVEN 30 entries WHEN `GET /shop.example.com/history` THEN 30 rows link to `/r/<id>`; the trace on the result page shows 24.
GIVEN `GET /` THEN the idle page holds the input (action `/`, name `d`), the examples, the tagline and the four `/inspect/<tool>` links; GIVEN `GET /?d=Example.com` THEN 301 to `/example.com`.
GIVEN any response of the result routes THEN its CSP equals v2-pages' and `script-src` has no `'unsafe-inline'`.
GIVEN lens's `GET /` WHEN `tests/repo/test_header_parity.sh` runs THEN it expects v2-pages' CSP there, without `'unsafe-inline'`, keeps lens's CSP on `/health`, and passes.
GIVEN lens's app WHEN `GET /favicon.svg`, `GET /favicon.ico`, `GET /bimi-logo.svg` and `GET /.well-known/mta-sts.txt` THEN none is answered by the result router.

## Phase 3 — Run route and fragments

**Depends on:** Phase 2, `v2-admission` merged
**Requirements:** 6, 19, 20, 21, 22

`crates/web`: `POST /{domain}/run` over v2-api's `DomainRuns::start`, the partial `Run` from
section events, fragments rendered by the page's templates and emitted through `stream::fragment`,
the refusal answer and the no-JS form answer. Router tests use a stub `DomainRuns`; one lens test
drives lens's implementation (v2-api's) through the route on tokio's paused clock.

### Test Scenarios

GIVEN stub modules finishing dns, tls, http, email and ip in that order WHEN POSTed with SSE THEN the events are start, five section groups in that order, and the end fragments, followed by `end`.
GIVEN a finished run WHEN each end fragment is compared with the page templates rendered for the same run THEN the HTML of each element ID is equal, and the end fragments include `#spectrum` and the open issues.
GIVEN any section fragment WHEN its HTML is parsed in a `<template>` THEN its root is the `<figure id="spectrum">` element (or a ray segment, issues or evidence element), never an SVG `<g>`, and the bands of finished sections carry `data-live`.
GIVEN a run whose live `tls` section reports a check `not_tested` and whose `Done` run carries it `not_applicable` WHEN the end fragments arrive THEN the spectrum, issues and passed checks show `not_applicable`.
GIVEN a run recorded as `Changed` with a warn referencing `dns.caa` WHEN the end fragments arrive THEN its issue links `/r/<new id>/dns#ev-dns.caa`, as the GET page does; GIVEN `NotStored` THEN it links `#ev-dns.caa`.
GIVEN a stub where the email section times out WHEN streamed THEN its ray segment is incomplete, its band's current run group carries `data-unmeasured`, the email evidence element is the live section partial, and the end header reads `incomplete`; GIVEN a failed hard fail as well THEN it reads F.
GIVEN a complete run recorded as `Changed` WHEN the end fragments arrive THEN each evidence element links `/r/<new id>/<protocol>`.
GIVEN a run in flight with two sections done WHEN a second request joins THEN it first receives those two sections' fragments, then the rest live.
GIVEN a fresh head entry WHEN POSTed with `trigger=visit` THEN `start` is not called and the stored entry's end fragments and `end` are answered; GIVEN `trigger=recheck` THEN `start` is called with `recheck`.
GIVEN lens's implementation (v2-api's) and a run of the domain 20 s ago WHEN `trigger=recheck` is POSTed THEN it is refused `throttle` with `last_run_ago_s` 20; GIVEN the per-IP limit spent THEN it is refused `ip_limit` before `admit` runs.
GIVEN admission refuses with `throttle`, `last_incomplete`, 40 s ago, retry 20 s WHEN POSTed THEN it answers 429, `Retry-After: 20` and one fragment reading "a run 40 s ago was incomplete; a new run is possible in 20 s".
GIVEN refusals for budget, per-IP limit, global limit and slots THEN the sentences are the address one, the address one, the busy one and the busy one.
GIVEN `Sec-Purpose: prefetch` and `start` answering `NotStarted` WHEN POSTed THEN 503 with `Cache-Control: no-store`.
GIVEN a form POST without the SSE accept WHEN the run completes THEN it answers 303 to `/<domain>`; GIVEN a refusal THEN it answers 429 with the full page and the line.

## Phase 4 — Stream wiring and countdown

**Depends on:** Phase 3, v2-pages' `stream` island
**Requirements:** 23

`crates/web/islands/src/countdown.ts`, its registration and vitest tests; the `[data-refreshing]`
rule in `static/pages.css`; a vitest integration test of the stream island over markup the Phase 3
templates rendered, with a fetch stub yielding SSE text.

### Test Scenarios

GIVEN a rendered stale page and a stubbed SSE answer WHEN the islands mount THEN the stream island posts once with `trigger=visit`; GIVEN a fresh page THEN it posts nothing; GIVEN the re-check button pressed THEN it posts `trigger=recheck`.
GIVEN a stubbed `429` stream with one refusal fragment WHEN applied THEN the run line shows the reason and the countdown island is mounted on it.
GIVEN the start fragment WHEN inserted THEN the spectrum carries `data-refreshing`; GIVEN the end fragment THEN it no longer does.
GIVEN a refused fragment with `data-retry-after="2"` WHEN two ticks pass THEN the countdown text reads 0 and re-check is enabled.
GIVEN the end fragments WHEN inserted THEN the `spectrum`, `issues` and `countdown` islands are mounted on the new nodes.
GIVEN the countdown island's source WHEN the gate runs THEN it passes rule 1 (no style writes).

## Phase 5 — Spectrum island

**Depends on:** Phase 2
**Requirements:** 17

`crates/web/islands/src/spectrum.ts` with vitest over markup that the Phase 2 templates rendered.

### Test Scenarios

GIVEN three trace rows WHEN row 2 is hovered THEN every line's `data-tone` and label equal row 2's precomputed state; WHEN the pointer leaves THEN the current run's state returns.
GIVEN row 2 clicked WHEN row 3 is hovered and left THEN row 2's state returns; WHEN row 2 is clicked again THEN it is released.
GIVEN the phone width WHEN › is pressed twice THEN the second-newest-but-one run is shown; GIVEN a line tapped THEN its sentence is shown.
GIVEN any interaction WHEN the DOM is diffed THEN only `data-*`, `aria-*`, class attributes and text changed.

## Phase 6 — Issues and recent islands

**Depends on:** Phase 2
**Requirements:** 10, 25

`crates/web/islands/src/issues.ts` and `recent.ts` with vitest; the islands bundle stays under
v2-pages' 30 KB test.

### Test Scenarios

GIVEN the default grouping WHEN "severity" is pressed THEN rows regroup critical → low with no request made, and `netray.grouping` is `severity`; GIVEN a reload THEN severity is applied.
GIVEN `localStorage` throws WHEN the island loads THEN the default grouping stays and nothing throws.
GIVEN nine result pages shown WHEN the idle page loads THEN eight recent domains are listed, newest first, one per domain, and no request was made.
GIVEN the bundle with all islands of this feature WHEN v2-pages' size test runs THEN it passes.

## Phase 7 — OG cards on the layout

**Depends on:** Phase 1, `v2-entries` merged, `v2-admission` merged
**Requirements:** 29

`crates/web/src/spectrum.rs` (or a sibling module): the card plate SVG from a layout run.
`crates/lens/src/og/`: both cards draw the plate, the predecessor read for fixed lines, the mono
face in the font database. SDD §4.6 and V4.4's row.

### Test Scenarios

GIVEN `shop.example.com`'s latest entry WHEN `/r/<id>/og.png`'s SVG is built THEN it holds 54 lines and exactly the labels of its warn, fail and fixed lines, and the PNG is 1200×630.
GIVEN an entry whose predecessor had `http.hsts` fail and the entry pass WHEN the card is built THEN the `http.hsts` line is fixed and labelled; GIVEN a swept predecessor THEN no line is fixed.
GIVEN the same entry WHEN the page's layout run and the card's are compared THEN each line's x as a fraction of the plate width and its tone are equal.
GIVEN `/og/shop.example.com.png` with a fresh head THEN the card shows the head entry's spectrum and v2-admission's text lines; GIVEN no entry THEN the unscored card is unchanged.
GIVEN the OG font database WHEN initialised THEN it holds the Inter faces and one mono face.

## Phase 8 — Behavioural verification and screenshots

**Depends on:** Phase 4, Phase 5, Phase 6, Phase 7
**Requirements:** 26, 27, 28, 30

This feature's run fixtures in `tests/acceptance/fixtures/history/` (seeded by v2-inspectors'
`netray-store` example), the `LENS_EXTRA` pass-through in `acceptance-local`, `tests/acceptance/result-page/*.spec.ts`, any CSS
fixes for 390 px, and the SDD amendments. The run's `report.md` lists the screenshot paths. The
feature then halts for the operator's review of these screenshots and approval of the copy before
`finish`, whenever they exist; the 2026-10-18 midday review (lane B) is not this review.

### Test Scenarios

GIVEN the seeded stack at 390 px WHEN `/shop.example.com` loads THEN the document does not scroll sideways, header, diff and issues have `scrollWidth ≤ clientWidth`, every target in them is at least 44 × 44 px, and no two axis group names overlap.
GIVEN a JS-off context WHEN `/stale.example.com` loads THEN the stored grade shows and no POST is sent.
GIVEN a client budget of 0 WHEN `/stale.example.com` loads with JS THEN the run line shows "too many checks from your address; try again in M s".
GIVEN desktop WHEN a trace row of `shop.example.com` is clicked THEN the spectrum shows that run and the row carries the marker.
GIVEN desktop WHEN an issue's evidence link on `/shop.example.com` is followed THEN v2-inspectors' view of the same entry opens at the referenced block.
GIVEN the spec finished WHEN `target/screenshots/v2-result-page/` is listed THEN it holds all eight states at both widths and `og-shop.png`.
GIVEN a fixture run missing one catalogue ID WHEN the fixture test runs THEN it fails naming the ID.
GIVEN the stack seeded on any day WHEN `/shop.example.com` and `/stale.example.com` load THEN `data-state` is `fresh` and `stale`.

## Decision log

- The result routes mount on the router that serves the apex today (lens, ahead of
  `spa::handler`) in this feature. The Host switch, `netray serve`, deleting the SPA and the
  `lens.` redirects stay in `v2-cutover`. V4.13 asks for `netray.info/<domain>`, and lens already
  is the apex, so no infrastructure change is needed. Replacing the SPA's `/` early harms nobody,
  because nothing is released before the local `0.30.0` (implementer, operator decision of
  2026-10-17).
- Routers in `netray-web`, mounted by lens until the cutover, over a separate crate or routes in
  lens: SDD §3.1 places them there (operator, 2026-10-17, X1).
- The page reads `netray-store` directly, over calling API v1: S7 renders in Rust on the server,
  and the API adds nothing the store lacks (R5).
- `RunCheck` findings and evidence references come from `v2-api` Phase 1 over adding them in this
  feature: one additive model change serves the API, the result page and the inspectors
  (operator, 2026-10-17, X2).
- Evidence linked into v2-inspectors' evidence views over in-page panels per protocol: one evidence
  rendering per run and no duplicated panels; only a run that was not stored, which has no URL
  (§6.1), shows v2-inspectors' section partial in the page (operator, 2026-10-17).
- The `stream` island from v2-pages over building it here: the result page and the inspectors use
  one island built once (operator, 2026-10-17, X3).
- The OG cards switch to `spectrum::layout` here over in v2-entries: the page's layout run, its
  predecessor mapping and the card share one feature (operator, 2026-10-17, X4).
- The user-visible copy drafted as defaults (requirement 21's throttle sentence, requirement 12's
  "unchanged since", requirement 11's class labels, requirement 24's scope sentence and examples,
  requirement 18's "changes" column) accepted as drafted and revised from this feature's own
  Phase 8 screenshots, over interviewing it now: it keeps the batch approval (operator,
  2026-10-17).
- The run POST reaches lens through v2-api's `DomainRuns`, over a second trait: one seam from
  netray-web to lens's admission and validation (P26), and netray-web cannot depend on lens
  (implementer, 2026-10-17).
- `DomainRuns` is defined once by `v2-api` in `netray_web::api` with `validate` and `start(domain,
  client, trigger, prefetch)`, and lens implements `start` over `v2-admission`'s `check_limits`
  then `admit`, over `v2-result-page` extending the trait: one owner for the one seam from
  netray-web to lens, built in lane A before lane C uses it (implementer under the operator's autonomy mandate, 2026-10-17).
- A `RunStream` `Section` carries the engine's `netray_engine::SectionEvent`, whose outcome holds
  the section's checks with findings and evidence references and its blocks, over an
  admission-specific section event: a section fragment renders from that event alone, and the
  engine already sends it (implementer under the operator's autonomy mandate, 2026-10-17).
- Before `Done`, a section's live evidence element renders v2-inspectors' `evidence::section` over a
  partial `Run` assembled in `views.rs` from the `Section` events received so far, over a second
  live-section renderer: one evidence markup for live and stored runs, and each engine
  `SectionEvent` carries the checks (with findings and references) and blocks that `Run` needs (implementer under the operator's autonomy mandate, 2026-10-17).
- No V1, badge or OG response waits on the history store, and only `v2-api`'s and
  `v2-result-page`'s run answers read on to `Recorded`, over v1 answering at `Done` without
  `recorded` and `entry_id`: SDD §7.1's run answer needs the record outcome, and the V1 routes
  stop at `Done` (implementer under the operator's autonomy mandate, 2026-10-17).
- `is_prefetch` lives in `netray-common` (`netray_common::prefetch::is_prefetch`), over lens's
  `admission.rs`: netray-web's run and inspector routes decide the prefetch flag with the same
  helper, and netray-web cannot depend on lens (implementer under the operator's autonomy mandate, 2026-10-17).
- One acceptance seed, `cargo run -p netray-store --example seed` over JSON run fixtures in
  `tests/acceptance/fixtures/history/`, created by `v2-inspectors` (first in build order to need
  it) and extended by `v2-result-page`, over a second, `sqlite3`-loaded `history-seed.sql`:
  `HistoryStore::record` runs the migrations and computes fingerprints and diffs, so fixtures
  cannot drift from the schema, and no `sqlite3` CLI joins the toolchain (implementer under the operator's autonomy mandate, 2026-10-17).
- One build order in every spec (lane A `v2-history` → `v2-admission` → `v2-entries` → `v2-api`;
  lane B `v2-spectrum` ∥ `v2-pages`, parallel to lane A; lane C `v2-inspectors`, its Phase 2 before
  `v2-result-page` Phase 1, then `v2-result-page`; then `v2-cutover`) over each spec's own reading
  of the program: every **Depends on** and Context names the same order (implementer under the operator's autonomy mandate, 2026-10-17).
- A fresh visit POST starts nothing and answers the stored entry, over calling `admit`: the cache
  window is the caller's step in v2-admission's interface (implementer, 2026-10-17).
- The idle input uses v2-pages' `?d=` redirect, over a second query parameter: one path for the
  idle input, the 404 page's input and lens's old bookmarks (implementer, 2026-10-17).
- Streaming is verified by Rust tests with stub modules and by vitest, over a live Playwright run:
  local runs need the network, and a skip marker is refused (AGENTS.md).
- Screenshots come from a seeded store with a client budget of 0, over live runs: every state is
  offline and deterministic, and the refusal state is reachable.
- A no-JS form POST waits for the run and answers 303, over answering SSE to a browser that cannot
  read it: §5.2 promises a check button without JavaScript.

- `test_header_parity.sh` changed under `ADLC-Test-Change` for lens's `/` only, expecting
  v2-pages' strict CSP, over keeping `/` on lens's SPA until `v2-cutover`: V4.13 and the idle page
  need `/` here, and the scope stays the one route this feature takes over (implementer under the
  operator's autonomy mandate, 2026-10-17).
- `finish` waits for the operator's review of this feature's own Phase 8 screenshots, over the
  2026-10-18 midday review, which covers lane B and precedes them: copy approval is the
  operator's, so this halt is the one that waits under the mandate (implementer under the
  operator's autonomy mandate, 2026-10-17).
- Fixture times as `age_seconds` before the seed run's clock, over absolute fixture times: a fixed
  time makes "fresh" impossible on a later day; the contract is v2-inspectors' seed's
  (implementer under the operator's autonomy mandate, 2026-10-17).
- `LENS_CACHE__TTL_SECONDS=3600` in the result-page acceptance run, over the 300 s default: the
  fresh states must survive a suite longer than five minutes, and 7200 s stays stale
  (implementer under the operator's autonomy mandate, 2026-10-17).
- Section fragments re-send the whole `<figure id="spectrum">`, over bare `band-<protocol>` `<g>`
  fragments or one inline `<svg>` per band: the stream island parses fragments as HTML in a
  `<template>`, where a bare `<g>` breaks, and the figure keeps v2-spectrum's one-plate markup
  (implementer under the operator's autonomy mandate, 2026-10-17).
- The end fragments re-send the open issues with entry links, over issues linking only once an
  entry id is known: links during the stream stay usable (in-page live evidence), and after the
  end they equal the GET page's (implementer under the operator's autonomy mandate, 2026-10-17).
- Section fragments provisional and every end fragment re-rendered from the `Done` run, over
  building the live view through lens's adaptation: lens's adaptation stays in lens, which
  netray-web cannot depend on, and the final page is the stored run's (implementer under the
  operator's autonomy mandate, 2026-10-17).
- `data-shown` and per-section `SectionState`, over `spectrum__run--shown` and inferring
  `data-unmeasured` from missing measured lines: v2-spectrum requirements 10 and 18 are the
  contract (implementer under the operator's autonomy mandate, 2026-10-17).

- Taken over from v2-spectrum's phase 3 review (deferred there, 2026-10-17): the IP group name spills past the plate's right edge and makes the spectrum box scroll at container widths 432–920 px; this feature fixes it with the page layout and a 390/1440 px screenshot shows it gone (implementer under the operator's autonomy mandate, 2026-10-17).
- A warn or fail whose check ID the current catalogue no longer holds (an entry recorded before a release removed the check) is left out of the open issues and listed last in the passed-checks details under its ID, over a fallback owner group: requirement 7's title, finding, fix hint and guide all come from the catalogue, and requirement 9's groups from its owner and severity (implementer under the operator's autonomy mandate, 2026-10-18; Phase 1 review).
- A changed evidence block with no items by design (a text shape, a shape change) reads its block ID alone, and one item reads "1 item", over "(0 items)": requirement 11's count is the items `BlockChangeKind::Changed` carries (implementer under the operator's autonomy mandate, 2026-10-18; Phase 1 review).
- An entry scored with a profile version this build does not embed (a custom `[scoring] profile_path`) is mapped for the layout with `Profiles::current()`, over leaving it without a spectrum or skipping it: the page still renders the latest entry (requirements 3, 4), and the card of requirement 29 draws the same mapping (implementer under the operator's autonomy mandate, 2026-10-18; Phase 2 review).
- An issue's `since` counts a shown entry that lacks the check as a different state, so the current span starts where the check first appears, over skipping such entries: a check a release added gets the date it first failed (implementer under the operator's autonomy mandate, 2026-10-18; Phase 2 review).
- A method a result route does not serve on a non-domain first segment reaches lens's fallback, as a GET of it does, over axum's 405: requirement 1 keeps every non-domain path on its current handler (implementer under the operator's autonomy mandate, 2026-10-18; Phase 2 review).
- The end fragments replace the header's parts (`result-grade`, `run-line`, `progress`, `result-share`), never the whole `#result-header`, over re-sending the header: the stream island's form lives in the header, and replacing it detaches the form, so `data-stream-state` and `netray:stream-end` would land on a detached node and requirement 23's `recent` write could not happen (implementer under the operator's autonomy mandate, 2026-10-19; Phase 3 review).
- A complete run whose record failed (`Recorded(None)`) ends with its own grade, the run line "not recorded" and the diff note "not recorded", over showing it as incomplete: only `run.complete == false` makes a run incomplete (implementer under the operator's autonomy mandate, 2026-10-19; Phase 3 review).
- When the history re-read after a stored record fails, the end fragments render the `Done` run as the recorded entry (its id from `Recorded`, created now), over the not-stored view: the entry exists and its links resolve (implementer under the operator's autonomy mandate, 2026-10-19; Phase 3 review).
- The `countdown` island counts to a deadline (`Date.now()` at mount plus `data-retry-after` seconds) and redraws every second, over counting ticks: a hidden tab or the back-forward cache stops timers, and the button must open when the server's `Retry-After` ends (implementer under the operator's autonomy mandate, 2026-10-19; Phase 4 review).
- The `countdown` island enables and disables the re-check button `#recheck button[name="trigger"]` outside its own element, over a second mechanism through the stream island: requirement 23 asks the countdown to enable re-check, and `frontend-rules.md` §3's "markup inside its element" yields to it for this one named button (implementer under the operator's autonomy mandate, 2026-10-19; Phase 4 review).
- `finish` no longer waits for the copy and layout approval from the Phase 8 screenshots: the operator reviews copy and layout live on a local instance after the merge, and copy changes come as a follow-up feature (operator, 2026-10-19, relayed by the coordinator; supersedes the halt before `finish` recorded above).
- The `spectrum` island moves `aria-current` to the run on display (hovered, pinned or stepped), over leaving it on run 0: the trace marker (R43) follows the shown run, and a pinned row carries only the marker, no further emphasis (frontend-rules §4) (implementer under the operator's autonomy mandate, 2026-10-19; Phase 5 review).
- On the end page of a run that was not stored, the line sentences include that run as the newest point (dated now), over the stored history alone: the sentence must not contradict the line shown; live figures during a stream keep the stored history's sentences, which the end fragments replace (implementer under the operator's autonomy mandate, 2026-10-19; Phase 5 review).
- The run stepper renders only when the trace holds two or more runs, over two inert buttons on a page without history (implementer under the operator's autonomy mandate, 2026-10-19; Phase 5 review).

## Open decisions

None.

## Out of scope

- Evidence views, explore-live links, inspectors: `v2-inspectors` (V4.7, SDD §6).
- `base.html`, CSP, islands scaffold and the `stream` island, static pages, the codename check:
  `v2-pages` (V4.8–V4.12).
- `spectrum::layout` and its property tests: `v2-spectrum` (V4.4).
- Admission, join, throttle, history as cache: `v2-admission` (V2.6). `/r/<id>` and its 410:
  `v2-entries` (V2.7). API v1 and the `RunCheck` fields: `v2-api` (Phase 3).
- Host routing, SPA deletion, redirects, `netray_` metrics, the agreement test: `v2-cutover`
  (V5.1–V5.5).
