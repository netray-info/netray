# Spec: v2-inspectors

Status: Ready for Implementation
Created: 2026-10-17

## Goal

`netray-web` renders every evidence block of a stored run through one function over the six shapes of `netray-model` into the one `evidence_block` macro, serves the evidence view of a protocol at `GET /<domain>/<protocol>` (latest entry) and `GET /r/<id>/<protocol>` (entry `<id>`) in §6.1's order with shared evidence shown once and an explore-live link derived from each block, and serves the DNS, TLS, HTTP and IP inspectors at `GET /inspect/<tool>`, whose jobs start only by `POST /inspect/<tool>/run`, stream as HTML fragments, show the protocol's live band, write nothing to `history.db`, and carry the run line "live · not recorded" and no grade. This is SDD criterion V4.7 (`specs/sdd/v2.md`, Phase 4 table); a Playwright pass on the local stack screenshots every view and inspector.

## Non-goals

- Subdomain routing (`dns.`, `tls.`, `http.`, `ip.` by `Host`), the redirect map of §7.3 and deleting the V1 SPAs (`crates/{dns,tls,http,email,ip}/frontend`): v2-cutover (V5.1, V5.3).
- An email inspector page: email has none (S3); `POST /api/v1/email/inspect` belongs to v2-api.
- `ip.netray.info`'s text answer and its cut routes (§7.2): v2-api and v2-cutover.
- The result page, its stream, open issues and the history table (V4.5, V4.6, V4.13): v2-result-page. This feature hands it the evidence href and the section renderer (I8).
- API v1 endpoints and the inspector job functions themselves: v2-api (I1, I2).
- `base.html`, the CSP header, the islands bundle and the `stream` island: v2-pages (I4, I5).
- `spectrum::layout`: v2-spectrum (I6).
- `RunCheck.findings` and `RunCheck.evidence`: v2-api Phase 1 (I3, X2).
- prism's shared-result key `?r=` (ignored, §7.3), `+check` as a job (the band replaces it, S29), STARTTLS for IMAP and FTP (stubs, `crates/tls/src/tls/connect.rs:201`).
- Inspector metrics: §9.2 lists none.
- A release; the build runs towards a local 0.30.0 (operator, 2026-10-17).

## Context and constraints

- SDD `specs/sdd/v2.md`: §6.1 (evidence views, R27–R30), §6.2 (inspectors, R31, S3), §3.7 (shapes, typed values, block IDs, "no module ships templates"), §8.3 (`run_line`, `check_row`, `evidence_block`, `spectrum`, `query_input` are used by evidence views and inspectors), §8.4 (islands: `stream`, `dns`; 30 KB gzipped cap; vitest), S22 (five DNS jobs, a third island allowed), S29 (DNS band of the registrable domain, Public Suffix List), S8 (no run for `Sec-Purpose: prefetch`/`prerender`; idle inspector pages prerendered), S25 (no `unsafe-eval`, no inline script), §7.3 (inspectors keep `?q=`, `?h=`, `?url=`, `?ip=`).
- Build order (one order in every spec): lane A `v2-history` → `v2-admission` → `v2-entries` → `v2-api`; lane B `v2-spectrum` ∥ `v2-pages`, parallel to lane A; lane C, after the operator's UI review at midday 2026-10-18, this feature first (its Phase 2 before `v2-result-page` Phase 1), then `v2-result-page`; then `v2-cutover`. Lanes A and B are merged when this feature starts.
- `netray-web` today (`crates/web/Cargo.toml`) depends on `netray-common`, askama and serde_json only (`crates/web/Cargo.toml:9-11`); §3.1 lists `engine`, `store`, `model` and the module crates for it. Its `EvidenceShape` view (`crates/web/src/components.rs:74-85`) is string-typed (`RecordRow.ttl: u32`), while `netray_model::evidence::Shape` (`crates/model/src/evidence.rs:277-289`) carries tagged `Value`s (`text`, `int`, `bool`, `time`, `duration` in ms, `addr`, `:144-152`), a table key column, and a `Tree(Node)` of nested fields (`:269-273`).
- The `evidence_block` macro renders tree depth as an inline `style="--evidence-block-depth: …"` attribute (`templates/components/evidence_block.html`, tree branch). v2-pages keeps `style-src 'self' 'unsafe-inline'` for the 4a components (its requirement 11), so the attribute is allowed, not refused.
- The stored `RunCheck` holds only `id` and `status` (`crates/model/src/run.rs:40-45`); live `CheckResult` carries `findings: Vec<String>` and `evidence: Vec<BlockId>` (`crates/model/src/lib.rs:167-174`). v2-api Phase 1 adds both to `RunCheck` (X2, I3).
- Catalogue texts (`title`, `short`, `finding`, `fix_hint`, `guide`) come from `netray_engine::catalogue()` (`crates/engine/src/catalogue.rs:158`), the only product copy of a check (R37). `Catalogue` is built only by `from_toml` (`:133`) and exposes `entries()` and `get()` (`:148-155`).
- Block owners follow the block ID prefix (`BlockId::module()`, `crates/model/src/evidence.rs:89`). Registered blocks: DNS `dns.<type>` records (`crates/dns/src/module.rs:64`); `tls.addresses` (column `address`), `tls.chain`, `tls.handshake`, `tls.tlsa`, `tls.ct`, `tls.caa`; `http.redirects`, `http.port80` (chain), `http.port80_probe` (fields), `http.headers`, `http.server`, `http.cors`, `http.cookies` (`crates/http/src/module.rs:52-60`); `email.mx` (column `host`), `email.dmarc`, `email.mta_sts` and twelve more; `ip.addresses` (column `address`).
- Each module crate already translates its V1 response into a section: `netray_tls::module::translate(&InspectResponse)` (`crates/tls/src/module.rs:270`, port 443 or the first port), `netray_http::module::translate(&InspectResponse)` (`crates/http/src/module.rs:228`), `netray_ip::module::translate(&[(IpAddr, Result<Ifconfig, String>)], total)` (`crates/ip/src/module.rs:178`), each `-> SectionOutcome` (`crates/engine/src/lib.rs:117`: four variants, `crates/engine/src/lib.rs:117-129`: `Measured { checks, evidence }`, `NotApplicable { reason }`, `Incomplete { reason }`, `TimedOut`). DNS has `translate(&[CheckEvent])` for its checks (`crates/dns/src/module.rs:595`) and no translation of a trace, compare or DNSSEC job into blocks.
- Inspector inputs today: TLS `host`, `host:port`, `host:port,port`, IPv4/IPv6 with optional port (`crates/tls/src/input.rs:29-35`), STARTTLS chosen by port inside `tls_handshake` (`crates/tls/src/tls/connect.rs:91`, `:188-201`): SMTP on 25 and 587, IMAP 143 and FTP 21 stubs; DNS a dig-like query language with `+trace`, `+dnssec`, `+compare`, `+auth` (`crates/dns/src/parser.rs:140`, flags read in `crates/dns/frontend/src/App.tsx:69-90`); the V1 TLS app starts a job on load from `?h=` (`crates/tls/frontend/src/App.tsx:183-187`). The SDD cites `crates/tlsight/src/tls/connect.rs`, renamed by S9 to `crates/tls/`.
- V1 job budgets: DNS jobs under a 30 s stream deadline (`STREAM_TIMEOUT_SECS`, `crates/dns/src/api/mod.rs:34`, applied e.g. at `crates/dns/src/api/trace.rs:169`), with `max_timeout_secs` (default 10 s, `crates/dns/src/config.rs:334`) clamping each query or hop, not the job, TLS request 15 s (`crates/tls/src/config.rs:317`), HTTP total 30 s (`crates/http/src/config.rs:231`); TLS handshakes 10 concurrent (`:309`).
- No Public Suffix List crate is in `Cargo.lock`; S29 needs one. `just check` runs `cargo deny check bans licenses sources`.
- Client addresses: lens's `client_ip_from` (`crates/lens/src/routes.rs:704`) drops `CF-Connecting-IP` and applies `netray_common::ip_extract::IpExtractor` (trusted-proxy CIDRs, `crates/common/src/ip_extract.rs`); v2-api keys every per-client limit on it (its requirement 9).
- lens's router ends in `.fallback(spa::handler)` (`crates/lens/src/lib.rs:152`); the binary builds lens's module registry in `lens_registry` (`crates/netray/src/main.rs:143-238`) and calls `lens::run_with`. `tests/repo/test_lens_names_no_module.sh` refuses a normal dependency of lens on a module crate (direct dependencies only).
- `just acceptance-local` starts `netray site` and the six services on free ports, `LOCAL_<NAME>_URL`, lens with `LENS_HISTORY__DB_PATH` in its temp dir (`justfile:113-155`); default specs need no network. A locally built binary from a worktree has no outbound network (memory, `reference_new_binary_no_network`): live jobs in screenshots run from main's target path.
- Gate: `just adlc-verify`; frontends are type-checked with `npx tsc --noEmit`; new config keys need `deny_unknown_fields`, startup rejects a `validate()` rule and a `startup_rejects` row in `tests/repo/test_check_config.sh`; axum 0.8 refuses `/x/{*rest}` beside `/x/{id}` (CLAUDE.md). Amend `specs/sdd/v2.md` in the commit whose code changes what it says.

