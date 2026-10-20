# Plan: v2-result-fidelity

## Phase 1 — Frame, header and hero

### Groups

G1: C1, C2, C3, C4, C7, C8, C27 · G2: C5, C6, C9, C10, C11, C12, C14–C23, C25 (C24 already_implemented) · G3: C13, C26 · G4: C28 (design-check run)

G1 owns every stylesheet, including the rules for G2's markup. G2 carries store, lens and result views together because `Entry.duration_ms` changes the `Entry` literal in `crates/web/src/pages/result/mod.rs`.

### Plan

G1 — tokens (`font.size.button` 14px, `font.tracking.grade` -0.04em, `layout.gutter` 32px, `layout.gutterPhone` 16px; SDD §8.2 lists the phone value); `base.html` header: logo, `<span class="page-tagline">` from a new `partials/header_tagline.html` ("your domain, in full spectrum"; the footer keeps `partials/tagline.html`), `<nav class="page-nav">` Inspectors `/inspect/dns`, Guides `/guide/`, API `/api/v1/docs`, Status `/status`, theme button last in the nav; footer nav Privacy, Status, Compare. `static/button.css` (`.button`, `.button--primary`, `.button--secondary`) joined into `APP_CSS` (`crates/web/src/pages/assets.rs`). `pages.css`: container (`box-sizing: border-box; padding-inline: var(--layout-gutter)`) on `.page-header`, `main`, `.page-footer`; `.page-tagline`; nav muted; `.theme-toggle` unboxed; `.result-hero` row (flex, center, wrap, gap 28px), `.result-hero__title` (flex 999 1 360px, column, gap 6px), `.result-hero__actions`; `#result-header h1` (h1 token, weight strong, tracking display, line-height 1.1, margin 0); `.hard-fail`, `.hard-fail__without`. `grade.css`: bar `var(--line-fail)`, tones on the status `text` tokens, letter tracking grade and line-height 0.85, `.grade__score` (mono, small, muted), `.grade__score strong` (ink, weight bold as the board's `<strong>`), `.grade__descriptor` (mono, caption, muted). `run_line.css`: `.run-line--title` (mono, caption, caps tracking, uppercase). `result_header.html`: `<header id="result-header" class="result-hero">`, grade, `.result-hero__title` (run line, h1, progress), `.result-hero__actions` (form, share); every id stays. `result_recheck_button.html`: classes `button button--primary`. `result_share.html`: `<a id="share" class="button button--secondary" href="/r/<id>">Share</a>` (the share dialog of Phase 7 builds on this link). Tests adjusted with `ADLC-Test-Change`: `pages_frame.rs` (header hrefs, labels), island fixtures regenerated (`UPDATE_GOLDEN=1` for `result_islands_fixtures`, `result_islands_issues_fixture`, `result_islands_trace_fixture`).

G2 — migration `0002_entries_duration.sql`; `Entry.duration_ms`, `record_timed` (record delegates with None), `insert` binds it, Identical leaves it; SDD §4.1 row. Lens `history::record` takes the duration, `admission.rs` passes the elapsed run time. `crates/web/src/grade_legend.rs` `descriptor(Grade)` (V1 gradeLegend.ts texts, "descriptor, meaning"; Incomplete "incomplete, not every section measured"). `GradeView::descriptor()`; grade macro renders `score <strong>N</strong> / 100` and `.grade__descriptor`. `run_line.html` gains macro `run_title` (`run-line run-line--title`). `views.rs`: `run_title(...)`, `section_name(Protocol)`, `hard_fail_view(run, &Score, catalogue)`. `mod.rs`: `RunLineBody::Title`, score via `score_run` with the entry's profile shown only when complete and the grades agree (warn naming the entry otherwise), cached flag from `full_page` (fresh) and `run.rs::stored_answer` (true), banner partial `result_hard_fail.html` inside a stream-replaced wrapper `<div id="hard-fail-slot">` (added to `fragments()`), headings: "Since run k−1" h2 before the diff (first entry: a first-run heading), "<n> lines need attention" h2 first in `#issues`, "Evidence" h2 in `#evidence`, labels "Re-check now" / "Check now". SDD §5.7 page order amended. Tests adjusted with `ADLC-Test-Change`: `Entry` literals (`duration_ms: None`), entry id assertions moved from the run line to the share link, stream fragment lists gain `hard-fail-slot`, island fixtures regenerated, acceptance spec reads the id from `#share`.

G3 — `entry.html` keeps title/meta, drops the `styles` and `header` overrides and carries the frame's scripts so the header's Theme button works (`EntryPage` gets a `Frame`); `render.rs` keeps `stylesheet()` for gone and privacy. `entries_frame.rs`, `entries_page.rs` adjusted with `ADLC-Test-Change`; frontend-rules §1 updated in the prose commit.

G4 — `just design-check`; rows into report.md; fixes go to G1's stylesheets.

## Phase 2 — Spectrum figure

### Groups

G1: C1–C13 (one group: every criterion touches the spectrum layout, its component or stylesheet) · G2: C14 (design-check run)

### Plan

G1 — `crates/web/src/spectrum.rs` `run_heights`: height = 0.35 + 0.65 × w / w_max over measured lines (w the effective weight it computes today), muted lines unchanged; the view (`SpectrumView` in `crates/web/src/components.rs`) gains the tick list (N + 1, major at the ends and region bounds) and per-region left/width percentages and an optional section grade; `figure_parts` in `crates/web/src/pages/result/mod.rs` passes section grades from `score_run`'s section percentages through the profile thresholds (expose a lookup in `netray_engine::scoring`). `templates/components/spectrum.html`: ticks in `.spectrum__axis`, region label spans with `left`/`width`, `<span class="spectrum__region-grade">`, label band 56 px for up to three rows, leaders 8/22/36 px by label index. `static/spectrum.css`: plate radius, axis 32 px, ticks 4/10 px, region labels caption mono uppercase .14em centred, line labels caption mono 600 transparent. `partials/result_spectrum.html`: the legend with `.legend__mark` entries (pass, warn, fail, "fixed since run k−1" when a predecessor exists) and `.legend__note`; legend rules in `pages.css`. Catalogue short of `tls.expiry_window` "Expiry". SDD §5.4 amended (height rule). OG plate goldens regenerated if the heights move them.

G2 — `just design-check` with the phase 2 manifest elements; rows into report.md.

## Phase 3 — History trace, diff and first visit

### Groups

G1: C1, C5, C6, C7, C8, C9, C11, C15, C16, C17, C18 and the server half of C2, C3, C12, C13 (`fidelity_replay_markup.rs`) · G2: C2, C3, C4, C12, C13, C14 (spectrum island) · G3: C19 (design-check)

### Plan

G1 — `templates/components/trace.html` + `static/trace.css`: rows `li.trace__row[data-run]` with `data-run-no`, `data-date`, `data-grade`, no date/grade text; one cell per line with a centred 5 px mark (pass 1 px hair, warn/fail 3 px, untested muted); row height `--spectrum-trace-row`; edge dates `.trace__edge[data-edge]`. `views::trace_view` supplies run numbers, dates and grades. `partials/result_spectrum.html`: the figure as a grid with the gutter column (`#spectrum-gutter.spectrum-gutter` with run, date, grade spans), the caption and hint with `data-rest`, the table link `.trace__table-link` to `/<domain>/history`, region grades and the legend's fixed entry per run inside `[data-run]` wrappers. `views::line_code(&CheckId)` (tag and 1-based spectrum position); `diff_block` component: marker, kind (70 px), text, code; no in-box title or summary for a stored entry; `.since-meta` after the since h2 (predecessor date, grades). First entry: `<section id="diff" class="first-run">` with title, text and ghost (one row of cells, two dashed rows). Island fixtures regenerated.

G2 — `islands/src/spectrum.ts`: hover/pin set `data-hovered`/`data-dimmed`/`data-pinned` on rows, fill the gutter from the row's data attributes and set `data-replaying`, swap caption/hint/run-title text from `data-rest`, restore on leave; no style writes; bundle under the cap.

G3 — `just design-check` with the phase 3 manifest elements (FTrace trace, caption, gutter; F since and diff; FFirst first-run box; F/FDark "history rows" accepted).

## Phase 4 — Open issues and passed checks

### Groups

G1: C1, C3–C8, C10–C15 (C2, C9 already in place) · G2: C16 (design-check)

### Plan

G1 — `partials/result_issues.html`: head row `.section-head.section-head--split` with the h2, `.attention-meta` ("f fail · w warn") and the visible switch (Owner, Severity, Protocol); groups as cards `.issue-group` with `.issue-group__head` (`.issue-group__label` h3, `.issue-group__count` "n lines"/"1 line"). `components/check_row.html` + `static/check_row.css`: side column (`.check-row__side`: bar, severity word, line code) and content column (title, since, body, fix, `.check-row__meta` "Protocol · source" with "evidence →" and "guide →" links); the evidence views, inspectors and `/r/<id>` keep working with the same macro (their views supply the new fields). `static/grouping_switch.css`: unboxed tabs, 36 px, mono label size, bottom rule, pressed ink bold with the marker. `views::since_text(statuses, first_run_no)` replaces the month-day since in the result page. `partials/result_passed.html`: `details#passed.clean-bar` with the summary bar; the coverage line leaves `#evidence` (its links return in Phase 5's tabs). `pages.css`: `#result` flex column with `gap: var(--space-9)`, `.attention-meta`, clean bar. Island fixtures regenerated; `issues.ts` keeps grouping working with the new markup (its tests stay green).

G2 — `just design-check` with the phase 4 manifest elements.

## Phase 5 — Evidence section

### Groups

G1: C1–C6 (one group: the evidence partial, its view, the tabs island and stylesheet) · G2: C7 (design-check)

### Plan

G1 — `templates/pages/result.html` and `partials/result_evidence.html`: the head row (`.section-head`, h2 "Evidence", `.evidence-meta`), the card `.evidence-tabs[data-island="tabs"]` with the nav of five tab links (`#evidence-<p>`, `data-tab`, `marker`, `aria-current` on the current) and five panels `div#evidence-<p>.evidence-tabs__panel[data-panel]` (the stream ids), the current one `data-shown`. `crates/web/src/pages/result/mod.rs` (`EvidenceInner` and the stored/live/unrecorded paths): per panel the protocol's blocks through the evidence_block component, `.evidence-tabs__feeds` ("Feeds lines: <code> <short> · …" for the lines whose checks reference the panel's blocks), `.evidence-tabs__all` to `/r/<id>/<p>` (stored only), `.evidence-tabs__explore` from `explore::view_link`, `.evidence-tabs__reason` for an unmeasured or not-applicable section; the current tab: TLS when TLS has a non-pass line, else the first protocol with one, else DNS. Live fragments render the same panel element. `islands/src/tabs.ts` registered as "tabs": data-ready on mount, click and arrow keys switch `data-shown`/`aria-current`, re-show the current panel after a stream fragment. CSS (a section in `pages.css`): card, nav rule, tab font and marker, panel padding, `[data-ready]` hiding of non-current panels, phone table scroll and "← swipe the table". Island fixtures regenerated.

G2 — `just design-check` with the phase 5 manifest elements.

## Phase 6 — Phone layout

### Groups

G1: C1–C12 (one group: the phone values touch the frame, the result partials and the component stylesheets together) · G2: C13 (design-check M, MDark)

### Plan

G1 — `base.html`: `details.page-menu` with `summary.page-menu__button` (three bars) around `nav.page-nav`; the Theme button after the details, last in the header row (desktop: summary hidden, nav shown; phone: tagline hidden, logo 58 × 33, the menu button a 44 × 44 box, the closed menu hides the nav, nav links 44 px). The run title marks its profile part `data-part="profile"` (hidden at phone width). Phone rules (`@media (max-width: 480px)` in pages.css and the component stylesheets): hero values (grade 64 px, h1 26 px / 1.15, buttons 15 px, Re-check flex 1), spectrum without the inline min-width, labels 10 px, region labels 9 px .1em, the email region's short name "MAIL" through `data-short`, trace rows 6 px (token), the stepper bar (buttons 44 × 44 with line border, centre box with run, date, grade and state) and its hint, h2 19 px, the diff row meta line `.diff-block__meta` (kind · code) with kind and code hidden, the check row's `.check-row__phone-meta` (SEV · code · since) above the title with a one-column row, 44 px tap targets for every link and button the frame and the result page carry. `fidelity_header.rs` follows the menu markup (Theme last in the header row, outside the details). Island fixtures regenerated; the spectrum island's stepper fills the centre box from the shown row.

G2 — `just design-check` with the M and MDark manifest elements.
