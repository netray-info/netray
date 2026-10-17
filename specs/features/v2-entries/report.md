# Report: v2-entries

## Phase 1 — Frame and page templates in netray-web

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: `templates/base.html` frame (doctype, lang, metas, `styles`/`header`/`main`/footer/`scripts` order); pages inline one `<style>` (tokens, base.css, ten component sheets) and empty `header`/`scripts` | green | entries_frame.rs |
| C2 | R2: module `entries` with `render::{entry,gone,privacy}` on `templates/pages/*.html`; markup only from R44 macros; no new stylesheet; gate passes | green | entries_gate.rs |
| C3 | R3: entry page order h1, grade (tone by letter, no score), run line, diff block, check rows, counts; title/og metas; escaping | green | entries_page.rs |
| C4 | R4: gone page heading, sentence, query_input; no retention period, no domain/ID, no og metas | green | entries_gone.rs |
| C5 | R5: privacy page carries §4.7's statements only | green | entries_privacy.rs |
| C6 | R6: `entries::view` maps Entry to EntryPageView (timestamp, fail-then-warn in catalogue order, unknown ID, counts); `format_utc_timestamp` moved, lens re-exports | green | entries_view.rs, lens og_timestamp_moved.rs |
| C7 | R7: diff block mapping (title, summary forms, kinds, tones, evidence items, engine note) | green | entries_diff.rs |
| C8 | Scenario: grade-B view with two issues and a diff renders in order with og:image `/r/<id>/og.png` | green | entries_page.rs |
| C9 | Scenario: view without a diff block has no `diff-block` class | green | entries_page.rs |
| C10 | Scenario: `<script>` in domain or check title is escaped; no `<script` element | green | entries_page.rs |
| C11 | Scenario: every page starts with doctype, one `<style>` with a token property and `.check-row`, no `<script`, no `/static/`, footer links `/privacy` | green | entries_frame.rs |
| C12 | Scenario: gone page heading, sentence, form action `/` name `d`, no og meta, no "90", no domain but `example.com` | green | entries_gone.rs |
| C13 | Scenario: privacy page holds each R5 statement; no "cookie", "GeoLite2", "log" | green | entries_privacy.rs |
| C14 | Scenario: diff tls.not_expired fail → pass, C → B: "fixed" item tone fix, summary "C → B" | green | entries_diff.rs |
| C15 | Scenario: `rescored` gives the "(profile)" step; `engine_changed` the note; neither, neither | green | entries_diff.rs |
| C16 | Scenario: first entry has no diff block; swept predecessor with kept diff has one | green | entries_diff.rs |
| C17 | Scenario: fail rows precede warn rows, catalogue order, no pass row | green | entries_view.rs |
| C18 | Scenario: unknown check ID, warn: ID as title, no fix, no guide link | green | entries_view.rs |
| C19 | Scenario: netray_web and lens `format_utc_timestamp` agree | green | crates/lens/tests/og_timestamp_moved.rs |
| C20 | Scenario: gate passes the new templates; a page template writing `class="check-row"` is refused (rule 2) | green | entries_gate.rs |

Test files are under `crates/web/tests/` unless named with a path. Two test assertions searched the whole page for a component class name that the inlined stylesheet also holds (`diff-block`, `grade__score`); they now search the markup (`class="…"`) under `ADLC-Test-Change`.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet (two test assertions corrected by the orchestrator under `ADLC-Test-Change`) | 82306 | 154 |

### Review

| pass | class | finding | outcome |
|---|---|---|---|
| 1 | BLOCKER | gone and privacy pages did not override `header` and `scripts` with nothing (R1) | fixed; `pages_override_header_and_scripts_with_nothing` |
| 1 | AMENDMENT | evidence change with no items rendered "<id>: 0 changed" | R7 amended ("<id> changed"); repaired in phase |
| 1 | AMENDMENT | `<nav` refused inside the frame's footer, which v2-pages' footer holds | R1 amended (header navigation); test refuses `<nav` outside the footer; repaired in phase |
| 1 | DEFERRED | v2-admission not yet merged; the privacy page's throttle statement holds once it is | lane A order; rebase before finish |
| 2 | — | none | — |

### Behavioural verification

skipped: Phase 1 adds library render functions and templates only; no route, CLI or UI action is wired until Phase 2. The pages are exercised end to end through `entries::render` in the `entries_*` tests.

