# Report: v2-inspectors

## Phase 1 — One renderer over the six shapes

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: one function `Block` → `EvidenceBlockView` matching on `Shape` only, six shapes, typed value formatting, no module/block name or key in the renderer | green | crates/web/tests/evidence_render_shapes.rs, evidence_render_values.rs |
| C2 | R2: `evidence_block` carries no `style` attribute; tree depth `data-depth` 0–7 (clamped) styled by `evidence_block.css`; `id="ev-<block id>"`; values escaped | green | crates/web/tests/evidence_render_depth.rs, evidence_render_escape.rs |
| C3 | S: block of each shape → `data-shape`, `id="ev-<block id>"`, every value in order | green | crates/web/tests/evidence_render_shapes.rs |
| C4 | S: fields `int 3`, `bool true`, `time`, `duration 120`, `addr 2001:db8::1` → `3`, `true`, `2026-10-08T14:32:00Z`, `120 ms`, `2001:db8::1` | green | crates/web/tests/evidence_render_values.rs |
| C5 | S: tree of depth 9 → `data-depth` 0..7, deepest two clamped, no `style` attribute | green | crates/web/tests/evidence_render_depth.rs |
| C6 | S: text block `<script>alert(1)</script>` → escaped | green | crates/web/tests/evidence_render_escape.rs |
| C7 | S: renderer source holds no block name (`dns.`, `tls.`, `http.`, `email.`, `ip.`) | green | crates/web/tests/evidence_render_source.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 42817 | 54 |

### Review

Reader: no findings.

### Behavioural verification

skipped: Phase 1 adds a library function and a macro change, no route or command; the rendered markup is asserted by the phase's tests.

### Verify

`adlc verify`: verified, 237 tests collected.

## Phase 2 — Evidence view model

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R3: view of `p` = run line; one check row per catalogue check of `p` in catalogue order (short, tone, title, findings or catalogue finding, fix_hint on warn/fail, guide link, one link per referenced block); owned blocks in run order with feeds; `not_applicable` reason above rows | green | crates/web/tests/evidence_section_rows.rs |
| C2 | R4: a block of another module is not rendered; a referencing check yields one line with the block's title linking to the owner's view `#ev-<block id>` | green | crates/web/tests/evidence_section_shared.rs |
| C3 | R5: run line `run of <YYYY-MM-DD HH:MM> UTC · entry <id>` (created_at); live incomplete `incomplete, not recorded` | green | crates/web/tests/evidence_section_runline.rs |
| C4 | R7: the section renderer takes a live run and renders the same markup as the stored view | green | crates/web/tests/evidence_section_href.rs |
| C5 | S: `tls.chain_trusted` referencing `tls.chain@192.0.2.1:443` → row links `#ev-tls.chain@192.0.2.1:443`, block feeds list its short | green | crates/web/tests/evidence_section_rows.rs |
| C6 | S: `tls.caa_compliant` referencing `dns.caa` → not rendered in TLS view, one line links DNS view `#ev-dns.caa`; DNS view renders it | green | crates/web/tests/evidence_section_shared.rs |
| C7 | S: warn check without findings → body catalogue `finding`, fix catalogue `fix_hint` | green | crates/web/tests/evidence_section_bodies.rs |
| C8 | S: stored check without findings/evidence → catalogue texts, no block link | green | crates/web/tests/evidence_section_bodies.rs |
| C9 | S: TLS section `not_applicable` "no A or AAAA record" → reason shown, every TLS row `untested` | green | crates/web/tests/evidence_section_scope.rs |
| C10 | S: entry `Ab3dE5fG` created 2026-10-08 14:32 UTC → `run of 2026-10-08 14:32 UTC · entry Ab3dE5fG`; live incomplete → `incomplete, not recorded` | green | crates/web/tests/evidence_section_runline.rs |
| C11 | S: check rows follow catalogue order and cover every check of the protocol | green | crates/web/tests/evidence_section_scope.rs |
| C12 | S: `href("Ab3dE5fG", Tls, Some(tls.chain@192.0.2.1:443))` → `/r/Ab3dE5fG/tls#ev-tls.chain@192.0.2.1:443` | green | crates/web/tests/evidence_section_href.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 2 | sonnet | 50083 | 63 |
| G1 (review fix) | 1 | sonnet | 32049 | 34 |

