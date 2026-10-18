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

## Phase 5 — Inspector frame

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R9: `GET /inspect/{dns,tls,http,ip}` renders the page on `base.html` with a `query_input` field `q`/`h`/`url`/`ip`, prefilled from the query, an empty result region, `data-island="stream"`, `data-stream-url="/inspect/<tool>/run"`, `data-stream-autostart="load"` when prefilled, a no-JavaScript notice; a GET starts no job; `/inspect/email` `404` | green | crates/web/tests/inspect_page.rs |
| C2 | R10: `POST /inspect/<tool>/run` streams `fragment` events then `end`: run line first with parts pending, then each part's blocks, then the band (the band: Phase 6, Decision log); invalid input → one fragment with the job function's message, then `end`; `429` with `Retry-After` and one fragment; prefetch → `503` `no-store` | green | crates/web/tests/inspect_stream.rs |
| C3 | R11: every inspector run renders `run_line` "live · not recorded" and no `grade`; no job writes to the store | green | crates/web/tests/inspect_no_store.rs |
| C4 | R12: a host-name target links `/<registrable domain>` by the PSL (one workspace PSL crate, licence accepted by `cargo deny`); an address or a name without a registrable domain gets none | green | crates/web/tests/inspect_backlink.rs |
| C5 | R13: the inspector routes reach jobs only through `InspectorJobs`; router tests run offline with a stub; the routes name no module crate | green | crates/web/tests/inspect_seam.rs |
| C6 | R14: `[inspect.<dns, tls, http, ip>] deadline_ms, max_concurrent`, `deny_unknown_fields`, defaults 30000/15000/30000/5000 ms and 8; zero is a startup reject; full → "busy, try again in a moment", no queue; past the deadline finished parts stay, others "timed out"; page route and v1 share one limiter instance (the `admit` hook here; the shared instance: Phase 6, Decision log) | green | crates/web/tests/inspect_config.rs, inspect_limits.rs |
| C7 | S: `GET /inspect/tls?h=example.com:443` → prefilled, `data-stream-autostart="load"`, empty result region, no job recorded | green | crates/web/tests/inspect_page.rs |
| C8 | S: `POST /inspect/tls/run`, stub with two blocks → first fragment "live · not recorded", later fragments both blocks, `end`, no `grade` element | green | crates/web/tests/inspect_stream.rs |
| C9 | S: `POST /inspect/http/run` with `Sec-Purpose: prerender` → `503`, `Cache-Control: no-store`, no job | green | crates/web/tests/inspect_prefetch.rs |
| C10 | S: `max_concurrent = 1`, one job in flight → second post answers "busy" at once | green | crates/web/tests/inspect_limits.rs |
| C11 | S: stub limiter refusing with 20 s → `429`, `Retry-After: 20`, one fragment, no job | green | crates/web/tests/inspect_refusal.rs |
| C12 | S: `deadline_ms = 100`, parts at 50 ms and 500 ms → first shown, second "timed out" | green | crates/web/tests/inspect_limits.rs |
| C13 | S: `[inspect.dns] max_concurrent = 0` → lens exits non-zero naming the key; unknown key under `[inspect.tls]` → `--check-config` exits 1 naming it | green | tests/repo/test_check_config.sh |
| C14 | S: target `www.example.com` → page links `/example.com`; `192.0.2.1` → no link | green | crates/web/tests/inspect_backlink.rs |
| C15 | S: a store and a stub job → entry count and every `last_seen_at` unchanged | green | crates/web/tests/inspect_no_store.rs |
| C16 | S: speculation rules parsed → `/inspect/dns` matches, `/example.com/tls` does not | green | crates/web/tests/inspect_speculation.rs, pages_csp.rs |
| C17 | S: `GET /inspect/email` → `404` | green | crates/web/tests/inspect_page.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 74305 | 176 |
| G2 | 1 | sonnet | 31976 | 218 |
| review fix 1 | 1 | sonnet | 45343 | 71 |
| review fix 2 | 1 | sonnet | 29157 | 38 |

### Review