## Phase 2 — The routers in netray-web, `/r/<id>` mounted in lens

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R8: netray-web gains axum, `entries::router(EntriesDeps)` serving the five `/r/` paths; `EntriesDeps` holds the store, an optional V1 lookup and the card renderer as functions; `NOT_FOUND_HTML` byte-identical, used by lens's `snapshot_router` | green | crates/web/tests/entries_router.rs, crates/lens/tests/entries_mount.rs |
| C2 | R9: `test_web_crate.sh` allows a Router only under `entries/`, `pages/`, `api/`, allows axum; `netray` → `netray-web` guard as found; lens depends on netray-web | already_implemented | tests/repo/test_web_crate.sh |
| C3 | R10: `/r/{id}` precedence: malformed 404, entry 200, live V1 its page, failed V1 500, else 410; entry wins over V1 | green | crates/web/tests/entries_router.rs |
| C4 | R11: entry page and gone page content-type and `public, max-age=3600` | green | crates/web/tests/entries_router.rs |
| C5 | R12: failed history lookup 500, warn log with the ID, never 410/V1/entry | green | crates/web/tests/entries_router.rs |
| C6 | R13: lens mounts `entries::router` when history enabled (V1 lookup when snapshots enabled), else `snapshot_router`, else nothing; before the SPA fallback, inside the middleware | green | crates/lens/tests/entries_mount.rs |
| C7 | R14: `/r/`, `/r/{id}/`, `/r/{id}/{*rest}` but `og.png` 404 `NOT_FOUND_HTML`; no run, cache, limiter or store write | green | crates/web/tests/entries_router.rs, crates/lens/tests/entries_mount.rs |
| C8 | R17: SDD §4.6 states the lookup order, 410/404 and the minimal page on netray-web's router mounted by lens | green | tests/repo/test_entries_sdd.sh |
| C9 | R20: lane merge with v2-pages by whichever lands second | test-unwritable | — |
| C10 | Scenario: entry → 200 entry page with its domain, `public, max-age=3600` | green | crates/web/tests/entries_router.rs |
| C11 | Scenario: swept entry → 410 gone page without the domain | green | crates/web/tests/entries_router.rs |
| C12 | Scenario: never-issued `AAAAAAAA` with a history store → 410 | green | crates/web/tests/entries_router.rs |
| C13 | Scenario: live V1 snapshot through lens's app → 200 V1 page, `public, max-age=86400, immutable` | green | crates/lens/tests/entries_mount.rs |
| C14 | Scenario: failing V1 lookup, well-formed non-entry ID → 500 | green | crates/web/tests/entries_router.rs |
| C15 | Scenario: entry and V1 snapshot share an ID → entry page | green | crates/web/tests/entries_router.rs, crates/lens/tests/entries_mount.rs |
| C16 | Scenario: `/r/x`, `/r/AAAA-AAA`, `/r/%C0`, `/r/`, `/r/AAAAAAAA/`, `/r/a/b` → 404 byte-equal `NOT_FOUND_HTML` | green | crates/web/tests/entries_router.rs |
| C17 | Scenario: dropped `entries` table → 500, warn log naming the ID | green | crates/web/tests/entries_router.rs |
| C18 | Scenario: no history store → `snapshot_routes.rs` unchanged and green | already_implemented | crates/lens/tests/snapshot_routes.rs (unchanged) |
| C19 | Scenario: history on, snapshots off → entry 200; both off → `/r/AAAAAAAA` equals the SPA's answer | green | crates/lens/tests/entries_mount.rs |
| C20 | Scenario: counting `badge_check_fn` never called for entry, swept and snapshot IDs; `last_seen_at` unchanged | green | crates/lens/tests/entries_mount.rs |
| C21 | Scenario: `test_web_crate.sh` passes with axum and Router under `entries/`; planted Router in `components.rs` fails; `netray` → `netray-web` check as found | already_implemented | tests/repo/test_web_crate.sh |

C2 and C21 pass from the test commit on (it adds axum to `netray-web`; Phase 1 made lens depend on `netray-web`). C18 is the protected `snapshot_routes.rs`, green unchanged. C9 (requirement 20) cannot be tested on this branch: `v2-pages` has not landed, so the frame merge is applied by whichever lands second, in its rebase; the allow-list half is C2.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 40091 | 42 |
| G2 | 1 | sonnet (orchestrator boxed `CardRenderer`'s error to drop a `clippy::result_large_err` allow) | 37738 | 91 |

### Review

| pass | class | finding | outcome |
|---|---|---|---|
| 1 | — | none | — |

### Behavioural verification

`netray lens` from this worktree's build with `[history]` on (default) and `[snapshots] enabled = true`, fresh databases:

```
== GET /r/AAAAAAAA
HTTP/1.1 410 Gone
content-type: text/html; charset=utf-8
cache-control: public, max-age=3600
<!DOCTYPE html><html lang="en"><head>…<title>Entry expired</title><style>:root {…
== GET /r/x
HTTP/1.1 404 Not Found
content-type: text/html; charset=utf-8
<!DOCTYPE html><html><head><title>Not found</title></head><body><h1>Snapshot not found</h1>…
== GET /r/AAAAAAAA/og.png
HTTP/1.1 404 Not Found   (Phase 3 builds the card)
== GET /privacy
HTTP/1.1 200 OK, the SPA shell (Phase 4 mounts the privacy page)
```

The security headers (`x-content-type-options: nosniff`) show the router runs inside lens's middleware. An entry's `200` needs a recorded run (no network here); `entries_router.rs` and `entries_mount.rs` cover it.