### Review

Pass 1: BLOCKER feeds listed only the view's own checks (fixed: every referencing check of any protocol); AMENDMENT path-relative `<owner>#ev-` links break from `/<domain>` and re-read the latest entry (repaired in phase: `entry_section` links `/r/<id>/<owner>#ev-`, the live `section` links `#ev-` on the same page; I8 and the decision log amended); DEFERRED same as the amendment for `/<domain>/<p>` (resolved by it). Pass 2: no findings.

Test fixes: `evidence_section_rows.rs`/`evidence_section_href.rs` `created_at` was 2026-05-08 17:44, not the asserted 2026-10-08 14:32.

### Behavioural verification

skipped: Phase 2 adds pure view-model functions and a partial, no route; Phase 4 serves them.

### Verify

`adlc verify`: verified.

## Phase 3 — Explore-live links

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R8: one explore-live link per view, one per row for `tls.addresses`/`email.mx`/`ip.addresses`, one per block for `dns.<type>`/`email.dmarc`/`email.mta_sts`, none otherwise, all from one function; values percent-encoded; apex-relative `/inspect/<tool>` | green | crates/web/tests/explore_dns.rs, explore_tls.rs, explore_email.rs, explore_http_ip.rs |
| C2 | S: `dns.mx` records sharing owner `example.com` → `/inspect/dns?q=example.com%20MX` | green | crates/web/tests/explore_dns.rs |
| C3 | S: `tls.addresses` rows `192.0.2.1`, `2001:db8::1` → `?h=192.0.2.1:443`, `?h=%5B2001:db8::1%5D:443` | green | crates/web/tests/explore_tls.rs |
| C4 | S: `email.mx` host `mx.example.com` → `/inspect/tls?h=mx.example.com:25` | green | crates/web/tests/explore_email.rs |
| C5 | S: `email.dmarc`, `email.mta_sts` → `_dmarc.example.com TXT`, `_mta-sts.example.com TXT` | green | crates/web/tests/explore_email.rs |
| C6 | S: `http.headers` → none; HTTP view link `/inspect/http?url=https%3A%2F%2Fexample.com%2F` | green | crates/web/tests/explore_http_ip.rs |
| C7 | S: TLS view with a two-row `tls.addresses` and a `tls.chain` → exactly three links | green | crates/web/tests/explore_tls.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 2 | sonnet | 36311 | 47 |
| G1 (review fix) | 1 | sonnet | 24711 | 28 |

### Review

Pass 1: BLOCKER null MX row linked `h=:25` (fixed: skipped); AMENDMENT empty `dns.<type>` block got no link and requirement 8 asks for one (repaired: `<domain> <TYPE>`); DEFERRED a CNAME chain dropped the type (repaired with the amendment); NIT real owners are FQDNs (accepted by the parser). Pass 2: AMENDMENT a Unicode IDN MX host yields a link the TLS inspector refuses (repaired: non-ASCII hosts get none, decision logged); NIT staged blob stale (restaged). Requirement 8 amended in the spec.

### Behavioural verification

skipped: Phase 3 adds a pure link-derivation module; Phase 4 renders the links on the evidence routes.

### Verify

`adlc verify`: verified.