Pass 1: BLOCKER the back-link came only from the GET prefill (fixed: `#inspect-backlink` re-sent as a fragment by every started job, decision logged); BLOCKER the HTTP authority was split by hand (`https://bank.example.com\@evil.example/` linked `/evil.example`; fixed: `url::Url::parse` as the job parses); BLOCKER the TLS input was split before percent-decoding (fixed: the TLS input order); AMENDMENT the band (requirement 10) and the shared limiter instance (requirement 14) need Phase 6 (spec amended: Phase 6 carries both, decision logged); DEFERRED `registrable_domain` case-sensitive (fixed in phase); DEFERRED `InspectDeps::new` trusts no proxy, so Phase 6 must hand it v1's extractor; DEFERRED `inspect_no_store.rs`'s store half cannot fail until a real `InspectorJobs` runs (Phase 6). Regression tests: `crates/web/tests/inspect_review.rs`. Pass 2: the four fixes hold; AMENDMENT a non-host target (`q=https://example.com`, `h=www.example.com/`) still got a back-link (repaired in phase: a host holds only letters, digits, `-`, `_`, `.`); AMENDMENT the stream form had no `[data-stream-error]` element (frontend rules §5; repaired in phase: `query_input` writes it hidden inside a stream form); DEFERRED as pass 1 on `inspect_no_store.rs`; NIT a refused or invalid run keeps the previous back-link (one fragment by requirement 10). Regression tests: `crates/web/tests/inspect_review_second.rs`.

### Behavioural verification

The routes are mounted in lens in Phase 6 (with the real `InspectorJobs`); the router itself is exercised offline by the `inspect_*` tests. The config entry point, `target/debug/netray lens --check-config` on `crates/lens/lens.dev.toml` plus one appended table:

```
== [inspect.tls] max_concurrent = 2
config ok: …/c.toml                                                     exit 0
== [inspect.dns] max_concurrent = 0
config error: …/c.toml: inspect.dns.max_concurrent must be at least 1   exit 1
== [inspect.tls] bogus = 1
config error: …/c.toml: unknown field `bogus`, expected `deadline_ms` or `max_concurrent` for key `inspect.tls`   exit 1
```

## Phase 6 — TLS, HTTP and IP inspectors

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R15: the TLS inspector accepts tlsight's inputs (`host`, `host:port`, `host:port,port`, addresses), keeps SMTP STARTTLS on 25 and 587, refuses 143 and 21 with "STARTTLS for IMAP and FTP is not supported" before the job; blocks and band are `netray_tls::module::translate` over the job's `InspectResponse` | green | crates/web/tests/inspect_tls_jobs.rs |
| C2 | R16: the HTTP inspector accepts any `http`/`https` URL, path included, and renders the blocks of `netray_http::translate`; its band is that translation's checks | green | crates/web/tests/inspect_http_jobs.rs |
| C3 | R17: the IP inspector without `ip` looks up the caller's address (v1's key), with `ip` any global address; blocks and band from `netray_ip::translate(&[(addr, result)], 1)`; without `[modules.ip]` one fragment "IP lookups are unavailable" | green | crates/web/tests/inspect_ip_jobs.rs |
| C4 | R25: a protocol's band is `spectrum::layout` over `Catalogue::for_protocol(p)` (catalogue order) with the one live run under `Profiles::current()`, no older run, rendered through `SpectrumView` | green | crates/web/tests/inspect_band_layout.rs |
| C5 | R10 (carried from Phase 5): the band streams after the parts and before `end` | green | crates/web/tests/inspect_band.rs |
| C6 | R14 (carried from Phase 5): the page route and `/api/v1/<tool>` share one instance of each inspector's per-client and per-target limiters, keyed by v1's caller address | green | crates/web/tests/inspect_shared_limits.rs |
| C7 | R13 (carried from Phase 5): lens mounts the inspector routes with the real `InspectorJobs` and v1's `IpExtractor` ahead of `spa::handler` | green | crates/lens/tests/inspect_mount.rs |
| C8 | R11 (carried from Phase 5): a real job run against a store leaves its entry count and every `last_seen_at` unchanged | green | crates/web/tests/inspect_no_store_real.rs |
| C9 | S: `h=mail.example.com:143` → the stream holds "STARTTLS for IMAP and FTP is not supported" and the job function is not called | green | crates/web/tests/inspect_tls_jobs.rs |
| C10 | S: a TLS job yielding one `tls.chain` block and a failing `tls.not_expired` → the chain renders and the band carries a fail line for `tls.not_expired` | green | crates/web/tests/inspect_band.rs |
| C11 | S: `url=https://example.com/path?x=1` → the job receives that URL unchanged | green | crates/web/tests/inspect_http_jobs.rs |
| C12 | S: `/inspect/ip` from a trusted proxy with `X-Forwarded-For: 192.0.2.7` and no `ip` → the job looks up `192.0.2.7` | green | crates/web/tests/inspect_ip_jobs.rs |
| C13 | S: `ip=2001:db8::1` → the job looks up `2001:db8::1` and the band is the IP band | green | crates/web/tests/inspect_ip_jobs.rs |
| C14 | S: lens without `[modules.ip]` → an IP job answers one fragment "IP lookups are unavailable" | green | crates/web/tests/inspect_ip_jobs.rs |
| C15 | S: `/api/v1/tls/inspect` exhausting the per-target budget of `example.com` → `POST /inspect/tls/run` with `h=example.com` answers `429` | green | crates/web/tests/inspect_shared_limits.rs |
| C16 | S: `Catalogue::for_protocol(Tls)` holds exactly the TLS checks in catalogue order, and `layout` over it yields one line per TLS check | green | crates/engine/tests/catalogue_for_protocol.rs, crates/web/tests/inspect_band_layout.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 4 | sonnet | 52542 | 133 |
| G2 | 1 | sonnet | 65122 | 132 |
| G3 | 1 | sonnet | 28286 | 42 |

