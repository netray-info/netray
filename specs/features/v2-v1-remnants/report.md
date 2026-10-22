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

## Phase 3 — Dead code

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R10: netray_common::ecosystem and ::metrics deleted, serde_json a dev-dependency only | green | tests/repo/test_v1_remnants.sh |
| C2 | R11: no module crate re-exports RequestId or enables the middleware feature | green | tests/repo/test_v1_remnants.sh |
| C3 | R12: crates/tls/src/routes.rs is crates/tls/src/inspection.rs, every user renamed | green | tests/repo/test_v1_remnants.sh |
| C4 | R13: test_v1_remnants.sh fails while 10 to 12 do not hold | green | tests/repo/test_v1_remnants.sh |
| C5 | today's tree fails the check naming each remnant | green | tests/repo/test_v1_remnants.sh |
| C6 | the cleaned tree passes it, the workspace builds with all targets and the gate passes | green | tests/repo/test_v1_remnants.sh |

### Behavioural verification

```
$ cargo build --workspace --all-targets --all-features   # finished, no new warning
$ bash tests/repo/test_v1_remnants.sh
PASS: test_v1_remnants
```

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 common, tls rename, re-exports | 1 | implementer, no coder: deletions and a mechanical rename | 0 | 0 |

### Review

- BLOCKER (fixed, second pass clean): `netray-dns` also re-exported `netray_common::middleware::request_id` as `request_id_middleware` (no user); with the feature dropped, `cargo check -p netray-dns` failed alone while the unified gate passed. The re-export is gone; `cargo check -p <crate>` and `--tests --features testing` pass for dns, tls, http and email, `-p netray-common --tests` with and without `--all-features`.
- AMENDMENT (phase 3, repaired): `test_v1_remnants.sh` passed when `cargo metadata` failed; it now fails when the scan produced nothing (`PATH=/usr/bin:/bin` gives "cargo metadata produced nothing").
- AMENDMENT (phase 3, repaired): the check did not test the `middleware` feature; it now reads each module crate's `netray-common` features from `cargo metadata` (adding it back to tls fails the check).
- AMENDMENT (phase 3, repaired): `crates/common/README.md` and `CLAUDE.md` named the deleted `ecosystem` and `metrics` modules and `EcosystemConfig`.
- NIT (repaired): `tests/repo/test_config_strict.sh`'s scope comment named `EcosystemConfig` and the gone `BackendConfig`.

## Review fixes (after e2bb5899..a29fecfb)

- The 19 TLS test files the module rename touched carry `ADLC-Test-Change` trailers; `adlc check` no longer lists them.
- `docs/ip-enrichment.md`: `user_agent_regexes` must load when configured (load error at startup, the whole previous data kept on reload).
- `tests/repo/test_v1_remnants.sh` reads `crates/tls/src/lib.rs` (`pub mod inspection;`, no `routes` or `#[path]`), `crates/tls/CLAUDE.md` and the old module path in the tls and web crates; `#[path = "inspection.rs"] pub mod routes;` and `src/routes.rs` in CLAUDE.md each fail it.
- `tests/repo/test_crate_docs.sh` matches record files with `:(glob)` pathspecs, so only `crates/<crate>/docs/` and `specs/` count.

## Serve-test port race (coordinator, 2026-10-22)

`crates/netray/tests/support/mod.rs` (`listening`, `bind_failed`, `ATTEMPTS`) and a labelled retry loop in the ten tests that spawn `netray serve` on `free_port` ports: serve_api_inspect_tables, serve_api_metrics, serve_entries_gone, serve_hosts, serve_inspector_hosts, serve_inspector_links, serve_metrics, serve_probe_headers, serve_reserved_paths, serve_startup. All ten pass; a serve started on an occupied port exits with `AddrInUse` in its log, which `bind_failed` reads.
