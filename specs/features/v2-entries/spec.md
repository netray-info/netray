# Spec: v2-entries

Status: Ready for Implementation
Created: 2026-10-17

## Goal

With `[history] enabled`, `GET /r/<id>` for the ID of a stored history entry answers `200` with a server-rendered page on the Phase 4a components (grade, run line, the entry's warn and fail checks, its stored diff), and `GET /r/<id>/og.png` answers that entry's own OG card, computed from the stored entry only, in the V1 card layout. A well-formed ID that is neither an entry nor a live V1 snapshot answers `410` with a page that offers the idle input and names no domain. A live V1 snapshot ID still renders its V1 page, and with `[history] enabled = false` every `/r/` answer is what it is today. `GET /privacy` answers §4.7's text, and the private `privacy-policy.md` replaces its "aggregates only" line for the history. No `/r/` request starts a run. Routes, templates and the entry → page mapping live in `netray-web` (SDD §3.1, operator X1); lens mounts `netray-web`'s routers ahead of its SPA fallback until v2-cutover. This is SDD Phase 2 criteria V2.7 and V2.8 (§4.6, §4.7; R15, R16). The full result page (V4.5, V4.13), the evidence views (V4.7) and the OG card drawn from the spectrum layout (V4.4, v2-result-page) replace this rendering in Phase 4.

## Non-goals

- Admission, history-as-cache, run join, throttle (V2.6, feature `v2-admission`).
- API v1, and any wire field that hands an entry ID to a client (`v2-api`). The lens V1 wire, its goldens and `snapshot_id` on the done event stay unchanged.
- The new result page, the spectrum, the trace, R19's first-state and incomplete lines, and the evidence views `/r/<id>/<protocol>` (Phase 4: V4.4–V4.7, V4.13).
- Removing `snapshots.db` or making V1 snapshot IDs answer `410` (V5.6, at the cutover).
- Islands, speculation rules, navigation, the logo, `/static/` serving and the CSP of the page frame (`v2-pages`, §8.1, §8.4).
- A privacy policy beyond §4.7's history statements: a separate feature after this program (operator, 2026-10-17).
- A release; the program builds to a local `0.30.0`, the only release (operator, 2026-10-17).

## Context and constraints

- Builds on the unmerged `v2-history` (worktree `.worktrees/netray/v2-history`). `netray-store` reads one entry with `HistoryStore::get(id) -> Result<Option<Entry>, StoreError>` (`crates/store/src/lib.rs:213-221`); `Entry` carries `id, domain` (A-label), `created_at` (Unix seconds), `last_seen_at, predecessor_id, profile_version, grade, fingerprint, run, diff, engine_version` (`:103-116`). The sweep deletes rows (`:234-270`), so nothing maps a swept ID to a domain (SDD §4.6).
- Entry IDs and V1 snapshot IDs have the same shape: 8 characters over `[0-9A-Za-z]`, nanoid with collision retry, in two separate tables (`crates/store/src/lib.rs:16-24`, `:321-347`; `crates/lens/src/snapshot/store.rs:13-19`, `:55-86`). `snapshot::validate_shortid` is the regex `^[0-9A-Za-z]{8}$` (`crates/lens/src/snapshot/shortid.rs:7-16`). Neither store checks the other's IDs, so an ID's shape cannot tell the two apart.
- `/r/` today: `snapshot_router` routes `/r/{shortid}`, `/r/`, `/r/{shortid}/` and `/r/{shortid}/{*rest}` (`crates/lens/src/routes.rs:1567-1574`); a malformed ID, an unknown or expired ID, a missing store and every unmatched `/r/` path answer `404` with the "Snapshot not found … expired or the ID is unknown" page (`:1581-1621`, `not_found_html` `:1623-1629`); a store error answers `500` JSON (`:1601-1608`); a live snapshot answers `200` with `public, max-age=86400, immutable` (`:1609-1619`), its HTML from `snapshot::render_snapshot_html(&Snapshot, &SiteConfig)` (`crates/lens/src/snapshot/render.rs:29`). `lib.rs` merges the router only when `[snapshots] enabled` and ends in `.fallback(spa::handler)` (`crates/lens/src/lib.rs:146-152`). `crates/lens/tests/snapshot_routes.rs:203-273` holds that contract on an `AppState` without a history store.
- The V1 snapshot page points its `og:image` at the domain card `/og/<domain>.png` (`crates/lens/src/snapshot/render.rs:67`), which may start a run (`crates/lens/src/og/handler.rs:138-166`). The V1 card renderer `svg_for_grade(domain, grade, label, score_pct, checked_at)` and `svg_to_png` live in lens (`crates/lens/src/og/render.rs:98-145`) with the font database (`og/fonts.rs:8`); `format_utc_timestamp(SystemTime)` formats "May 8, 2026 · 17:44 UTC" (`:49`). The stored entry holds no score percentage (§4.1), and §3.2 step 6 says `/r/<id>` and the OG card compute no verdict.
- The entry ID never reaches a client in this feature: the done event carries only the V1 `snapshot_id` (`crates/lens/src/routes.rs:226-231`, `:797-801`). `/r/<id>` for entries is reached by the tests, by the operator, and later by API v1 and the result page. `v2-api` requirement 13 agrees with this feature's entry `og.png` (requirement 15), not with `/r/<id>`: both answer `410` for every well-formed ID that is no entry, a live V1 snapshot's ID included, while `/r/<id>` answers a live V1 snapshot's ID with its V1 page `200` (requirement 10).
- `AppState` holds `history_store: Option<Arc<HistoryStore>>` and `snapshot_store` (`crates/lens/src/state.rs:57-59`), set at startup (`crates/lens/src/lib.rs:68-108`). lens depends on `netray-model`, `netray-engine`, `netray-store`, not yet on `netray-web`.
- `netray-web` today: a library depending on `netray-common`, askama and serde_json (`crates/web/Cargo.toml`), with no route (`crates/web/src/lib.rs:1-7`). It holds the ten R44 macros and their view structs (`crates/web/src/components.rs`), the tokens CSS (`crates/web/src/tokens.rs:5`), `static/base.css` and one stylesheet per component, and the tagline partial; there is no `base.html` and no `templates/pages/` yet. `tests/repo/test_web_crate.sh:15-17` refuses a `Router` in `crates/web/src`, `:44` refuses axum as a dependency, `:51` refuses `netray` depending on `netray-web` directly. SDD §3.1 gives `netray-web` the axum router, templates, OG card and badge, depending on `engine`, `store`, `model`, `common`. Operator X1: the first feature that needs a router lifts the first two guards under `ADLC-Test-Change`; this feature does not need the third, which v2-pages (requirement 8) or v2-api (requirement 1) lifts.
- The gate scans `templates/**/*.html`, `static/**/*.css` and `islands/**/*.ts` (`crates/web/src/gate.rs:181-190`): rule 2 refuses a component's class outside its own macro file, rule 3 any inline `<script>` but speculation rules; askama expressions are opaque to it.
- lens's CSP is `default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; …` (`crates/common/src/security_headers.rs:67`), applied by lens's layers to every route of its app, merged routers included. lens serves no `/static/` path; its SPA assets sit under `/assets/` and root files (`crates/lens/src/spa.rs:77`).
- **Build order** (one order in every spec): lane A `v2-history` → `v2-admission` → `v2-entries` → `v2-api`; lane B `v2-spectrum` ∥ `v2-pages`, parallel to lane A; lane C, after the operator's UI review at midday 2026-10-18, `v2-inspectors` (its Phase 2 before `v2-result-page` Phase 1), then `v2-result-page`; then `v2-cutover`. This feature starts when `v2-admission` has merged.
- **Parallel lane.** `v2-pages` (lane B) may land before or after this feature and also creates `templates/base.html` (its requirements 9 and 32: `styles`, `header`, `main` and `scripts` blocks), a router in `crates/web/src/pages/` (`pages::router()`, merged by lens ahead of its SPA fallback), the `/static/` serving, and a `test_web_crate.sh` change. It serves no `/privacy` route: `/privacy` is this feature's. This feature creates only what its pages need; whichever lands second merges onto the other's (requirement 20).
- In production the apex host goes to lens except the paths the site router lists (`/guide`, `/compare`, `/status`, `/tools`, a fixed set of files; netray-ops `ansible/roles/app_stack/templates/netray-web-compose.yml.j2:44`, private). `/r/…` and `/privacy` therefore reach lens without an infrastructure change; today `/privacy` falls through to the SPA (`crates/lens/src/lib.rs:152`).
- The SPA starts a check from `?d=<domain>` on load (`crates/lens/frontend/src/App.tsx:62-64`), so a form `GET / ?d=…` is the idle input; the form itself starts no run (P06).
- Catalogue texts per check (`title`, `finding`, `fix_hint`, `guide` path) come from `netray_engine::catalogue()` (`crates/engine/src/catalogue.rs:16-30`, `:158`); a check ID the catalogue does not know carries no texts (`crates/lens/src/routes.rs:2030`). Stored runs hold check IDs and statuses only, no findings (`crates/model/src/run.rs:41-46`). The stored diff is `netray_model::diff::Diff` (`crates/model/src/diff.rs:13-76`).
- The privacy policy stub lives in netray-ops `specs/netray/sdd/privacy-policy.md` (private); line 25 reads "What's stored long-term (nothing per request; aggregates only)." §4.7 replaces that line for the history. The monorepo gate cannot see that repository: two commits in two repositories (`~/Documents/src/CLAUDE.md`).
- axum 0.8: `/x/{*rest}` conflicts with `/x/{id}` (CLAUDE.md), and merging two routers that route the same path panics; whether `/r/{id}/og.png` may sit beside `/r/{id}/{*rest}` is the implementer's to settle, and the tests go through the router.
- Gate: `just adlc-verify`; full suite `just check`. Amend `specs/sdd/v2.md` in the same commit as the code that changes what it says. `v2-admission` lands before this feature (lane A); it changes neither `HistoryStore::get` nor `/r/`.

## Requirements

Imperative, numbered, each testable on its own. `(default, not interviewed)` marks a value the
operator never decided; the implementer halts when such a default would be visible to a user.
`(mandate default)` marks a user-visible value chosen under the operator's autonomy mandate; the
implementer builds it as written and does not halt on it, and the operator reviews it from
screenshots at the midday UI review on 2026-10-18 (Decision log).

1. `netray-web` gains `templates/base.html` holding only what this feature's pages need: `<!DOCTYPE html>`, `<html lang="en">`, charset and viewport metas, a `<title>`, optional `description`, `og:title`, `og:image` and `og:url` metas, the `styles` block in the head, and in the body, in this order, the `header` block, the `main` block, a footer with the tagline partial and a link to `/privacy`, and the `scripts` block last (the frame order of v2-pages requirements 9 and 32). This feature's entry, gone and privacy pages override `styles` with one `<style>` element holding the tokens CSS, `static/base.css` and the ten component stylesheets in that order (inline; default, not interviewed), and override `header` and `scripts` with nothing, so they hold no `<script>`, islands bundle, speculation rules, navigation or `/static/` link whatever the frame's defaults are. This feature adds none of those to the frame; its own defaults for `header` and `scripts` are empty.
2. `netray-web` gains a module `entries` (`crates/web/src/entries/`) with `render::entry(&EntryPageView)`, `render::gone()` and `render::privacy()`, each returning `askama::Result<String>` from `templates/pages/entry.html`, `gone.html` and `privacy.html`, each on `base.html`. Component markup comes only from the R44 macros; no new stylesheet is added; the gate passes over the new templates.
3. The entry page renders exactly, in this order: an `<h1>` with the domain; the `grade` component (the letter, no score; tone pass for A+, A, B, warn for C, D, fail for F; mandate default); a `run_line` with the parts "recorded <created_at>", "profile v<profile_version>", "engine <engine_version>"; the `diff_block` when the view has one; one `check_row` per open issue in the view's order; one paragraph of status counts ("N pass · N warn · N fail · N not applicable · N not tested · N not measured", zero counts included). Its `<title>` and `og:title` are "<domain> · <grade>", its `og:image` is `/r/<id>/og.png` and its `og:url` `/r/<id>`. Every value is escaped.
4. The gone page renders a heading "This entry has expired", the sentence "This entry has expired and is no longer stored.", and the `query_input` component with action `/`, name `d`, label "Check a domain", placeholder `example.com`, submit "Check" (texts: mandate default). It answers a swept entry, an expired V1 snapshot and a never-issued ID alike (requirement 10), so it states no retention period. It names no domain and no ID, and its `og:*` metas are absent.
5. The privacy page carries only SDD §4.7's statements, in plain prose: the history stores the domain, the check results and their evidence, and nothing about the visitor; an entry is kept 90 days after it was last seen, the latest entry keeps its predecessor, and a domain nobody checks for 90 days is deleted with all its entries; rate limiters and the throttle are held in memory only, keyed by address or domain, and nothing of them is written to disk. It makes no claim about request logs, GeoLite2, browser storage or cookies.
6. `netray-web` builds an `EntryPageView` from a stored `netray_store::Entry` in `entries::view`: `created_at` formatted as "May 8, 2026 · 17:44 UTC" by `format_utc_timestamp`, which moves from `crates/lens/src/og/render.rs` to `netray-web` unchanged and which lens re-exports at its old path; open issues are the run's checks with status fail, then warn, each group in catalogue order, with code = check ID, title = catalogue `title`, body = catalogue `finding`, fix = `fix_hint`, guide link = the catalogue `guide` path, no evidence link and no "since"; a check ID the catalogue does not know shows its ID as title, an empty body and no fix or guide; the counts come from the run's check statuses. No scoring function is called.
7. The diff block, present exactly when `Entry.diff` is `Some` (also when `predecessor_id` is NULL because the predecessor was swept): title "Changes since the previous entry"; summary "<from> → <to>", or "<from> → <rescored> (profile) → <to>" when `rescored` is set, or "Grade unchanged (<to>)" when they are equal and nothing is rescored; one item per check change in stored order, kind = the class in words ("fixed", "improved", "new", "worse", "now applicable", "no longer applicable", "into not tested", "out of not tested", "into not measured", "out of not measured", "added", "removed", "added by release", "removed by release"), text = catalogue title or check ID, protocol = the ID's protocol, tone fix for fixed and otherwise by the `to` status (pass, warn, fail; untested for every other status or none); then one item per evidence change, kind "evidence", text "<block id> added", "<block id> removed" or "<block id>: <n> changed", tone untested; the note "The previous entry was recorded by another engine version." when `engine_changed`.
8. `netray-web` gains axum (workspace), `netray-store`, `netray-model` and `netray-engine` (path). `entries::router(EntriesDeps) -> axum::Router` serves `/r/{id}`, `/r/{id}/og.png`, `/r/`, `/r/{id}/` and `/r/{id}/{*rest}`; `EntriesDeps` holds the `Arc<HistoryStore>`, an optional V1 snapshot lookup and the entry card renderer (requirement 15), both supplied by lens as functions, so `netray-web` names no lens type. `entries::privacy_router() -> axum::Router` serves `GET /privacy`. The lens not-found HTML moves to `netray-web` as `entries::NOT_FOUND_HTML`, byte-identical, and lens's `snapshot_router` answers with it.
9. `tests/repo/test_web_crate.sh`, under `ADLC-Test-Change`, wherever its guards are still in place: allows a `Router` only under `crates/web/src/entries/`, `crates/web/src/pages/` and `crates/web/src/api/` (the allow-list v2-pages and v2-api also write) and refuses one anywhere else in `crates/web/src`; allows axum. This feature leaves the guard that `netray` does not depend on `netray-web` as it finds it (v2-pages or v2-api lifts it). lens depends on `netray-web`.
10. `GET /r/{id}` on `entries::router` answers by precedence: a malformed ID (not `^[0-9A-Za-z]{8}$`, or rejected by the extractor) → `404` with `NOT_FOUND_HTML`; an entry with that ID → `200` entry page; else, when a V1 lookup is configured, a live V1 snapshot with that ID → its V1 page, status and headers as today, and a failed V1 lookup → `500` as today; else → `410` gone page. An entry wins over a V1 snapshot with the same ID.
11. The entry page answers `content-type: text/html; charset=utf-8` and `cache-control: public, max-age=3600`; the gone page answers `410`, `content-type: text/html; charset=utf-8` and `cache-control: public, max-age=3600` (both default, not interviewed).
12. A failed history lookup answers `500`, logs at `warn` with the ID, and never answers `410`, the V1 page or the entry page: a failed lookup is never an answer.
13. lens mounts `entries::router` when `[history] enabled` (with the V1 lookup when `[snapshots] enabled`), else its own `snapshot_router` when `[snapshots] enabled`, else no `/r/` route; it mounts `entries::privacy_router` whatever the store configuration; both before `.fallback(spa::handler)` and inside its middleware stack. Without a history store, every `/r/` path answers exactly as today: status, headers and body. With `[snapshots] enabled` that includes `404` for unknown, expired and malformed IDs and for `/r/{id}/og.png`; with both disabled every `/r/` path reaches `spa::handler`, as today.
14. On `entries::router`, `/r/`, `/r/{id}/` and every `/r/{id}/{*rest}` other than `og.png` answer `404` with `NOT_FOUND_HTML`. This catch-all stays whole here; `v2-inspectors` (lane C, after this feature) narrows it to `/r/{id}/{segment}/{*rest}` for its evidence views under `ADLC-Test-Change` (its requirement 6). No `/r/` or `/privacy` request starts a run, reads or writes the moka cache, consumes a rate limiter, or writes to either store (`last_seen_at` included).
15. `GET /r/{id}/og.png` for an entry answers `200` `image/png` 1200×630 in the V1 card's layout (operator X4): lens's renderer, passed in `EntriesDeps`, draws the entry's grade letter, its domain, `created_at` as the timestamp, the default label, and no score line (§3.2 step 6; the entry holds no percentage); `etag` over ID and grade, `304` on a matching `If-None-Match`, `cache-control: public, max-age=3600` (default, not interviewed). A well-formed ID that is no entry, a live V1 snapshot's ID included, answers `410` with an empty body; a malformed ID `404`. A render failure answers as `/og/<domain>.png`'s does.
16. `GET /privacy` answers `200` with the privacy page, `content-type: text/html; charset=utf-8`, `cache-control: public, max-age=3600` (default, not interviewed), whatever the store configuration. `/` and every other path the SPA serves today still reach the SPA, except `/r/*` when `[history] enabled` (requirement 13).
17. `specs/sdd/v2.md` §4.6 says, in the commit that builds requirement 10: until V5.6, `/r/<id>` looks up the history first, then the V1 snapshots (a lookup lens hands to `netray-web`), and answers `410` for a well-formed ID in neither; a malformed ID answers `404`; until V4.5 the entry renders the minimal page of this feature, served by `netray-web`'s router, which lens mounts until V5.1.
18. `specs/sdd/v2.md` §4.6 says, in the commit that builds requirement 15: the entry's OG card shows grade, domain and recording time in the V1 layout and no score, until v2-result-page switches it to `spectrum::layout` (V4.4).
19. `specs/sdd/v2.md` §4.7 says the page is served by `netray-web` at `/privacy` (lens mounts it until V5.1), and the Phase 2 table marks V2.7 and V2.8 as built by `v2-entries`.
20. Lane merge with `v2-pages`, applied by whichever of the two lands second, in its rebase (the rule of v2-pages requirement 32, stated once for both): one `templates/base.html` with the `styles`, `header`, `main` and `scripts` blocks, the union of both features' optional head elements, and v2-pages' defaults for the three blocks (the `/static/app.css` link, the header with navigation, the bundle and speculation rules); this feature's entry, gone and privacy pages keep overriding `styles` with their inline `<style>` and `header` and `scripts` with nothing (requirement 1), so they render complete whether or not lens mounts the pages router; one `templates/pages/privacy.html` and exactly one `/privacy` route in `netray-web`, in `entries::privacy_router`; the `test_web_crate.sh` allow-list names `entries/`, `pages/` and `api/`.
21. A separate commit in netray-ops, made by the overnight run and pushed by the operator, changes `specs/netray/sdd/privacy-policy.md`: the line "What's stored long-term (nothing per request; aggregates only)." gains the history exception with §4.7's statements and a pointer to `/privacy`. The monorepo gate does not check it; finish checks it by `git -C <netray-ops> log` and a grep for "90 days" in that file.

## Phases

One branch per feature; its phases are steps on it. Each phase is one failing-tests commit and one
production commit; review and finish run once, after the last phase.

## Phase 1 — Frame and page templates in netray-web

**Depends on:** `v2-history` and `v2-admission` merged (lane A); `v2-pages` may have landed (requirement 20)
**Requirements:** 1, 2, 3, 4, 5, 6, 7

`crates/web`: `templates/base.html` (or this feature's blocks merged into `v2-pages`' frame, requirement 20), `templates/pages/{entry,gone,privacy}.html`, `src/entries/` with `EntryPageView` (domain, id, grade view, run line view, optional diff block view, open issues as `CheckRowView`s, counts), the Entry → view mapping, `format_utc_timestamp` (moved, re-exported by lens) and the three render functions; `netray-store`, `netray-model`, `netray-engine` as dependencies. Tests render each page from views built through `HistoryStore::record` on a temporary database; the existing gate test runs over the new templates.

### Test Scenarios

GIVEN an `EntryPageView` for `a.example.com` with grade B, two open issues and a diff block WHEN `entries::render::entry` renders it THEN the markup holds, in order, the `<h1>`, the grade root with tone pass and no score, the run line, the diff block, two check rows, the counts paragraph, and `og:image` `/r/<id>/og.png`.
GIVEN the same view without a diff block WHEN rendered THEN no `diff-block` class appears.
GIVEN a domain or a check title containing `<script>` WHEN rendered THEN it appears escaped and the page holds no `<script` element.
GIVEN any of the three pages WHEN rendered THEN it starts with `<!DOCTYPE html>`, holds one `<style>` element containing a tokens custom property and the `.check-row` rule, no `<script` and no `/static/` link, and links `/privacy` in its footer.
GIVEN `entries::render::gone()` WHEN rendered THEN it holds the heading, the sentence of requirement 4, the `query_input` form with action `/` and name `d`, no `og:` meta, no "90", and no domain other than the placeholder `example.com`.
GIVEN `entries::render::privacy()` WHEN rendered THEN each statement of requirement 5 is present; and the words "cookie", "GeoLite2" and "log" do not appear (neighbour of "only §4.7").
GIVEN an entry with a stored diff in which `tls.not_expired` went fail → pass and the grade C → B WHEN its view renders THEN the diff block holds a "fixed" item with tone fix and the summary "C → B".
GIVEN an entry whose diff carries `rescored` WHEN rendered THEN the summary has the "(profile)" step; GIVEN `engine_changed` THEN the engine note is present; GIVEN neither THEN neither appears.
GIVEN a first entry (no diff) WHEN rendered THEN there is no diff block; GIVEN an entry whose predecessor was swept (`predecessor_id` NULL, diff kept) THEN the diff block is present.
GIVEN an entry with fail checks and warn checks WHEN rendered THEN every fail row precedes every warn row, each group in catalogue order, and no pass check has a row.
GIVEN an entry holding a check ID the catalogue does not know, status warn WHEN rendered THEN its row shows the ID as title and no fix or guide link.
GIVEN a fixed instant WHEN `netray_web`'s `format_utc_timestamp` and lens's `og::render::format_utc_timestamp` format it THEN both answer the same string.
GIVEN the new templates WHEN the gate test runs THEN it reports no violation; GIVEN a page template that writes `class="check-row"` itself THEN the gate refuses it (rule 2).

## Phase 2 — The routers in netray-web, `/r/<id>` mounted in lens

**Depends on:** Phase 1
**Requirements:** 8, 9, 10, 11, 12, 13, 14, 17, 20

`crates/web`: axum, `src/entries/` router with `EntriesDeps` and `NOT_FOUND_HTML`. `tests/repo/test_web_crate.sh` (with `ADLC-Test-Change`). `crates/lens`: `netray-web` as a dependency, the V1 lookup function handed to `EntriesDeps`, the mount condition in `lib.rs`, `snapshot_router` answering `NOT_FOUND_HTML`; SDD §4.6. Router tests drive `entries::router` in `netray-web`; mount tests drive lens's app.

### Test Scenarios

GIVEN a history store with an entry of `a.example.com` WHEN `GET /r/<its id>` THEN `200`, the entry page with that domain, `cache-control: public, max-age=3600`.
GIVEN an entry swept by `HistoryStore::sweep` WHEN `GET /r/<its id>` THEN `410`, the gone page, and the body does not contain `a.example.com`.
GIVEN a well-formed ID never issued WHEN `GET /r/AAAAAAAA` with a history store THEN `410`.
GIVEN a history store and a V1 lookup that returns a live V1 snapshot WHEN `GET /r/<snapshot id>` through lens's app THEN `200` with the V1 snapshot page and `public, max-age=86400, immutable`, as today.
GIVEN a V1 lookup that fails WHEN `GET /r/<well-formed id that is no entry>` THEN `500`, not `410`.
GIVEN an entry and a V1 snapshot whose ID the test rewrites in `snapshots.db` to the entry's ID WHEN `GET /r/<that id>` THEN the entry page.
GIVEN a history store WHEN `GET /r/x`, `/r/AAAA-AAA`, `/r/%C0`, `/r/`, `/r/AAAAAAAA/`, `/r/a/b` THEN each answers `404` with a body byte-equal to `NOT_FOUND_HTML`.
GIVEN a history store whose `entries` table the test drops through a second pool on the store's database file (`DROP TABLE entries`, as `crates/lens/tests/history_record.rs:374` does) WHEN `GET /r/AAAAAAAA` THEN `500`, not `410`, and a `warn` log naming the ID.
GIVEN no history store WHEN the existing `snapshot_routes.rs` tests run THEN they pass unchanged (unknown and expired IDs `404`, same body).
GIVEN `[history] enabled = true` and `[snapshots] enabled = false` WHEN lens's app is built THEN `/r/<entry id>` answers `200`; GIVEN both disabled THEN `GET /r/AAAAAAAA` answers what `spa::handler` answers for it.
GIVEN a `badge_check_fn` stub that counts calls WHEN `GET /r/<entry id>`, `/r/<swept id>` and `/r/<snapshot id>` THEN the stub was never called and the entry's `last_seen_at` is unchanged.
GIVEN `cargo metadata` WHEN `test_web_crate.sh` runs THEN it passes with axum and a `Router` under `crates/web/src/entries/`; GIVEN a planted `Router` in `crates/web/src/components.rs` THEN it fails; the `netray` → `netray-web` check is as this feature found it.

## Phase 3 — The entry's own OG card

**Depends on:** Phase 2
**Requirements:** 15, 18

`crates/web`: the `/r/{id}/og.png` handler on the entry, calling the renderer from `EntriesDeps`. `crates/lens`: that renderer, the V1 card without its score line (`svg_for_grade` with no score, `svg_to_png`); SDD §4.6.

### Test Scenarios

GIVEN an entry of `a.example.com` with grade A WHEN `GET /r/<id>/og.png` through lens's app THEN `200`, `image/png`, a 1200×630 PNG, an `etag`, `cache-control: public, max-age=3600`.
GIVEN the entry's SVG before rasterising WHEN inspected THEN it holds the letter A, the domain and the formatted `created_at`, and no `%` score text.
GIVEN that `etag` WHEN the card is requested with `If-None-Match` equal to it THEN `304`.
GIVEN a swept entry's ID, a never-issued well-formed ID, or a live V1 snapshot's ID WHEN `GET /r/<id>/og.png` THEN `410` with an empty body.
GIVEN `GET /r/x/og.png` THEN `404`; GIVEN `GET /r/<entry id>/other` THEN `404` with `NOT_FOUND_HTML` (neighbour of "only `og.png`").
GIVEN a renderer stub that fails WHEN the entry card is requested THEN the answer equals `/og/<domain>.png`'s on a render failure.
GIVEN no history store WHEN `GET /r/<snapshot id>/og.png` THEN `404` with today's not-found page.
GIVEN a counting `badge_check_fn` stub WHEN the entry card is requested THEN the stub was never called.

## Phase 4 — `/privacy`, the SDD and the private policy

**Depends on:** Phase 3 (the Phase 2 table may only mark V2.7 built once it is)
**Requirements:** 16, 19, 21

`crates/web`: `entries::privacy_router`. `crates/lens`: its mount before the SPA fallback. SDD §4.7 and the Phase 2 table. netray-ops: the `privacy-policy.md` commit.

### Test Scenarios

GIVEN lens with history and snapshots disabled WHEN `GET /privacy` THEN `200`, `text/html; charset=utf-8`, `public, max-age=3600`, and the body holds requirement 5's statements.
GIVEN the same app WHEN `GET /` and `GET /some-client-route` THEN both answer the SPA shell as today.
GIVEN a counting `badge_check_fn` stub WHEN `GET /privacy` THEN the stub was never called.
GIVEN `netray-web`'s routers WHEN every router module is merged into one app THEN it builds without a duplicate-route panic and `/privacy` is routed once.
GIVEN `specs/sdd/v2.md` WHEN read THEN §4.7 names `/privacy` on `netray-web`, mounted by lens until V5.1, and the Phase 2 table marks V2.7 and V2.8 as built by `v2-entries`.
GIVEN netray-ops after finish WHEN `specs/netray/sdd/privacy-policy.md` is read THEN it carries the history exception with "90 days" and `/privacy`, committed and unpushed.

## Decision log

- Lookup order history → V1 snapshots → `410` over telling the two apart by ID shape: both stores issue the same 8-character shape (`crates/store/src/lib.rs:16-24`, `crates/lens/src/snapshot/store.rs:13-19`), and changing either shape would break V1 links or §4.1. A cross-store collision is about N_entries × N_snapshots / 62⁸ (≈ 5·10⁻⁵ at 10⁵ each); the entry wins then, and V5.6 ends the overlap.
- A well-formed ID in neither store answers `410`, not `404`: a swept ID cannot be told from an unknown one, because the sweep deletes the row (§4.6); an expired V1 snapshot therefore also answers `410` when the history is enabled, as V5.6 makes every V1 ID do.
- Today's `/r/` behaviour kept exactly without a history store over switching it unconditionally: `snapshot_routes.rs` stays protected and unchanged, and `[history] enabled = false` stays a switch back to V1.
- A failed history lookup answers `500`, never `410` or the V1 page: Rule M's "a failed lookup is never an answer" applied to the store.
- The entry page and card read only the stored entry; no score percentage and no re-scoring: §3.2 step 6 ("none computes a verdict"), and §4.1 stores no percentage.
- Pages, routes and the Entry → view mapping in `netray-web`, mounted by lens ahead of its SPA fallback, over lens-held routes and mapping with a route-free `netray-web`: SDD §3.1 assigns routers and templates to `netray-web`, and v2-cutover then moves nothing of this feature (operator, 2026-10-17, X1).
- V1 snapshot lookup and the V1 card renderer handed in from lens as functions over moving `snapshot/` and `og/` into `netray-web`: both go at the cutover (V5.6) and with v2-result-page's spectrum card, and `netray-web` cannot depend on lens (implementer, 2026-10-17).
- Entry OG card in the V1 layout over the spectrum layout: v2-result-page owns the switch to `spectrum::layout` (operator, 2026-10-17, X4).
- §4.7's text only on `/privacy` over the whole policy now: the whole policy needs facts outside the monorepo (log retention on the host, GDPR Art. 13 controller details) and is a separate feature after this program (operator, 2026-10-17).
- The overnight run commits the netray-ops `privacy-policy.md` change over leaving it to the operator: the history makes the "aggregates only" line false the moment it is enabled; the operator reviews and pushes (operator, 2026-10-17).
- Served by lens over `site/` and `netray site`: production sends the apex's unlisted paths to lens already (netray-ops compose template :44, private), so `/r/` and `/privacy` need no routing change, and `site/` ends with V5.1.
- Inline stylesheets in this feature's frame over a `/static/` route of its own: `v2-pages` builds the `/static/` serving; lens's CSP allows inline styles (default, not interviewed).
- Lane split with `v2-pages`: each creates only what its pages need, and the second to land merges (requirement 20) over one lane waiting for the other: both start the same night. `/privacy` is owned by `entries::privacy_router`, and `pages::router()` serves no `/privacy`, because lens merges both routers ahead of its SPA fallback (X1; v2-pages requirement 33 merges `pages::router()` unconditionally) and axum panics on a path routed twice (implementer, 2026-10-17).
- Router modules `entries/` and `pages/` side by side over one shared module: the lanes compose without editing each other's files, and the guard's allow-list names both.
- The entry's `og.png` answers `410` for a live V1 snapshot's ID over `404`: V1 never linked such a card, and one rule for "not an entry" is simpler than a second store lookup.
- No entry ID on the lens wire: the V1 wire and goldens stay byte-identical (as `v2-history` decided for the diff); entries become reachable from clients with API v1 and the result page.
- `/r/{id}/{*rest}` other than `og.png` stays `404` here: the evidence views (V4.7) and v2-cutover's 13j give those paths meaning.
- `v2-inspectors` narrows `v2-entries`' `/r/{id}/{*rest}` catch-all to `/r/{id}/{segment}/{*rest}`
  under `ADLC-Test-Change` and mounts its `/r/{id}/{segment}` route only where `entries::router` is
  mounted (`[history] enabled`), over `v2-entries` pre-narrowing a route it gives no meaning:
  `v2-entries` lands first (lane A), the feature that gives the segment meaning changes the rows,
  and without a history store `/r/` stays exactly V1's (implementer under the operator's autonomy mandate, 2026-10-17).