### Review

Pass 1: BLOCKER a TLS page job could pass admission and be refused inside the 200 stream ("rate limited (api_v1_tls)", a limiter scope in UI copy), because admission charged one unit and the inspection charged ports-by-addresses again (fixed: admission charges one unit per port on the shared `[api.tls]` instance, the inspection runs against a per-job limiter with `[api.tls]`'s burst, a late `RateLimited` renders "too many requests"; decision logged); AMENDMENT a "busy" answer charged the shared limiters (repaired in phase: the slot is taken before admission, decision logged); AMENDMENT IP admission charges the caller before validating (repaired in phase as a decision: v1's order kept); DEFERRED the DNS placeholder until Phase 7; NIT a part timed out with a band that still renders when the job lands in the same instant; NIT the TLS page job logs an empty request id. Regression tests: `crates/web/tests/inspect_review_phase6.rs`. Pass 2: no blocker, the fixes hold; DEFERRED a port count above `[api.tls]`'s burst is refused with `Retry-After: 60` on every retry, as tlsight V1 refuses it (unreachable with the defaults, burst 10/20 over `max_ports` 7); NIT the per-job limiter's cap path counts a `rate_limit_hits_total{scope="inspect_tls"}` without refusing.

### Behavioural verification

`target/debug/netray lens` on `crates/lens/lens.dev.toml` (no `[modules.ip]`), port 18431:

```
== GET /inspect/tls?h=example.com
HTTP/1.1 200 OK
<form class="query-input" action="/inspect/tls" method="get" data-island="stream" data-stream-url="/inspect/tls/run" data-stream-autostart="load">
<div id="inspect-result"></div>
== POST /inspect/tls/run h=mail.example.com:143
HTTP/1.1 200 OK   content-type: text/event-stream
event: fragment
data: {"id":"inspect-result","html":"<div id=\"inspect-result\"><p>STARTTLS for IMAP and FTP is not supported</p></div>"}
== POST /inspect/ip/run ip=
event: fragment
data: {"id":"inspect-result","html":"<div id=\"inspect-result\"><p>IP lookups are unavailable</p></div>"}
event: end
== POST /inspect/http/run url=https://example.com/
event: fragment  inspect-result: run line "live · … not recorded", part pending
event: fragment  inspect-backlink: <a href="/example.com">Back to example.com</a>
event: fragment  inspect-part-0: <section class="evidence-block" id="ev-http.redirects" data-shape="chain"> …
event: fragment  inspect-band: <div class="spectrum"> … min-width: calc(12 * var(--spectrum-line-pitch-min)) (the 12 HTTP checks)
```

