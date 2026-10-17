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