- One `templates/base.html` with `styles`, `header`, `main` and `scripts` blocks, whose defaults are
  `v2-pages`' frame; `v2-entries`' entry, gone and privacy pages override `styles` with their one
  inline `<style>` and `header` and `scripts` with nothing; whichever of `v2-entries` and
  `v2-pages` lands second merges onto the other's frame. This over lens mounting `/static/` for
  `v2-entries`' pages in the merge: both specs state one rule, and those pages render whether or
  not the pages router is mounted (implementer under the operator's autonomy mandate, 2026-10-17).
- Each spec asserts the `test_web_crate.sh` shape it leaves (Router allowed under
  `crates/web/src/entries/`, `pages/` and `api/`; axum allowed; the `netray` → `netray-web` guard
  lifted by `v2-pages` or `v2-api`, whichever lands first) and lifts only what is still in place,
  over asserting guards a parallel lane may already have lifted (implementer under the operator's autonomy mandate, 2026-10-17).
- One build order in every spec (lane A `v2-history` → `v2-admission` → `v2-entries` → `v2-api`;
  lane B `v2-spectrum` ∥ `v2-pages`, parallel to lane A; lane C `v2-inspectors`, its Phase 2 before
  `v2-result-page` Phase 1, then `v2-result-page`; then `v2-cutover`) over each spec's own reading
  of the program: every **Depends on** and Context names the same order (implementer under the operator's autonomy mandate, 2026-10-17).
- Frame order `styles` in the head; `header`, `main`, footer, `scripts` last in the body over
  `styles`, `header`, `main`, `scripts`, then the footer: v2-pages requirements 9 and 32 fix that
  order for the one shared `base.html`, and two orders would make the lane merge rewrite the frame
  (implementer under the operator's autonomy mandate, 2026-10-17).
- Gone-page sentence "This entry has expired and is no longer stored." over "Entries are kept for
  90 days after a domain was last checked …": the page also answers expired V1 snapshot IDs and
  never-issued IDs, which the 90-day rule does not cover, and §4.5/§4.7 keep an entry 90 days after
  it was last seen, not after the domain was last checked, with the latest entry kept beyond that
  (implementer under the operator's autonomy mandate, 2026-10-17).
- Grade tones (pass A+/A/B, warn C/D, fail F) and the gone-page copy built as mandate defaults over
  halting for an interview: both are user-visible, the overnight run must not halt on them, and the
  operator reviews them from screenshots at the midday UI review on 2026-10-18, as v2-pages does
  with its approved defaults (implementer under the operator's autonomy mandate, 2026-10-17).
- A failed history lookup forced by `DROP TABLE entries` over a database file made unreadable
  (`chmod 000`): the open pool keeps answering through its held connections, so the file mode
  forces nothing; dropping the table is the repo's method (`crates/lens/tests/history_record.rs:374`)
  (implementer under the operator's autonomy mandate, 2026-10-17).

## Open decisions

None.

## Out of scope

- History-as-cache, run join, throttle, admission: `v2-admission` (SDD §4.4, V2.6).
- Evidence views `/r/<id>/<protocol>`, the full result page, spectrum, trace, R19 lines: Phase 4 (SDD §5, §6; V4.4–V4.7, V4.13).
- The spectrum OG card: v2-result-page (V4.4). The domain card `/og/<domain>.png` from the stored entry: §5.1, with `v2-admission` or Phase 4.
- V1 snapshot IDs answering `410` and removing `snapshots.db`: V5.6.
- The full privacy policy (request logs, GeoLite2, browser storage, cookies): a separate feature after this program.
- The frame's header, navigation, islands, speculation rules, `/static/` serving and page CSP: `v2-pages`.
- Linking `/privacy` from the SPA footer: `[site] footer_links` in the deployment config (netray-ops).
- The generated sitemap entry for `/privacy`: `v2-pages`.