## Phase 7 — DNS inspector

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R18: the DNS inspector offers query, trace (`+trace`), resolver compare (`+compare`), authoritative compare (`+auth`) and DNSSEC chain (`+dnssec`), parsed by `crates/dns/src/parser.rs`, run through v2-api's DNS jobs; `netray-dns` maps each job's collected object to blocks (trace and DNSSEC `tree`, compares `table`, query `records`, ids under `dns.`); `+check` is not a job and the page says the band covers it | green | crates/dns/tests/job_blocks.rs, crates/web/tests/inspect_dns_jobs.rs |
| C2 | R19: for every DNS job the DNS module runs live on the registrable domain, concurrently and under the same deadline, its checks the DNS band; without a registrable domain the band region says so | green | crates/web/tests/inspect_dns_band.rs |
| C3 | R20: a `dns` island builds `q` from record types, resolver and job and writes it into the form; no layout, no verdict, no style writes; vitest tests in the gate; bundle under the 30 KB gzip cap | green | crates/web/islands/test/dns.test.ts, crates/web/tests/inspect_dns_page.rs |
| C4 | R21: this feature's templates, stylesheets and island pass the gate of §8.5 and the codename check, containing neither "lens" nor a codename | green | crates/web/tests/inspect_gate.rs |
| C5 | R22: `specs/sdd/v2.md` amended: §6.1/§6.2 cite `crates/tls/src/tls/connect.rs` (and `:201`), not `crates/tlsight/…`; §6.1 states the run line's timestamp, the `ev-<block id>` anchors and the explore-live derivation; §6.2 names the job route, the limits, the band restriction and `/inspect/<tool>` on netray-web's router before v2-cutover; the Phase 4 table marks V4.7 built by `v2-inspectors` | green | tests/repo/test_sdd_v2_inspectors.sh |
| C6 | S: `q=example.com A +trace` → the trace job runs and renders a `tree` block | green | crates/web/tests/inspect_dns_jobs.rs |
| C7 | S: `q=_dmarc.example.com TXT` → the query job runs and the DNS band is computed for `example.com` | green | crates/web/tests/inspect_dns_band.rs |
| C8 | S: `q=example.com +compare` and `+auth` → each renders a `table` block | green | crates/web/tests/inspect_dns_jobs.rs |
| C9 | S: `q=com NS` → the band region says there is no registrable domain and no DNS module run happens | green | crates/web/tests/inspect_dns_band.rs |
| C10 | S: `q=example.com +check` → the page says the band covers the check and no `+check` job runs | green | crates/web/tests/inspect_dns_page.rs, crates/web/tests/inspect_dns_jobs.rs |
| C11 | S: the `dns` island with types `A`, `MX` and `+dnssec` → `q` reads `example.com A MX +dnssec` | green | crates/web/islands/test/dns.test.ts |
| C12 | S: the crate's templates, stylesheets and islands → the gate and the codename check both pass | green | crates/web/tests/inspect_gate.rs |
| C13 | Operator 2026-10-18, Rule M: every record type whose lookups failed shows "not measured" with its reason; a partly failed query shows both; an error event after a block is kept; authcompare renders TXT one way | green | crates/dns/tests/job_blocks_rule_m.rs, crates/web/tests/inspect_dns_rule_m.rs |
| C14 | Operator 2026-10-18: the DNS band runs under the job's admission, no module limiter inside the stream | green | crates/dns/tests/module_admitted.rs |
| C15 | Third review: offered types in zone-file form, authcompare rows rendered alike, an authoritative REFUSED a failed row, summary warnings shown, the band under the route's deadline | green | crates/dns/tests/job_blocks_rendering.rs, crates/web/tests/inspect_dns_review3.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 73478 | 146 |
| G2 | 2 | sonnet | 80457 | 181 |
| G3 | 2 | sonnet | 34925 | 51 |
| G4 | 2 | sonnet | 46270 | 50 |

### Review

