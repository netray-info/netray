# Report: v2-v1-remnants

## Phase 1 — OG footer

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: without a label the footer reads exactly netray.info, no separator | green | crates/web/tests/it/og_footer.rs |
| C2 | R2: a non-empty label keeps label · netray.info; an empty label renders netray.info | green | crates/web/tests/it/og_footer.rs |
| C3 | R3: CHANGELOG states the footer | green | CHANGELOG.md (prose, read) |
| C4 | default label, svg_for_grade footer exactly netray.info, no footer separator | green | crates/web/tests/it/og_footer.rs |
| C5 | default label, svg_for_entry footer exactly netray.info | green | crates/web/tests/it/og_footer.rs |
| C6 | label custom renders custom · netray.info | already_implemented | crates/web/tests/it/og_footer.rs |
| C7 | empty label renders exactly netray.info | green | crates/web/tests/it/og_footer.rs |
| C8 | /og/example.com.png without label equals the PNG of the no-label card | green | crates/web/tests/it/history_cache_badge_og.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 og footer (layout.rs, render.rs) | 1 | implementer, no coder: a two-line change | 0 | 0 |

Tests written by the implementer, not test-writer agents: five scenarios over one renderer function.

### Review

- AMENDMENT (phase 1, repaired): the decision log now supersedes v2-release-cleanup's requirement 6 footer.
- NIT `crates/web/src/pages/og.rs:45`: an explicit `?label=` card keeps the 0.26.0 ETag input while its bytes change, so a cache revalidating across the deploy may keep the old unscored card until a head exists. Not acted on.
- NIT `crates/web/src/og/render.rs:99`: a label of only spaces counts as non-empty and renders `· netray.info`; requirement 2 as written. Not acted on.

### Behavioural verification

skipped: the route is exercised end to end through the router (`history_cache_badge_og::v1r_c8_the_og_route_draws_no_label_by_default`); a live `netray serve` adds nothing the PNG byte comparison does not cover.
