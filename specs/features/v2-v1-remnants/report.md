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

## Phase 2 — Docs

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R4: crate-level doc comments and common module headers describe V2 | green | tests/repo/test_crate_docs.sh |
| C2 | R5: descriptions name no service, repository is the monorepo, no manifest names a missing file | green | tests/repo/test_crate_docs.sh |
| C3 | R6: no tracked crate docs/, specs/, ROADMAP.md, SECURITY.md | green | tests/repo/test_crate_docs.sh |
| C4 | R7: IP classification reference at docs/ip-enrichment.md, V2 wording, linked from crates/ip | green | tests/repo/test_crate_docs.sh |
| C5 | R8: root SECURITY.md covers the whole repository with scope lists and the private channel | green | tests/repo/test_crate_docs.sh |
| C6 | R9: test_crate_docs.sh enforces 4 to 6 | green | tests/repo/test_crate_docs.sh |
| C7 | R14: deleted records reachable from the decision log, quality.md superseded there | green | specs/features/v2-v1-remnants/spec.md (read) |
| C8 | today's tree fails the check naming records, descriptions, repositories, doc comments | green | tests/repo/test_crate_docs.sh |
| C9 | the cleaned tree passes the check | green | tests/repo/test_crate_docs.sh |
| C10 | a staged crates/dns/docs/notes.md fails the check | green | tests/repo/test_crate_docs.sh |
| C11 | root SECURITY.md names the private channel, scope lists and mhost out of scope | green | tests/repo/test_crate_docs.sh |
| C12 | crates/ip/README.md links docs/ip-enrichment.md | green | tests/repo/test_crate_docs.sh |

### Behavioural verification

```
$ git add crates/dns/docs/notes.md && bash tests/repo/test_crate_docs.sh
FAIL: V1 record files are tracked: crates/dns/docs/notes.md
$ bash tests/repo/test_crate_docs.sh   # cleaned tree
PASS: crate docs describe V2 modules
```

### Review

- AMENDMENT (phase 2, repaired): the email crate doc and description said "DNS only"; the module also fetches the MTA-STS policy and the BIMI logo.
- AMENDMENT (phase 2, repaired): `docs/ip-enrichment.md` said a batch reverse-resolves only with `?dns=true`; `1` and any case count too.
- AMENDMENT (phase 2, repaired): `docs/ip-enrichment.md` said a failed reload keeps the previous data; only a required source's failure does, an optional source is dropped.
- DEFERRED `crates/ip/src/enrichment.rs:457`: the `asn_patterns` gauge reports the key being set, not the file loading; the doc now says so, the code is unchanged.
- NIT (repaired): the ASN heuristic path in the moved doc is repository-relative now.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 docs, manifests, record files | 1 | implementer, no coder: prose and file moves | 0 | 0 |