Pass 1: BLOCKERs — several DNS jobs per request charged and ran the earlier ones when a later one was refused (fixed: one job per request); an address target got a band on the PSL's `2.1` (fixed); a failed job rendered an empty part (fixed for error events); compare/authcompare rows of failed lookups read as NODATA and their key columns repeated (fixed: `lookup` key, `outcome` cell); trace and DNSSEC nodes published a 0 ms latency placeholder (fixed); the island wrote picked types into an empty `q` (fixed). AMENDMENT the band spent the registry DNS module's per-target budget (fixed: its own `DnsModule`). Regression tests: `crates/dns/tests/job_blocks_review.rs`, `crates/web/tests/inspect_dns_review.rs`, `crates/web/islands/test/dns-review.test.ts`. Deferred: an authoritative REFUSED/SERVFAIL arrives as an empty `AuthResult::Response` (v2-api's event type has no rcode).
Pass 2: BLOCKER (surviving pass 1's Rule M finding) a query whose lookups all fail without an error event (SERVFAIL, timeout: non-definite `LookupResult`s inside the batches) still yields no block and an `Ok` empty part, and a partly failed query drops the failed type silently; AMENDMENT error events are dropped whenever the job yielded a block (`+auth` with no NS); AMENDMENT authcompare renders TXT differently for authoritative (hickory Display) and recursive (`rdata_text`) rows; AMENDMENT the band's own `DnsModule` per-target limiter (keyed by the shared resolver, 19 per run) refuses inside the 200 stream, which SDD §6.2 and requirement 10 forbid. Halted: blocker surviving two passes (handoff `specs/handoff/feature-v2-inspectors.md`).
Operator decision (2026-10-18): Rule M per failed type and the band under the job's admission; fixed with the two amendments, tests `96e460c2` (C13, C14): `query_blocks` emits a "not measured" `fields` block per failed type, `dns_part` keeps error events in a `dns.errors` table and never streams an empty part, `AuthRecord.presentation` (serde-skipped) carries the zone-file form, the band's `DnsModule` is `admitted_by_caller()`.
Pass 3 (over the fix): BLOCKERs — SRV/TLSA/SSHFP/NAPTR/HINFO data rendered as Rust Debug; authoritative rows rendered HTTPS/SRV/CAA unlike recursive rows; an authoritative REFUSED/SERVFAIL read "no records"; refused NS addresses vanished without a note; the band's deadline a fixed 30 s instead of `[inspect.dns] deadline_ms`. All fixed (tests `4d7672f7`, C15). AMENDMENTs — summary warnings dropped (fixed: `dns.warnings`); failed authoritative queries only in `dns.errors` (kept, logged). NITs — the behavioural transcript predated one job per request (re-run below); duplicate authcompare keys when two NS share an address (`build_server_list`, v2-api's job; deferred).

### Behavioural verification

`target/debug/netray lens` on `crates/lens/lens.dev.toml` (ports 18431/19431), live network, after the third review's fix:

```
== POST /inspect/dns/run q=dnssec-failed.org A
fragment inspect-result / inspect-backlink
fragment inspect-part-0   ev-dns.a (fields): "not measured", reason SERVFAIL
fragment inspect-band     <div class="spectrum"> …
end
== POST /inspect/dns/run q=example.com MX +trace
fragment inspect-part-0   ev-dns.trace (tree)          (one job per request: the trace only)
fragment inspect-band     <div class="spectrum"> …
end
== POST /inspect/dns/run q=example.com A @nosuchserver
fragment inspect-part-0   ev-dns.a (records), ev-dns.warnings (table): "unrecognized server: @nosuchserver"
end
== POST /inspect/dns/run q=nons.invalid-netray-test.com +auth
fragment inspect-part-0   ev-dns.authcompare (table), ev-dns.errors (table): "could not discover authoritative nameservers", ev-dns.warnings (table)
end
== GET /inspect/dns  → 200
```

## Phase 8 — Behavioural verification with screenshots

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R23: `crates/store/examples/seed.rs` (`cargo run -p netray-store --example seed -- <db> <dir>`) migrates and records every fixture of `tests/acceptance/fixtures/history/` through `HistoryStore::record` with the model's diff at `seed_now - age_seconds`, sorted by file name; fixture `{"domain","age_seconds","run"}`; refuses extra top-level keys and non-decreasing ages per domain before any record; `just acceptance-local` seeds lens's `history.db`; one `example.com` fixture (age 0, records/table/chain/fields/text blocks, findings, evidence refs); a Rust test holds every fixture run to the catalogue, each ID once | green | crates/store/tests/seed.rs, crates/engine/tests/history_fixtures.rs, tests/repo/test_acceptance_seed.sh |
| C2 | R24: `tests/acceptance/v2/inspectors.spec.ts` (project `local`) opens every evidence view and inspector at 1280/390 px, light/dark; asserts run line, check-row order, shared-evidence line, explore-live hrefs, no page scroll at 390 px outside the band, `/inspect/email` 404; screenshots per view/width/theme; `@live` runs one job per inspector | green | tests/acceptance/v2/inspectors.spec.ts |
| C3 | S: `example.com.json` (age 0) seeded at `seed_now` → one entry, `created_at` = `last_seen_at` = `seed_now` | green | crates/store/tests/seed.rs |
| C4 | S: `a.json` age 600, `b.json` age 0, one domain → recorded in that order at `seed_now - 600` and `seed_now` | green | crates/store/tests/seed.rs |
| C5 | S: `a.json` age 0, `b.json` age 600 (or 0) → non-zero, names both files, no entry | green | crates/store/tests/seed.rs |
| C6 | S: a fixture with `"created_at"` → non-zero, names the file, no entry | green | crates/store/tests/seed.rs |
| C7 | S: `/example.com/tls` at 390 px dark → run line names the entry, no page scroll, screenshot | green | tests/acceptance/v2/inspectors.spec.ts |
| C8 | S: `/inspect/{dns,tls,http,ip}` at 1280 px light → form renders, screenshot | green | tests/acceptance/v2/inspectors.spec.ts |
| C9 | S: `/example.com/email` → `email.mx` row's explore-live href is `/inspect/tls?h=<host>:25` | green | tests/acceptance/v2/inspectors.spec.ts |
| C10 | S: `@live` `/inspect/tls?h=netray.info` with JS → streams, "live · not recorded", band, no grade, entry count unchanged | green | tests/acceptance/v2/inspectors.spec.ts |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 (seed, fixture, recipe, `local` project) | 2 | sonnet | 42008 | 55 |
| G2 (evidence blocks fit 390 px) | 1 + 1 (follow-up: tables scroll inside the block instead of breaking short tokens) | sonnet | 51527 | 84 |
| review BLOCKER (incomplete run) | 1 | sonnet | 25696 | 14 |

The fixture `tests/acceptance/fixtures/history/example.com.json` is a live lens run of example.com
(2026-10-18) with documentation addresses and example.com names in place of the provider's, plus
an MX host, MTA-STS (fields and the policy text block) and an `ip` section (`ip.addresses`,
`ip.reputation`) added: the live domain has a null MX, so no text block and no `email.mx` link, and a
checkout has no GeoLite2 data, so no ip section (implementer under the operator's autonomy mandate, 2026-10-18).

### Behavioural verification

```
$ cargo run -q -p netray-store --example seed -- $T/history.db tests/acceptance/fixtures/history
seeded 1 fixture(s) into $T/history.db
exit=0
$ sqlite3 $T/history.db "select domain, created_at=last_seen_at, count(*) from entries"
example.com|1|1
$ cargo run -q -p netray-store --example seed -- $T/bad.db $T/bad   # a.json carries created_at
seed fixture: a.json: unknown top-level key `created_at` (allowed: domain, age_seconds, run)
exit=1
$ just acceptance-local tests/acceptance/v2/inspectors.spec.ts
  43 passed (6.5s)
$ just acceptance-local tests/acceptance/v2/inspectors.spec.ts --grep @live
  4 passed, 1 failed: live job /inspect/ip — a checkout has no GeoLite2 data, so lens runs without
  [modules.ip] and the ip job answers that IP lookup is off; C10 (/inspect/tls?h=netray.info: streams,
  "live · not recorded", band, no grade, entry counts of example.com and netray.info unchanged) passed
```

Before the fix the offline set failed at 390 px: `/example.com/dns` scrollWidth 942 and
`/example.com/tls` 941 (DNSKEY and HTTPS record data, the TLS addresses table); evidence blocks now
scroll a too-wide table inside the block and wrap field and chain values
(`crates/web/static/evidence_block.css`).

Screenshots (gitignored, not committed), in `tests/acceptance/test-results/v2-inspectors/`:

| file | shows |
|---|---|
| `example.com-<dns,tls,http,email,ip>-<1280,390>-<light,dark>.png` (20) | the evidence view of the seeded entry: run line, the protocol's check rows in catalogue order with their evidence links, shared-evidence line, the protocol's blocks, explore-live links |
| `inspect-<dns,tls,http,ip>-<1280,390>-<light,dark>.png` (16) | the inspector form before a job: target field, Run button, empty result; DNS adds the query builder |
| `live-inspect-<tls,dns,http>-<1280-light,390-dark>.png` (6, taken by hand from a local lens) | a live job's result: "live · not recorded", the job's blocks and the protocol's band, no grade |

### Review

| class | finding | outcome |
|---|---|---|
| BLOCKER | `seed` counted a fixture with an incomplete run as seeded while `record` stored nothing | fixed: the seed refuses it naming the file (test `incomplete_run_fails_naming_the_file_and_stores_nothing`) |
| AMENDMENT | seeded fingerprints and diffs use no volatile paths and no rescoring, unlike lens | decision log: kept as requirement 23 prescribes; fixtures needing lens's exact form say so (affected_phase 8, repaired_in_phase yes) |
| AMENDMENT | the `@live` ip job cannot pass without GeoLite2 data in the local stack | decision log: test kept for a stack with the data; run recorded above (affected_phase 8, repaired_in_phase yes) |
| DEFERRED | the age-order guard does not compare against a store that already holds the domain; `record` clamps silently | v2-cutover, which runs the seed for `serve` into a possibly non-empty store |