### Interfaces from the sibling features (drafts of 2026-10-17, decisions X1–X3)

Each is marked `(I<n>)` where a requirement uses it. If one is missing or differs when a phase starts, `implement` halts on it instead of building it here.

- **I1 (v2-api Phases 4, 5, 7). Job functions.** None returns `Block`s; each returns its V1 response type, and this feature gets checks and blocks from the module's translation above.
  - TLS: the axum-independent inspection type of `netray-tls`, built from `[api.tls]` (v2-api requirement 27), applying tlsight's input parsing, target policy with the rebinding re-check, cost limits and handshake semaphore, returning tlsight's `InspectResponse`. Its v1 body is `{host, port}` without `starttls`; SMTP STARTTLS on 25 and 587 still happens, because `tls_handshake` chooses it by port.
  - HTTP: `parse_url`, `validate_target` and `inspect_and_assemble` (`crates/http/src/inspect/mod.rs:149`, state-free), returning spectra's `InspectResponse` (v2-api requirement 28).
  - IP: `IpModule::lookup`, made public (v2-api requirement 19), over the lens process's `Arc<IpModule>` (`crates/netray/src/main.rs:204-248`, reloaded on SIGHUP); absent without `[modules.ip]`.
  - DNS: five functions in `netray-dns` for query, compare and authcompare (fan-out moved out of the handlers) and trace and dnssec (over `walk` and `walk_chain`), each with a typed collected object and `QueryPolicy::validate` (v2-api requirement 30).