## Phase 4 — Evidence view routes

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R6: five literal routes `/{domain}/{dns,tls,http,email,ip}` and `/r/{id}/{segment}` on netray-web's router (malformed `404`, entry `200`, live V1 snapshot ID `404`, else `410`, failed lookup `500`); `entries::router` narrowed to `/r/{id}/{segment}/{*rest}`; mounted by lens only with `entries::router`; no run, no store write | green | crates/web/tests/evidence_routes.rs, entries_router.rs (c16 rows) |
| C2 | S: one `example.com` entry → `GET /example.com/tls` `200` with its TLS view | green | crates/web/tests/evidence_latest.rs |
| C3 | S: `GET /Example.COM./tls` → the same view as `/example.com/tls` | green | crates/web/tests/evidence_latest.rs |
| C4 | S: no entry → `GET /example.com/dns` `404` linking `/example.com` | green | crates/web/tests/evidence_latest_missing.rs |
| C5 | S: `/r/<id>/email` `200`; well-formed unknown ID `410`; `/r/abc/tls` `404`; failing store `500` | green | crates/web/tests/evidence_entry.rs |
| C6 | S: `/r/<id>/smtp` `404` with v2-pages' page; lens merged, history on: `/example.com/smtp` → SPA shell | green | crates/web/tests/evidence_unknown_segment.rs, crates/lens/tests/evidence_spa_fallthrough.rs |
| C7 | S: lens merged, history on: `/assets/index-<hash>.js` served by the SPA handler with its type and immutable cache | green | crates/lens/tests/evidence_assets.rs |
| C8 | S: lens merged: `/r/<id>`, `/r/<id>/og.png`, `/r/<id>/tls` each answered by its handler, no route conflict; `/r/<id>/tls/x` `404` | green | crates/lens/tests/evidence_mount.rs |
| C9 | S: lens history off, snapshots on: `/r/<snapshot id>/tls` answers as before (`snapshot_router`'s `404`) | already_implemented | crates/lens/tests/evidence_snapshots_only.rs |
| C10 | S: any evidence route request leaves entry count and every `last_seen_at` unchanged | green | crates/web/tests/evidence_readonly.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 54777 | 110 |
| G2 | 1 | sonnet | 27537 | 51 |
| review fix | 1 | sonnet | 38837 | 73 |

### Review

Pass 1: BLOCKER non-UTF-8 path segments answered axum's `400` (fixed: `PathRejection` → 404 page); BLOCKER a failed V1 lookup answered `404` (fixed: 5xx → `500`); BLOCKER the domain segment used the `?d=` normaliser, not lens's `validate_domain` rule (fixed: rule moved to `netray_common::domain::validate`, lens wraps it); BLOCKER the no-entry link put a raw domain in `href` (`/\evil.com`; fixed: percent-encoded); AMENDMENT `/r/{id}/{segment}/` reached the SPA (repaired in phase: route added, requirement 6 amended). Regression tests: `crates/web/tests/evidence_review.rs`. Pass 2: no BLOCKER, no AMENDMENT; DEFERRED the no-entry page's `/<domain>` link reaches the V1 SPA until v2-result-page serves `/{domain}` (requirement 6 names the target).

### Behavioural verification

`target/debug/netray lens` with `crates/lens/lens.dev.toml`, an empty temporary `history.db` (history on), on 127.0.0.1:18431; `curl -s -w '%{http_code}'` per path:

```
/example.com/tls 404 text/html; charset=utf-8 | <h1>No entry for example.com</h1> href="/example.com"
/Example.COM./dns 404 text/html; charset=utf-8 | <h1>No entry for example.com</h1> href="/example.com"
/r/AAAAAAAA/tls 410 text/html; charset=utf-8 | <h1>This entry has expired</h1>
/r/abc/tls 404 text/html; charset=utf-8 | <h1>Nothing here</h1>
/r/AAAAAAAA/smtp 404 text/html; charset=utf-8 | <h1>Nothing here</h1>
/example.com/smtp 200 text/html; charset=utf-8 | <div id="root">   (SPA shell)
/r/AAAAAAAA/tls/x 404 text/html; charset=utf-8 | <h1>Snapshot not found</h1>   (narrowed entries catch-all)
```

A `200` view of a stored entry is covered by `evidence_latest.rs`, `evidence_entry.rs` and `evidence_mount.rs`; the seed that puts an entry into a running stack is Phase 8.