- **I2 (v2-api requirements 8, 9, 25, 27–31; operator decision "api").** Each inspector's per-client and per-target limits and job settings live in `[api.<inspector>]` of the lens config, keyed by `client_ip_from`.
- **I3 (X2, v2-api Phase 1).** `RunCheck` gains `findings: Vec<String>` and `evidence: Vec<BlockId>`, additive, ignored by fingerprint and diff. Entries recorded before render check rows without findings and block links.
- **I4 (v2-pages; X1).** `templates/base.html` (title, meta description, canonical, `og:*`, `/static/app.css`, navigation, a `main` block, `/static/islands.js`, speculation rules; v2-pages requirement 9); `/static/app.css` concatenating every component stylesheet in R44 order, so `evidence_block.css` reaches every page (requirement 10); the CSP of requirement 11 (`connect-src 'self'`, `form-action 'self' https://*.netray.info`); the speculation-rules constant whose `/inspect/*` entry v2-pages leaves to V4.7 (requirement 12); the 404 page `templates/pages/not_found.html` (requirement 14); the islands package `crates/web/islands` mounting islands by `data-island` (requirements 4–7, gate rule 1 refusing style writes, cap 30 720 bytes); this feature's routes under `crates/web/src/pages/`, where `tests/repo/test_web_crate.sh` allows a `Router` beside `src/entries/` and `src/api/` (requirement 8). Per X1, lens mounts netray-web's routers ahead of `spa::handler` before the cutover, and the Phase 4a guards of `test_web_crate.sh` are lifted by the first feature that needs it.
- **I5 (X3, v2-pages requirements 28–29; contract as v2-result-page requirements 19–23 use it).** The `stream` island on a `<form data-island="stream" data-stream-url="…">` posts to the server, once on mount only when the form carries `data-stream-autostart="<trigger>"` (adding that value as the `trigger` field), reads SSE `fragment` events `{"id", "html"}` and replaces the element with that ID, ends on `end`, executes no script in inserted markup, does not post while `document.prerendering` and waits for `prerenderingchange`, and shows a refusal's one fragment (`429`, `Retry-After`, `data-retry-after`).
- **I6 (v2-spectrum requirements 10, 15).** `netray_web::spectrum::layout(current, older, catalogue, width)` over a `&Catalogue`, each run as `CheckId → Status` with its `&Profile`; `SpectrumView` from a layout and a run index, rendered at `n × linePitchMin`. It has no per-protocol entry point; requirement 25 adds the restriction this feature needs.
- **I7 (v2-history, v2-entries).** `HistoryStore::latest(&str)` and `HistoryStore::get(&str)` → `Result<Option<Entry>, StoreError>` (`crates/store/src/lib.rs:208`, `:214`), `Entry.created_at` in Unix seconds. `/r/<id>` precedence (v2-entries requirement 10): malformed ID (not `^[0-9A-Za-z]{8}$`) `404`, entry `200`, live V1 snapshot its V1 page, else `410`; a failed lookup `500`, never `410` (requirement 12). v2-entries' `entries::router` (`crates/web/src/entries/`, mounted by lens only when `[history] enabled`; otherwise lens's own V1 `snapshot_router`) routes `/r/{id}`, `/r/{id}/og.png`, `/r/`, `/r/{id}/` and `/r/{id}/{*rest}`, the last answering `404` with `NOT_FOUND_HTML` for everything but `og.png` (its requirements 8, 13, 14). The domain segment is normalised as v2-result-page's `/{domain}` normalises it (its requirement 2, `validate_domain`).
- **I8 (offered to v2-result-page, operator decision "result-page").** `netray_web::evidence::href(entry_id, protocol, block: Option<&BlockId>) -> String` (`/r/<id>/<protocol>`, plus `#ev-<block id>`), the one source of the result page's evidence links into these views; and `netray_web::evidence::section(run: &netray_model::Run, protocol, run_line) -> EvidenceView` with its partial, for an incomplete run's evidence in the stream, which no URL reaches (§6.1) A block of another module links `#ev-<block id>` on the same page there (the result page holds every protocol's section); the stored views use `entry_section(entry_id, created_at, run, protocol)`, the same partial, whose foreign-block links are `href(entry_id, <owner>, Some(block))`, the owner's view of the same entry (implementer under the operator's autonomy mandate, 2026-10-18).

## Requirements

1. `netray-web` depends on `netray-model` and maps a `netray_model::evidence::Block` to an `EvidenceBlockView` with one function that matches on `Shape` only, for all six shapes: records (owner, type, ttl, data from their tagged values), table (columns in stored order, the key column marked), chain (items in order), tree (depth-first, each node's fields as `key: value` joined by `, `, with its depth), fields (repeated keys kept in order), text (verbatim). Typed values render as: `text` verbatim, `int` decimal, `bool` `true`/`false`, `time` as stored (RFC 3339), `duration` `<n> ms`, `addr` in std Display form. No module name, block name or key appears in the renderer (§3.7: no module ships templates).
2. The `evidence_block` macro carries no `style` attribute: tree depth is a `data-depth` attribute styled by `evidence_block.css` for depths 0–7, deeper nodes clamped to 7 (default, not interviewed). Every block carries `id="ev-<block id>"`. Every value is HTML-escaped by askama; a block value never reaches markup unescaped.
3. An evidence view of protocol `p` for a run holds, in order (R28): a run line; one `check_row` per catalogue check of `p` in catalogue order, with the catalogue `short` as code, the status as tone (`pass`, `warn`, `fail` as theirs; `not_applicable`, `not_tested` and `unmeasured` as `untested`, the muted tone of `crates/web/src/components.rs:9-15`), the catalogue `title`, the run's findings as body (the catalogue `finding` for a warn or fail without findings), the catalogue `fix_hint` for warn and fail, a guide link from the catalogue, and one link per referenced block (anchor `#ev-<block id>` in the view that owns it); then every block whose ID module is `p`, in run order, each listing as feeds the `short` names of the checks that reference it (I3). A section that is `not_applicable` shows its reason above its check rows.
4. A block owned by another module is not rendered in `p`'s view (R29); when a check of `p` references it, the view shows one line with the block's title linking to the owner's view at `#ev-<block id>` (`tls.caa_compliant` → the DNS view's `dns.caa`).
5. The run line names the run (§6.1): for a stored entry `run of <created_at as YYYY-MM-DD HH:MM> UTC · entry <id>` (created, not last seen: the evidence is the creating run's, default, not interviewed); for a live incomplete run `incomplete, not recorded`.
6. netray-web's pages router routes exactly five evidence paths under a domain, `GET /{domain}/dns`, `/{domain}/tls`, `/{domain}/http`, `/{domain}/email` and `/{domain}/ip`, each a literal second segment and never a parameter or wildcard second segment; each renders the evidence view of the domain's latest entry (I7), and with no entry answers `404` with a link to `/<domain>` (default, not interviewed). Every other two-segment path under the apex (`/assets/index-<hash>.js`, `/example.com/smtp`) matches no route of this feature and keeps reaching lens's `spa::handler` fallback exactly as before, with `[history] enabled` (lens serves its hashed Vite assets there, `crates/lens/src/spa.rs:77-87`). `GET /r/<id>/<protocol>` (inside `/r/`, the namespace `entries::router` already owns) follows `/r/<id>`'s precedence (I7): malformed ID `404`, entry `200` with its view, a live V1 snapshot's ID `404` (a snapshot has no evidence view; default, not interviewed), else `410`; a failed lookup `500`. netray-web's pages router serves `/r/{id}/{segment}` (the five protocols as above, any other segment the 404 page). v2-entries lands first (lane A), so this feature changes its router: `entries::router` keeps `/r/{id}`, `/r/{id}/og.png`, `/r/` and `/r/{id}/` and narrows its catch-all `/r/{id}/{*rest}` to `/r/{id}/{segment}/{*rest}`, so the merged routers hold no conflicting route. lens mounts the evidence routes only where it mounts `entries::router` (`[history] enabled`); without a history store `/r/` stays exactly V1's (`snapshot_router`, v2-entries requirement 13). v2-entries' test rows that `/r/{id}/{segment}` now answers (`/r/a/b` and `/r/<entry id>/other`, formerly `404` with `NOT_FOUND_HTML`, now `404` with v2-pages' page) change under `ADLC-Test-Change: <path> -- /r/{id}/{segment} is the evidence view (v2-inspectors requirement 6)`. Neither route starts a run or writes to the store (P06).
7. An incomplete run's evidence is reachable by no URL (R27): the evidence routes read only stored entries, and the section renderer offered to v2-result-page (I8) takes a live run and renders the same markup as the stored view.
8. Every evidence view carries one explore-live link for the view, plus, for a block with a per-row rule (`tls.addresses`, `email.mx`, `ip.addresses`), one link per row, and for a block with a per-block rule (`dns.<type>`, `email.dmarc`, `email.mta_sts`), one link for the block; blocks without a rule get none. All are derived from the block by one function, never written per view (R30, §6.1): DNS view `/inspect/dns?q=<domain>`, a `dns.<type>` block `?q=<owner> <TYPE>`, `<TYPE>` the block's type and `<owner>` the records' one owner, else (owners differ or no record) the domain; TLS view `/inspect/tls?h=<domain>:443`, each `tls.addresses` row `?h=<address>:443` (IPv6 bracketed); HTTP view `/inspect/http?url=https://<domain>/`; email `email.dmarc` → `/inspect/dns?q=_dmarc.<domain> TXT`, `email.mta_sts` → `/inspect/dns?q=_mta-sts.<domain> TXT`, each `email.mx` row → `/inspect/tls?h=<host>:25` (a null MX `.` or a non-ASCII host gets none) (SMTP STARTTLS by port, `crates/tls/src/tls/connect.rs:91`); IP each `ip.addresses` row → `/inspect/ip?ip=<address>`. Query values are percent-encoded. Links are apex-relative `/inspect/<tool>`, valid before and after v2-cutover (default, not interviewed).
9. `GET /inspect/dns`, `/inspect/tls`, `/inspect/http` and `/inspect/ip` render the inspector page on `base.html` (I4) with a `query_input` whose field is `q`, `h`, `url` or `ip` (§7.3), prefilled from the query string, and an empty result region. A GET never starts a job. The form carries `data-island="stream"` and `data-stream-url="/inspect/<tool>/run"`, and `data-stream-autostart="load"` when prefilled: with JavaScript the `stream` island posts it on load (I5), as `crates/tls/frontend/src/App.tsx:183-187` does today, and waits out prerendering; without JavaScript the page says that a live job needs JavaScript (default, not interviewed). The speculation-rules constant gains `{"href_matches":"/inspect/*"}` (S8; v2-pages requirement 12 leaves it to V4.7), and v2-pages' test that no pattern matches `/example.com/tls` still holds. `GET /inspect/email` answers `404` (S3).
10. `POST /inspect/<tool>/run` takes the form field and answers an SSE stream of `fragment` events `{"id", "html"}` ending with `end` (I5): first the run line "live · not recorded" with the job's parts pending, then each part's evidence blocks through requirement 1's renderer as it finishes, then the band, then `end`. Invalid input answers one fragment in the result region with the validation message of the job function and ends the stream. A refusal by the per-client or per-target limit answers `429` with `Retry-After` and one fragment "too many requests; try again in N s" (default wording, not interviewed). A request carrying `Sec-Purpose: prefetch` or `prerender`, as `netray_common::prefetch::is_prefetch` decides it (v2-admission requirement 18), starts nothing and answers `503` with `Cache-Control: no-store` (S8; the answer v2-admission and v2-api give a prefetch that starts nothing).
11. Every inspector run renders `run_line` with "live · not recorded" and no `grade` component (§6.2, §8.3); no job writes to `history.db` or calls the store, and no job passes the domain-run admission (§4.4). A test runs a job against a store and asserts its entry count and every `last_seen_at` unchanged.
12. When the target is a host name, the page links to `/<registrable domain>` of that name by the Public Suffix List (§6.2, S29); an address target or a name without a registrable domain gets no link. The list comes from one PSL crate added as a workspace dependency, whose licence `cargo deny` accepts (default, not interviewed).
13. Inspector jobs reach the routes through one trait in `netray-web` (`InspectorJobs`), so router tests run offline with a stub. The real implementation lives in the binary beside `lens_registry` (`crates/netray/src/main.rs:143-238`), over the I1 functions, the module translations and the process's module instances (`Arc<IpModule>`, the DNS module); the binary builds netray-web's router with it and hands it to `lens::run_with`, which merges it ahead of `spa::handler` (X1). lens names no module crate (`test_lens_names_no_module.sh`), and the inspector routes in `crates/web/src/pages/` reach jobs only through `InspectorJobs`; netray-web's `api/` module, not this feature, depends on the module crates (v2-api requirement 1) (default, not interviewed).
14. Each inspector has its own deadline and concurrency limit (P12, P13): `[inspect.dns|tls|http|ip] deadline_ms, max_concurrent` in the lens config beside `[api.*]` (I2; v2-cutover moves both into `netray.toml`), `deny_unknown_fields`, parsed by `netray lens --check-config`; defaults DNS 30 000 (V1's DNS stream deadline, `crates/dns/src/api/mod.rs:34`; `max_timeout_secs` still clamps each query or hop inside it), TLS 15 000, HTTP 30 000 (the V1 budgets) and IP 5 000 ms, 8 slots each (default, not interviewed). A zero value is a startup reject with a `validate()` rule and a `startup_rejects` row in `tests/repo/test_check_config.sh`. A full inspector answers one fragment "busy, try again in a moment" and does not queue; a job past its deadline keeps its finished parts and marks the others "timed out". The page route and `/api/v1/<tool>` share one instance of each inspector's per-client and per-target limiters (I2), keyed by the same caller address as v1.
15. The TLS inspector accepts the inputs of `crates/tls/src/input.rs:29-35`, uses SMTP STARTTLS on ports 25 and 587 as today, and refuses ports 143 and 21 with "STARTTLS for IMAP and FTP is not supported" before calling the job (§6.2; default wording, not interviewed). Its blocks and band are `netray_tls::module::translate` over the job's `InspectResponse` (I1), the band of port 443 or the first port as `translate` picks.
16. The HTTP inspector accepts any `http` or `https` URL, path included, and renders the redirect chain, headers and the other blocks of `netray_http::module::translate` over the job's `InspectResponse`; its band is that translation's checks (I1).
17. The IP inspector without `ip` looks up the caller's address (the address v1 keys on, I2), with `ip` any global address; its blocks and band (reputation) are `netray_ip::module::translate(&[(addr, result)], 1)` (I1). Without `[modules.ip]` it answers one fragment "IP lookups are unavailable" (as v1's `503`, default wording, not interviewed).
18. The DNS inspector offers the five jobs of S22: query, trace (`+trace`), resolver compare (`+compare`), authoritative compare (`+auth`), DNSSEC chain (`+dnssec`), parsed by `crates/dns/src/parser.rs`, run through the I1 functions. `netray-dns` gains one function per job mapping its typed collected object to `netray_model::evidence::Block`s: trace and DNSSEC chain as `tree`, compares as `table`, a query as `records`, block IDs under `dns.`. `+check` is not a job: the page says the band covers it (default, not interviewed).
19. For every DNS job, the DNS module runs live on the registrable domain of the queried name (requirement 12), concurrently with the job and under the same deadline, and its checks render as the DNS band (S29, I6, requirement 25); without a registrable domain the band region says so.
20. A `dns` island (S22, §8.4) builds the `q` string from record types, resolver and job choice and writes it into the form; it computes no layout and holds no verdict, writes no style (gate rule 1, I4), has vitest tests in the gate, and the bundle stays under the 30 KB gzipped cap (I4).
21. Templates, stylesheets and the island of this feature pass the gate of §8.5 (`crates/web/src/gate.rs`) and v2-pages' `tests/repo/test_no_codenames.sh`, and contain neither "lens" nor a codename (`prism`, `tlsight`, `spectra`, `beacon`, `ifconfig`) (V4.12).
22. `specs/sdd/v2.md` is amended with the code that changes it: §6.1 and §6.2 cite `crates/tls/src/tls/connect.rs` (STARTTLS) and `:201` (the stubs) instead of `crates/tlsight/…`; §6.1 states the run line's timestamp (requirement 5), the `ev-<block id>` anchors and the explore-live derivation of requirement 8; §6.2 names the job route (requirement 10), the limits (requirement 14), the band restriction (requirement 25), and that before v2-cutover the inspectors exist only under `/inspect/<tool>` on netray-web's router merged into lens (X1), the subdomains keeping the V1 SPAs; the Phase 4 table marks V4.7 as built by `v2-inspectors`.
23. The acceptance seed, the one every later feature reuses (v2-result-page adds fixtures, v2-cutover runs it for `serve`): `crates/store/examples/seed.rs`, run as `cargo run -p netray-store --example seed -- <db path> <fixture dir>`, opens the store, calls `HistoryStore::migrate`, and records each JSON run fixture of `tests/acceptance/fixtures/history/` through `HistoryStore::record` with the model's diff (`netray_model::diff::diff`) as its `diff` argument. Fixture contract (binding for v2-result-page's fixtures, its requirement 27): every fixture is `{"domain": <string>, "age_seconds": <u64>, "run": <Run>}`; the seed reads its clock once (`seed_now`, Unix seconds) and records each fixture with `now = seed_now - age_seconds`, in sorted file-name order. It refuses a fixture carrying an absolute time field (any key besides those three at the top level, e.g. `created_at` or `now`), naming the file, and fails, naming both files, when a later file (in that order) of the same domain does not have a smaller `age_seconds` (`record` clamps a `now` before the chain head's, so an out-of-order fixture would silently move). Either failure records nothing (the seed validates every fixture before the first `record`). `just acceptance-local` runs it into lens's temporary `history.db` (`LENS_HISTORY__DB_PATH`) before lens starts. This feature's fixture is one `example.com` entry, `age_seconds` 0, whose run holds a records, table, chain, fields and text block and checks with findings and evidence references. A Rust test holds every fixture run in the directory to the catalogue, each catalogue ID exactly once, naming a missing one (default, not interviewed). The views and inspectors are reached at `LOCAL_LENS_URL`.
24. A Playwright spec `tests/acceptance/v2/inspectors.spec.ts` (project `local`) opens every evidence view of the seeded entry and every inspector page at 1280 px and 390 px in light and dark, asserts the run line, the check-row order, the shared-evidence line, each explore-live href, no horizontal page scroll at 390 px outside the band (S16), and that `/inspect/email` is `404`, and saves a screenshot per view, width and theme under `tests/acceptance/test-results/` (gitignored; listed in `report.md`, not committed; default, not interviewed). With the argument `--grep @live` it also runs one job per inspector against `netray.info` and `example.com` and asserts "live · not recorded", no grade and a rendered band.
25. A protocol's band is `spectrum::layout` (I6) over the catalogue restricted to that protocol's checks, with the one live run under `Profiles::current()` and no older run, rendered through `SpectrumView`. `netray-engine` gains `Catalogue::for_protocol(Protocol) -> Catalogue`, keeping catalogue order (default, not interviewed).

## Phases

## Phase 1 — One renderer over the six shapes

**Depends on:** lane B and v2-history/v2-admission/v2-entries merged; built stacked on `feature/v2-api` (its Phase 1 committed) and rebased onto main when v2-api lands (operator, 2026-10-18, X6)
**Requirements:** 1, 2

`crates/web`: `netray-model` dependency, `src/evidence.rs` with the block-to-view function and value formatting, the tree branch of `templates/components/evidence_block.html` and `static/evidence_block.css` on `data-depth`, `id="ev-<block id>"`. Table tests over every shape and value type.

### Test Scenarios

GIVEN a block of each of the six shapes WHEN rendered THEN the markup carries `data-shape` of that shape, `id="ev-<block id>"` and every value of the block, in order.
GIVEN a fields block with `int 3`, `bool true`, `time 2026-10-08T14:32:00Z`, `duration 120`, `addr 2001:db8::1` WHEN rendered THEN it shows `3`, `true`, `2026-10-08T14:32:00Z`, `120 ms`, `2001:db8::1`.
GIVEN a tree of depth 9 WHEN rendered THEN nodes carry `data-depth` 0 to 7, the deepest two clamped to 7, and no element has a `style` attribute.
GIVEN a text block holding `<script>alert(1)</script>` WHEN rendered THEN the markup holds it escaped.
GIVEN the renderer's source WHEN a test greps it THEN no block name (`dns.`, `tls.`, `http.`, `email.`, `ip.`) appears.

## Phase 2 — Evidence view model

**Depends on:** Phase 1
**Requirements:** 3, 4, 5, 7

`crates/web/src/evidence.rs`: `section(run, protocol, run_line)` building check rows from the catalogue and the run, owned blocks with feeds, shared-evidence lines, the run line; `href(entry_id, protocol, block)` (I8); `templates/partials/evidence_section.html`. Pure functions, tested over fixed runs of `example.com` whose checks carry findings and evidence references (I3).

### Test Scenarios

GIVEN a run whose `tls.chain_trusted` passes and references `tls.chain@192.0.2.1:443` WHEN the TLS view is built THEN its check row links `#ev-tls.chain@192.0.2.1:443` and that block lists `short` of `tls.chain_trusted` among its feeds.
GIVEN `tls.caa_compliant` referencing `dns.caa` WHEN the TLS view is built THEN `dns.caa` is not rendered there and one line links to the DNS view's `#ev-dns.caa`; GIVEN the DNS view THEN `dns.caa` is rendered.
GIVEN a warn check without findings WHEN its row is built THEN the body is the catalogue `finding` and the fix is the catalogue `fix_hint`.
GIVEN a stored check without `findings` and `evidence` (an entry recorded before X2) WHEN its row is built THEN it renders with the catalogue texts and no block link.
GIVEN a run whose TLS section is `not_applicable` with reason "no A or AAAA record" WHEN the TLS view is built THEN the reason is shown and every TLS check row has the `untested` tone.
GIVEN a stored entry `Ab3dE5fG` created 2026-10-08 14:32 UTC WHEN its view is built THEN the run line reads `run of 2026-10-08 14:32 UTC · entry Ab3dE5fG`; GIVEN a live incomplete run THEN `incomplete, not recorded`.
GIVEN the check rows of a view WHEN compared with the catalogue THEN they follow catalogue order and cover every check of the protocol.
GIVEN entry `Ab3dE5fG`, protocol TLS and block `tls.chain@192.0.2.1:443` WHEN `href` is called THEN it returns `/r/Ab3dE5fG/tls#ev-tls.chain@192.0.2.1:443`.

## Phase 3 — Explore-live links

**Depends on:** Phase 1
**Requirements:** 8

`crates/web/src/explore.rs`: one function from (view protocol, block, domain) to links; the view's default link. Table-driven over every rule.

### Test Scenarios

GIVEN a `dns.mx` block whose records share owner `example.com` WHEN links are derived THEN one is `/inspect/dns?q=example.com%20MX`.
GIVEN a `tls.addresses` block with rows `192.0.2.1` and `2001:db8::1` WHEN links are derived THEN they are `?h=192.0.2.1:443` and `?h=%5B2001:db8::1%5D:443` (bracketed, percent-encoded).
GIVEN an `email.mx` row with host `mx.example.com` WHEN links are derived THEN one is `/inspect/tls?h=mx.example.com:25`.
GIVEN `email.dmarc` and `email.mta_sts` blocks for `example.com` WHEN links are derived THEN they query `_dmarc.example.com TXT` and `_mta-sts.example.com TXT`.
GIVEN an `http.headers` block WHEN links are derived THEN it gets none, and the HTTP view's link is `/inspect/http?url=https%3A%2F%2Fexample.com%2F`.
GIVEN a TLS view with one `tls.addresses` block of two rows and one `tls.chain` block WHEN its links are derived THEN it holds exactly three: the view's link and one per address row, none for `tls.chain`.

## Phase 4 — Evidence view routes

**Depends on:** Phase 2, Phase 3
**Requirements:** 6

`crates/web/src/pages/`: the five literal routes `GET /{domain}/{dns,tls,http,email,ip}` and `GET /r/{id}/{segment}` on netray-web's router, reading `HistoryStore` (I4, I7); `templates/pages/evidence.html`. `crates/web/src/entries/`: `entries::router`'s catch-all narrowed to `/r/{id}/{segment}/{*rest}`, v2-entries' affected rows under `ADLC-Test-Change`. `crates/lens`: the evidence routes mounted only with `entries::router`. Router tests against a temporary store; one test merges netray-web's router into lens's app and requests `/r/<id>`, `/r/<id>/og.png` and `/r/<id>/tls`.

### Test Scenarios

GIVEN a store with one entry for `example.com` WHEN `GET /example.com/tls` THEN `200` with the TLS view of that entry.
GIVEN `GET /Example.COM./tls` THEN the same view as `/example.com/tls`.
GIVEN no entry for `example.com` WHEN `GET /example.com/dns` THEN `404` with a link to `/example.com`.
GIVEN an entry ID WHEN `GET /r/<id>/email` THEN `200` with that entry's email view; GIVEN a well-formed ID with no entry and no V1 snapshot THEN `410`; GIVEN `GET /r/abc/tls` THEN `404`; GIVEN a failing store THEN `500`.
GIVEN `GET /r/<id>/smtp` THEN `404` with v2-pages' page; GIVEN lens with netray-web's router merged and `[history] enabled` WHEN `GET /example.com/smtp` THEN lens's `spa::handler` answers it (the SPA shell), not this feature's router.
GIVEN lens with netray-web's router merged and `[history] enabled` WHEN `GET /assets/index-x.js` (an embedded hashed asset of lens's test build) THEN the SPA handler serves it with its asset content type and `Cache-Control: public, max-age=31536000, immutable`, as without this feature.
GIVEN lens with netray-web's router merged WHEN `GET /r/<id>`, `/r/<id>/og.png` and `/r/<id>/tls` THEN each is answered by its own handler and the app builds without a route conflict; GIVEN `/r/<id>/tls/x` THEN `404`.
GIVEN lens with `[history] enabled = false` and `[snapshots] enabled = true` WHEN `GET /r/<snapshot id>/tls` THEN it answers exactly as before this feature (`snapshot_router`'s `404`).
GIVEN any evidence route request WHEN answered THEN the store's entry count and every `last_seen_at` are unchanged.

## Phase 5 — Inspector frame

**Depends on:** Phase 1
**Requirements:** 9, 10, 11, 12, 13, 14

`crates/web/src/pages/`: `InspectorJobs` trait and its stub, `templates/pages/inspector.html`, `GET /inspect/{tool}`, `POST /inspect/{tool}/run` with fragment SSE, per-inspector deadline and concurrency limit, the shared limiter hook, the PSL back-link, the speculation-rules entry. `crates/lens`: `[inspect.*]` config with validation; `tests/repo/test_check_config.sh` rows. Router tests with stub jobs, offline (I4, I5).

### Test Scenarios

GIVEN `GET /inspect/tls?h=example.com:443` WHEN answered THEN the form is prefilled with `example.com:443`, carries `data-stream-autostart="load"`, the result region is empty, and the stub records no job.
GIVEN `POST /inspect/tls/run` with a stub returning two blocks WHEN streamed THEN the first `fragment` holds "live · not recorded", later fragments hold both blocks, the stream ends with `end`, and no fragment holds a `grade` element.
GIVEN `POST /inspect/http/run` with `Sec-Purpose: prerender` WHEN answered THEN `503` with `Cache-Control: no-store` and the stub records no job.
GIVEN `max_concurrent = 1` and one stub job in flight WHEN a second job posts THEN it answers the "busy" fragment at once.
GIVEN a stub limiter that refuses with 20 s WHEN a job posts THEN `429`, `Retry-After: 20` and one fragment, and the stub records no job.
GIVEN `deadline_ms = 100` and a stub with one part finishing at 50 ms and one at 500 ms WHEN streamed THEN the first part is shown and the second marked "timed out".
GIVEN `[inspect.dns] max_concurrent = 0` WHEN lens starts THEN it exits non-zero naming the key; GIVEN an unknown key under `[inspect.tls]` THEN `--check-config` exits 1 naming it.
GIVEN a target `www.example.com` WHEN the page renders THEN it links `/example.com`; GIVEN `192.0.2.1` THEN no link.
GIVEN a store and a stub job WHEN the job runs THEN the entry count and every `last_seen_at` are unchanged.
GIVEN the speculation rules WHEN parsed THEN `/inspect/dns` matches and `/example.com/tls` does not.
GIVEN `GET /inspect/email` THEN `404`.

## Phase 6 — TLS, HTTP and IP inspectors

**Depends on:** Phase 5
**Requirements:** 15, 16, 17, 25

`crates/netray`: the real `InspectorJobs` over the TLS, HTTP and IP functions (I1), the module translations, the shared limiter instances (I2), the port refusal, the caller address, mounted through `lens::run_with`. `crates/engine`: `Catalogue::for_protocol`. `crates/web`: the band through `spectrum::layout` (I6). Router tests use stubs; one job test per inspector with the module crate's `testing` feature where it exists.

### Test Scenarios

GIVEN `h=mail.example.com:143` WHEN posted THEN the stream holds "STARTTLS for IMAP and FTP is not supported" and the job function is not called.
GIVEN a stub TLS `InspectResponse` whose translation yields one `tls.chain` block and a failing `tls.not_expired` WHEN streamed THEN the chain renders and the band carries a fail line for `tls.not_expired`.
GIVEN `url=https://example.com/path?x=1` WHEN posted THEN the job receives that URL unchanged.
GIVEN `GET /inspect/ip` from a trusted proxy with `X-Forwarded-For: 192.0.2.7` and no `ip` WHEN the job runs THEN it looks up `192.0.2.7`.
GIVEN `ip=2001:db8::1` WHEN posted THEN the job looks up `2001:db8::1` and the band is the IP band.
GIVEN lens without `[modules.ip]` WHEN an IP job posts THEN one fragment "IP lookups are unavailable".
GIVEN `/api/v1/tls/inspect` exhausting the per-target budget of `example.com` WHEN `POST /inspect/tls/run` with `h=example.com` THEN `429` (one shared limiter).
GIVEN `Catalogue::for_protocol(Tls)` WHEN compared with the catalogue THEN it holds exactly the TLS checks in catalogue order, and `layout` over it yields one line per TLS check.

## Phase 7 — DNS inspector

**Depends on:** Phase 6
**Requirements:** 18, 19, 20, 21, 22

`crates/dns`: the job-to-block functions. `crates/netray`: the DNS jobs in `InspectorJobs` and the live DNS band on the registrable domain. `crates/web`: `islands/src/dns.ts` with vitest tests; the gate and the codename check over every template, stylesheet and island of this feature; the SDD amendments of requirement 22, in the last code phase so each statement is true when written.

### Test Scenarios

GIVEN `q=example.com A +trace` WHEN posted THEN the trace job runs and renders a `tree` block.
GIVEN `q=_dmarc.example.com TXT` WHEN posted THEN the query job runs and the DNS band is computed for `example.com`.
GIVEN `q=example.com +compare` and `+auth` WHEN posted THEN each renders a `table` block.
GIVEN `q=com NS` WHEN posted THEN the band region says there is no registrable domain and no DNS module run happens.
GIVEN `q=example.com +check` WHEN the page renders THEN it says the band covers the check and no `+check` job runs.
GIVEN the `dns` island with types `A`, `MX` and `+dnssec` chosen WHEN it writes the form THEN `q` reads `example.com A MX +dnssec`.
GIVEN the crate's templates, stylesheets and islands WHEN the gate and the codename check run THEN both pass.

## Phase 8 — Behavioural verification with screenshots

**Depends on:** Phase 4, Phase 6, Phase 7
**Requirements:** 23, 24

`justfile` (`acceptance-local` running the seed), `crates/store/examples/seed.rs`, `tests/acceptance/fixtures/history/example.com.json` and the fixture test, `tests/acceptance/v2/inspectors.spec.ts`. The offline set runs in `just acceptance-local tests/acceptance/v2/inspectors.spec.ts`; the `@live` set runs from main's target path (no network from a worktree binary). `report.md` lists every screenshot and what it shows.

### Test Scenarios

GIVEN the fixture directory with `example.com.json` (`age_seconds` 0) WHEN the seed runs at `seed_now` THEN the store holds one `example.com` entry with `created_at` and `last_seen_at` equal to `seed_now`.
GIVEN two fixtures of one domain, `a.json` with `age_seconds` 600 and `b.json` with 0 WHEN the seed runs THEN they are recorded in that order at `seed_now - 600` and `seed_now`.
GIVEN `a.json` with `age_seconds` 0 and `b.json` of the same domain with 600 (or 0) WHEN the seed runs THEN it exits non-zero naming `a.json` and `b.json` and the store holds no entry.
GIVEN a fixture carrying `"created_at": 1760000000` WHEN the seed runs THEN it exits non-zero naming that file and the store holds no entry.

GIVEN the local stack with the seeded entry WHEN `/example.com/tls` opens at 390 px dark THEN the run line names the entry, there is no horizontal page scroll, and a screenshot is saved.
GIVEN the local stack WHEN each of `/inspect/dns`, `/inspect/tls`, `/inspect/http`, `/inspect/ip` opens at 1280 px light THEN the form renders and a screenshot is saved.
GIVEN `/example.com/email` WHEN read THEN the `email.mx` row's explore-live href is `/inspect/tls?h=<host>:25`.
GIVEN `@live` WHEN `/inspect/tls?h=netray.info` opens with JavaScript THEN the job streams, the page shows "live · not recorded", a band and no grade, and the seeded store's entry count is unchanged.

## Decision log

- `RunCheck.findings` and `RunCheck.evidence` added by v2-api in its Phase 1 over this feature adding them in Phase 2: one change serves the API, the result page's evidence links and these views, and v2-result-page and this feature no longer race on the same fields (operator, 2026-10-17; X2).
- The generic `stream` island owned by v2-pages over whichever of v2-result-page and v2-inspectors builds first: one island built once, no collision on `islands/stream.ts` (operator, 2026-10-17; X3).
- A server-rendered job route (`POST /inspect/<tool>/run`, fragments) over an island calling API v1 and rendering client-side: one evidence renderer, islands without layout (§8.4, S25), the same job functions as API v1 (operator, 2026-10-17).
- netray-web's routers merged into lens ahead of `spa::handler` over a separate process or a `netray-api` crate: routers and API v1 live in `netray-web` as §3.1 says, and lens serves the apex until the cutover (operator, 2026-10-17; X1).
- Result-page evidence linked into these views (`href`, I8) over in-page panels on the result page: no second rendering of stored evidence; the section renderer serves only an incomplete run's stream (operator, 2026-10-17).
- Inspectors only under `/inspect/<tool>` before v2-cutover, the subdomains keeping the V1 SPAs from their own processes, over host routing now: Host routing belongs to the one process of S1 (V5.1) (operator brief, 2026-10-17; default, not interviewed).
- Explore-live links apex-relative (`/inspect/<tool>?…`) over subdomain URLs: R33 serves the same page under `/inspect/<tool>`, so links stay valid across the cutover (default, not interviewed).
- Jobs reach the routes through one `InspectorJobs` trait, implemented in the binary, over netray-web calling the module crates: router tests run offline, the process's module instances (the reloaded `IpModule`, the DNS module) stay where `lens_registry` builds them, and netray-web names no module crate (default, not interviewed).
- Checks and blocks from each module's existing `translate` over the job's V1 response over new job functions returning blocks: v2-api's functions return V1 types, and `translate` is already the one mapping a domain run uses, so the live band and the stored run agree (default, not interviewed).
- A prefilled GET renders the form and the island posts on load, over starting a job on GET: P06 and S8 forbid side effects on GET; a bookmarked `?h=` still runs as V1 does (SDD §6.2, S8).
- Tree depth as `data-depth` over the inline `style` custom property: the evidence block then needs no inline style, leaving `style-src 'unsafe-inline'` to the spectrum alone (default, not interviewed).
- Block anchors `ev-<block id>` over the bare block ID: matches v2-result-page's anchors and cannot collide with page IDs such as `band-<protocol>` (default, not interviewed).
- Run line timestamp is the entry's `created_at` over `last_seen_at`: the volatile values shown are the creating run's (default, not interviewed).
- A protocol band as `layout` over `Catalogue::for_protocol` over a band-specific layout: v2-spectrum's layout stays the one placement function, and heights stay within the protocol's section weights (default, not interviewed).
- netray-web's pages router owns `/r/{id}/{segment}` and `entries::router` narrows its catch-all, over adding the evidence handler to the `/r/` router: the router modules stay apart (v2-entries, v2-pages), and axum refuses a parameter segment beside a catch-all (default, not interviewed).
- `v2-inspectors` narrows `v2-entries`' `/r/{id}/{*rest}` catch-all to `/r/{id}/{segment}/{*rest}`
  under `ADLC-Test-Change` and mounts its `/r/{id}/{segment}` route only where `entries::router` is
  mounted (`[history] enabled`), over `v2-entries` pre-narrowing a route it gives no meaning:
  `v2-entries` lands first (lane A), the feature that gives the segment meaning changes the rows,
  and without a history store `/r/` stays exactly V1's (implementer under the operator's autonomy mandate, 2026-10-17).
- One acceptance seed, `cargo run -p netray-store --example seed` over JSON run fixtures in
  `tests/acceptance/fixtures/history/`, created by `v2-inspectors` (first in build order to need
  it) and extended by `v2-result-page`, over a second, `sqlite3`-loaded `history-seed.sql`:
  the seed calls `HistoryStore::migrate` (`crates/store/src/lib.rs:145`) and then
  `HistoryStore::record` (`:160`), which computes the fingerprint and takes the model's diff
  (`netray_model::diff::diff`) as its `diff` argument, so fixtures go through the same write path
  as a run and cannot drift from the schema, and no `sqlite3` CLI joins the toolchain (implementer under the operator's autonomy mandate, 2026-10-17).
- The prefilled form carries v2-pages' `data-stream-autostart="load"`, over a `data-autostart` attribute: v2-pages' `stream` island reads only `data-stream-autostart`, and the job route ignores the `trigger` field it adds (implementer under the operator's autonomy mandate, 2026-10-17).
- A prefetched job POST answers `503` with `Cache-Control: no-store`, decided by `netray_common::prefetch::is_prefetch`, over `403`: one answer and one helper for every request that starts nothing on prefetch, as v2-admission and v2-api answer it (implementer under the operator's autonomy mandate, 2026-10-17).
- `is_prefetch` lives in `netray-common` (`netray_common::prefetch::is_prefetch`), over lens's
  `admission.rs`: netray-web's run and inspector routes decide the prefetch flag with the same
  helper, and netray-web cannot depend on lens (implementer under the operator's autonomy mandate, 2026-10-17).
- The inspector routes reach jobs only through `InspectorJobs`, while netray-web's `api/` module depends on the module crates (v2-api requirement 1), over claiming netray-web names no module crate: v2-api lands first (lane A) and adds those edges, and lens depends on netray-web since v2-entries (implementer under the operator's autonomy mandate, 2026-10-17).
- One build order in every spec (lane A `v2-history` → `v2-admission` → `v2-entries` → `v2-api`;
  lane B `v2-spectrum` ∥ `v2-pages`, parallel to lane A; lane C `v2-inspectors`, its Phase 2 before
  `v2-result-page` Phase 1, then `v2-result-page`; then `v2-cutover`) over each spec's own reading
  of the program: every **Depends on** and Context names the same order (implementer under the operator's autonomy mandate, 2026-10-17).
- Five literal evidence routes `/{domain}/{dns,tls,http,email,ip}` over one `/{domain}/{segment}`
  route answering `404` for other segments: a parameter second segment would swallow every
  two-segment path ahead of `spa::handler`, among them lens's hashed `/assets/*` files, while
  history is enabled by default (implementer under the operator's autonomy mandate, 2026-10-17).
- One explore-live link per view, one per row for blocks with a per-row rule and one per block
  otherwise, over "one more link per block": `tls.addresses`, `email.mx` and `ip.addresses` link
  each row to its own target, which a single block link cannot (implementer under the operator's
  autonomy mandate, 2026-10-17).
- DNS inspector default deadline 30 000 ms over 10 000 ms: V1 runs DNS jobs under its 30 s stream
  deadline, and `max_timeout_secs` (10 s) clamps a single query or hop, not the job (implementer
  under the operator's autonomy mandate, 2026-10-17).
- Seed fixtures carry `age_seconds` against one `seed_now`, over absolute fixture times: the
  result page's fresh and stale states hold on any day, and the seed refuses absolute times and
  non-decreasing ages per domain instead of letting `record`'s clamp move an entry silently
  (implementer under the operator's autonomy mandate, 2026-10-17; v2-result-page's contract).

- Stacked on `feature/v2-api` instead of waiting for its merge, and no midday review pause (operator, 2026-10-17/18, X5, X6): the branch is rebased onto origin/main after v2-api merges and lands after it.
- `RecordRow.ttl` a formatted `String` over keeping `u32`: requirement 1 formats every typed value
  one way, and a `u32` has no lossless place for a ttl the renderer cannot convert; the protected
  component test changes under `ADLC-Test-Change` (implementer under the operator's autonomy mandate, 2026-10-18).
- Phases 1–3 built on `feature/v2-api`'s base before merging origin/main (v2-pages), over merging it
  first: they need nothing of v2-pages, and v2-api resolves the same conflicts when it merges main;
  this branch merges main or v2-api before Phase 4 (implementer under the operator's autonomy mandate, 2026-10-18).
- The phase defaults marked "(default, not interviewed)" are built as written, over halting on each:
  operator decision X5, no review pause (implementer under the operator's autonomy mandate, 2026-10-18).
- Foreign-block links entry-anchored in stored views (`/r/<id>/<owner>#ev-<block id>`, via
  `entry_section`) and same-page (`#ev-<block id>`) in the live section, over one path-relative
  `<owner>#ev-<block id>`: a relative link from `/<domain>/tls` re-reads the latest entry and can
  land on another entry's block, and from the result page `/<domain>` it resolves to `/<owner>`
  (review of Phase 2; implementer under the operator's autonomy mandate, 2026-10-18).
- A block's feeds list every referencing check of any protocol, over only the view's own checks:
  requirement 3 says "the checks that reference it", and `dns.caa` backs `tls.caa_compliant`
  (review of Phase 2; implementer under the operator's autonomy mandate, 2026-10-18).
- The email view's own explore-live link is `/inspect/dns?q=<domain> MX` and the IP view's
  `/inspect/dns?q=<domain> A`, over no view link: requirement 8 gives every view one link and names
  none for these two; both inspect the records the view's checks start from (implementer under the
  operator's autonomy mandate, 2026-10-18).
- Explore-live query values percent-encoded as URI components, with `:` kept literal in `q`, `h`
  and `ip` (requirement 8's `?h=%5B2001:db8::1%5D:443`) and encoded in `url`
  (`url=https%3A%2F%2Fexample.com%2F`); an `email.mx` host's trailing dot is dropped (implementer
  under the operator's autonomy mandate, 2026-10-18).
- A `dns.<type>` link always carries the block's type and queries the domain when the records'
  owners differ or the block is empty, over `?q=<owner of the first record>` and no link for an
  empty block: a CNAME chain sorts the target first and dropping the type ran the inspector's
  default types, and an answered empty `dns.caa` (no CAA, the common case) still deserves its
  query; a null MX row (`.`) gets no link, since `h=:25` is refused (review of Phase 3;
  implementer under the operator's autonomy mandate, 2026-10-18).
- An `email.mx` host that is not ASCII gets no explore-live link, over converting it to punycode:
  the email module prints IDN labels in Unicode, the TLS inspector accepts ASCII labels only, and a
  conversion would add an IDNA dependency to netray-web for a rare case; a link the inspector
  refuses is worse than none (second review of Phase 3; implementer under the operator's autonomy
  mandate, 2026-10-18).

## Open decisions

None.

## Out of scope

- Subdomain routing, the redirect map, deleting the V1 SPAs and `packages/common-frontend`: v2-cutover (SDD §7.3, V5.1, V5.3).
- API v1 and the inspector job functions: v2-api (SDD §7.1, Phase 3).
- `base.html`, CSP, islands bundle and `stream` island: v2-pages (SDD §8.4, V4.9, V4.10).
- `spectrum::layout` and the band geometry: v2-spectrum (SDD §5.4, V4.4).
- The result page, its stream and open issues: v2-result-page (SDD §5, V4.5, V4.13).
- `/r/<id>` itself and its `410`: v2-entries (SDD §4.6, V2.7).
