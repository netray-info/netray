# Plan: v2-cutover

## Phase 1 — `netray serve` and `netray.toml`

### Groups

G1: C4, C12–C14, C16–C22 (`legacy.rs`) · G2: C2, C3, C9–C11 (`config.rs`) · G3: C1, C8, C23, C24 (`main.rs`, `lens::run_config`) · G4: C5–C7 (`netray.example.toml`, `netray.dev.toml`). Every test runs through `netray serve`, so G1, G2 and G4 go green only with G3's `mod` lines and subcommand; the four groups are coded by one coder in order G1 → G4.

### Plan

- **G1** `crates/netray/Cargo.toml`: `toml`, `serde`. `crates/netray/src/legacy.rs` (new): `pub fn refuse(raw: &toml::Table) -> Result<(), String>` over one static table of legacy key paths per origin (requirement 11), run on the raw file before deserialization; every message names the key path, its destination or "removed in 0.30.0", and the CHANGELOG 0.30.0 configuration table. Top-level `inspect` is legacy only for spectra's four keys. A bare key owned by exactly one module config names `[modules.<m>].<key>`; shared ones point to the CHANGELOG.
- **G2** `crates/netray/src/config.rs` (new): `Config` with `[server]` (+ `apex`), `[engine]`, `[store]`, `[admission]` (8 keys), `[badges]`/`[og_cards]` (lens's types), `[telemetry]`, `[modules]` (tables with `deadline_ms`); all `deny_unknown_fields`. `load(path, env)`: refuse the eleven legacy prefixes, legacy table on the raw file, filter `NETRAY_LIVE_TESTS`, `load_with_env(path, "NETRAY_", env)`, `validate()` (lens's rules under the new key names). `to_lens()` builds `lens::config::Config` (deadlines → `backends`, `deadline_ms` stripped from module tables, snapshots off). `specs/sdd/v2.md` S10 lists the tables.
- **G3** `crates/lens/src/lib.rs`: `run_config(config, registry)` split out of `run_with`. `crates/netray/src/main.rs`: `mod config; mod legacy;`, `Command::Serve { config, --check-config }`; check = load + `to_lens` + `lens_registry(.., false)`; serve = path from argument or `NETRAY_CONFIG`, load, `to_lens`, telemetry, `lens_registry(.., true)`, SIGHUP reload, `lens::run_config`.
- **G4** `crates/netray/netray.example.toml`: table headers uncommented, every key commented at its default, `[modules.*]` commented out wholly with `deadline_ms`. `crates/netray/netray.dev.toml`: local binds, dev store, no `[modules.ip]`.

CLAUDE.md stays out of the phase commit (shared prose, Phase 8).

## Phase 2 — Host routing

### Groups

G1: C16 (`legacy.rs`) · G2: C3, C15, C1, C2, C4–C7, C9, C10, C12–C14 (enrichment and host routing share `crates/netray/src/main.rs`; enrichment steps first). C8 deferred, C11 already in place. Test call sites of the module constructors were updated beforehand (c3f0201e).

### Plan

- **G1** `crates/netray/src/legacy.rs`: a fate "removed in 0.30.0, enrichment calls the in-process ip module" and four rows `modules.dns.backends.ip`, `modules.tls.backends.ip`, `modules.http.enrichment`, `modules.email.backends.ip_url`, matched before descent; prism/tlsight `backends.ip.*` and beacon's `backends.ip_url`/`timeout_ms` take the same fate.
- **G2 enrichment** `crates/common/src/enrichment.rs`: `EnrichmentSource` trait, `Transport::InProcess`, `EnrichmentClient::in_process` (base_url "", private-IP guard kept, `is_reachable` true). `crates/ip`: `enrichment` feature of netray-common; `IpModule` gains a stub variant (`testing::stub_ip_module(json)`), `impl EnrichmentSource for IpModule` (ifconfig `network` → `IpInfo`), `enrichment_client(Option<&Arc<IpModule>>)` re-exported. `crates/{http,dns,tls,email}`: `ModuleConfig` drops `enrichment`/`backends`; constructors `new(config, enrichment)`; V1 `Config`s and their own clients unchanged. `crates/netray/src/main.rs` `lens_registry`: ip module first, client to the four modules; `strip_lens_enrichment` keeps `netray lens` accepting its old tables (and its `ip_url` warning) until Phase 7.
- **G2 host routing** `crates/web/static/{bimi-logo.svg,mta-sts.txt}` (copies; `site/` keeps its own until Phase 7). `crates/web/Cargo.toml`: `tower` (util). `crates/web/src/host/mod.rs`: `HostRouters { apex, probes, apex_router, dns/tls/http/ip: Option<Router> }`, `router()` — probes first on any Host; host from `Host` or URI authority, port stripped, ASCII-lowercased; apex: `/favicon.svg`, `/bimi-logo.svg`, 404 for `/favicon.ico`, `/.well-known/*`, bare `/r`, `/api`, `/inspect` (± `/`) and `/metrics`, then `apex_router`; inspector hosts → their router or 404; `mta-sts.` → only the policy; `email.`, `lens.`, unknown, none → 404. `crates/lens/src/lib.rs`: `prepare` (state, stores, `app(state)`), `serve` (listeners, metrics, drain), `probe_router()`; `run_config` = both. `crates/netray`: depends on netray-web; the Serve arm composes `host::router` over `lens::prepare` and calls `lens::serve`. `specs/sdd/v2.md` §5.1 (`/health`, `/ready` before host routing; `bimi-logo.svg`) and §7.3 (unknown host 404, `mta-sts.`).

## Phase 3 — Redirects and 410

### Groups

G1: C1–C9, C12–C16 (`crates/web/src/host/`, `pages/errors.rs`) · G2: C10 (`entries/routes.rs`) · G3: C19, C20 (guide templates). C11, C17 deferred; C18 in place.

### Plan

- **G1** `pages/errors.rs`: `domain_path(raw)` shared; `query_redirect_absolute(apex, &Uri) -> Option<Response>` (absolute 301, 400 page for a bad value); `query_redirect` keeps its relative form (pinned by `pages_errors.rs`). `host/legacy.rs` (new): `moved` (301 absolute, GET/HEAD; other methods 405 `Allow: GET, HEAD`), `gone` (410, `ErrorResponse` code `gone`), `lens_host` (13a), `subdomain_common` (13f, 13h), `email_host` (13g, then f/h, then c/d on `/`, else e), `ip_successor` (14, `/ip/cidr` before `/ip/{fmt}`), `apex_legacy` (m, i, o, l, n in that order). `host/mod.rs` `dispatch`: apex `?d=` and `apex_legacy` before the reserved list (`/api/` leaves it), `lens.`/`email.` to their tables, inspector hosts `subdomain_common` (ip: `ip_successor` first) before the inspector. SDD §7.2 (`ip.` `/docs` 301) and §7.3 rows h, i, j, l–o.
- **G2** `entries/routes.rs`: `/r/{id}/{*rest}` → `entry_rest`: 410 `render::gone()` for a well-formed ID, `rest` one of `dns|tls|http|email|ip` and no stored entry; else `unmatched`.
- **G3** `domain-security-audit.html`: the dns, tls, email (and ip) samples become `/api/v1/…` (`POST /api/v1/dns/query`, `/api/v1/tls/inspect`, `/api/v1/email/inspect`, `GET /api/v1/ip/{address}`). `scoring.html`: only the `?d=…&dkim_selectors=…` sample changes (to `https://netray.info/example.com`); the rest of the `dkim_selectors` section is Phase 8's prose pass (inventory).

## Phase 4 — Metrics

### Groups

G1: C1–C7, C9–C12 (one group: every criterion changes `crates/lens/src/metrics.rs`, most also `admission.rs`, `history.rs`, `routes.rs`, `lib.rs`). C8 deferred.

### Plan

1. `crates/common/src/server.rs`: `install_metrics(buckets, after_install) -> handle` and `serve_metrics_handle(addr, shutdown, handle)`; `serve_metrics_with` composes them (V1 services unchanged). Under `serve` the recorder is installed before `lens_registry` and `prepare`, so startup gauges land.
2. `crates/store/src/lib.rs`: `HistoryStore::totals() -> (entries, domains)`.
3. `crates/lens/src/metrics.rs`: buckets renamed and added (`netray_run_duration_seconds`, `netray_client_hourly_runs`, `netray_time_to_first_section_seconds`, `netray_time_to_result_seconds`, `netray_run_interval_seconds`); `init_zero_series` describes and zero-registers all of §9.2 (runs 2×5, section outcomes 5×4, fixes 5, badge cache 4, rejection reasons 4, entry views, one api endpoint template, histograms, history gauges described) and the renamed operational series; `count_request` and `lens_check_requests_total` go.
4. `security/rate_limit.rs`, `admission.rs` (recompute) prefixes `lens` → `netray`.
5. `admission.rs`: `refused()` counts `netray_admission_rejections_total`; `check_limits` stops counting (the route counts `limit`); `Trigger::label()`; `run_task` counts `netray_runs_total{outcome,trigger}`, observes time to first section and to result, counts `netray_section_outcomes_total{section,outcome}`; `admit_dkim` and `/api/check` use `Trigger::Api`.
6. `history.rs`: `record` reads the previous head: predecessor counter, `netray_run_interval_seconds`, `netray_fixes_observed_total{section}` from `ChangeClass::Fixed`; error counter renamed; `set_gauges(store)` from `totals()`, called by `prepare` and after each sweep.
7. `routes.rs`, `og/handler.rs`: badge series → `netray_badge_requests_total`, `netray_badge_render_duration_seconds`; `run_check_handler` counts `reason="limit"`.
8. `lib.rs`: `http_metrics("netray")`; `install_metrics()`; `serve` takes the handle; `run_config` installs before `prepare`. `crates/netray/src/main.rs`: Serve arm installs before `lens_registry`.
9. `crates/web`: `metrics` dependency; `netray_entry_views_total` on an entry page view.
10. Module run path series → `netray_<module>_<suffix>` (dns `run_check`, circuit breaker, tls `inspect`, http `inspect`, email checks and resolver, ip enrichment and backend); dns `endpoint` label → `operation`, email `target` → `fetch`.
11. `specs/sdd/v2.md` §9.2 badge row cites the new series. CHANGELOG stays with Phase 8 (inventory).

## Phase 5 — Agreement

### Groups

G1: C3, C7, C8 (`tests/acceptance/`). C1, C2, C4, C5 already hold for main's views (`crates/netray/tests/agreement.rs`); C6 deferred. One group with obvious files, so the plan was written without a planner run.

### Plan

- **G1** `tests/acceptance/agreement/prod.spec.ts` (new): for `netray.info` and `example.com`, one `POST /api/v1/domains/{domain}/runs` each without streaming through a request context; page grade = API grade = badge letter; open issues = API warn/fail checks; every evidence view names the same entry. No page navigation; only the two domains. `playwright.config.ts`: `prod` = that spec plus `smoke/health`, `smoke/ready`, `smoke/security-headers`, `smoke/tls-certs`. `fixtures/env.ts`: no `lens`; production = the apex plus `dns.`, `tls.`, `http.`, `ip.`; local = one origin (`LOCAL_URL`) addressed by `Host`; smoke specs follow. Removed: `services/*`, `integration/cross-links.spec.ts`, `static-site/*`, the `api-contract` specs against V1 (`/api/meta`); others rewritten for V2 or removed when their subject is gone.

## Phase 6 — Survivors out of lens

### Groups

G0: C5, C6, C11, C12 (independent: `crates/web/src/api`, `crates/ip` + `legacy.rs`, `ci.yml`, inspector template + `stream.ts`) · G1: C1's engine and store half (additive copies) · G2: C1's netray-web half, C8, C9, C13 (apex composition in netray-web, inspector hosts) · G3: C1's composition root, C2, C3, C4, C7, C10 (`crates/netray` off lens; V1 `netray lens` isolated in `v1.rs`).

Copy, not move: the operator keeps V1 runnable as the reference (2026-10-19), so `crates/lens` and its tests stay untouched until Phase 7 deletes them; the survivors are copied without their V1-only parts and `serve` switches to the copies (Decision log 2026-10-19).

### Plan

- **G0** C5: `crates/web/src/api` counts `netray_api_requests_total{endpoint}` with the matched route template (`MatchedPath`, prefixed `/api/v1`) on every `/api/v1` answer. C6: `crates/ip` `ModuleConfig.user_agent_regexes`, the V1 UA parser moved into the module, `lookup_caller` fills the parsed UA; `legacy.rs` row `user_agent_regexes` → `[modules.ip].user_agent_regexes`; `module_data.rs`'s refusal row retired under `ADLC-Test-Change`. C11: `ci.yml` check job `timeout-minutes: 60`. C12: `partials/inspect_backlink.html` carries `data-stream-reset`; `islands/src/stream.ts` empties `[data-stream-reset]` elements of the page before posting.
- **G1** `crates/engine/src/domain_run.rs`: `check.rs` (`run_check_streaming`, `HARD_DEADLINE`, `SectionError`), `modules.rs`' `ModuleSection::adapt`/`verdict` without `BackendExtra`, `score.rs`' survivors, `history::build_run` and `volatile`, over a section plan `Vec<(Protocol, Duration)>`, a resolve budget and `&Profile`; stored `Run` bytes unchanged (NotTested → Skip → NotApplicable, evidence filter, `NoDnsResults`, `{e:?}` reasons, facts on DNS evidence, sorted sections, `engine_version`). `crates/store/src/record.rs`: `record`, `observe_recorded`, `set_gauges`, `run_sweep_loop`, the diff and volatile passed in. Engine-only tests (model_status, scoring_regression, ip_enrichment_errors, v3_render's scoring half) copied to `crates/engine/tests`.
- **G2** `crates/web/src/admission/` (admission.rs without V1 parts, rate limits, head lookup, `ApexState`, the `DomainRuns` implementation), `metrics.rs`, `error.rs` (subset + `validate_domain`), `badge/`, `og/` + fonts under `crates/web/assets/fonts/`, `pages/{badge,og}.rs`, `host/{apex,layers,probes}.rs` (lens's `routes()` without V1, its layer stack, probes); `client_ip_from` merged into `client.rs`. Inspector routers for `dns.`/`tls.`/`http.`/`ip.` at `/` over the same `InspectDeps` with the trusted-proxy extractor; `ip.` root answers a curl user agent with the caller's address as text. Tests of classes (a)/(c) copied to `crates/web/tests` on the new harness.
- **G3** `crates/netray/src/config.rs` builds the narrow structs directly (defaults as constants, `[api]` and `[inspect]` tables, `to_lens` removed for serve); `serve.rs` (listeners, metrics listener, flush, drain, telemetry); `v1.rs` holds `netray lens` and `strip_lens_enrichment`, the only lens user; the agreement test's harness on the new composition, assertions unchanged, then extended by C10's rows; C7 wiring test over the registry with `stub_ip_module`.

## Phase 7 — Deletion

### Groups

G1: C1, C3, C4, C6, C10–C15 (top level: `crates/netray` serve-only, `crates/lens`, frontends, `packages/`, `site/`, npm workspace, Dockerfile, release.yml, justfile, acceptance harness, `tests/repo` rewrites) · G2: C2, C5, C7–C9 (module crates `dns`, `tls`, `http`, `email`, `ip` and `common`: V1 service code). Planned by the orchestrator without a planner run: deletions with obvious files.

### Plan

- **G1** `crates/netray/src/main.rs` keeps `Serve` only; `site.rs`, `v1.rs` and the lens dependency go. `git rm` of `crates/lens`, `crates/*/frontend`, `packages/`, `site/`. `package.json` workspaces = `crates/web/islands`, lockfile regenerated. Dockerfile builds the islands only, ships `netray.example.toml`, `ENTRYPOINT ["netray","serve"]`. release.yml smoke = one `serve`, `/health` on the apex and four inspector hosts; workflow-rules R-R4. justfile: `validate-site`/`check-sitemap` out of the gate, islands-only dist check, one-process `acceptance-local` (apex `localhost`, `ACCEPTANCE_ENV`), `design-check` over one `serve`. The V1 site files the V2 parity tests compare against move to `crates/web/tests/fixtures/v1-site/`.
- **G2** each module crate loses `run`, its router and handlers, RustEmbed assets, docs page, per-service `Config` loader, hot reload, metrics server and toml files; shared types netray-web uses stay; `netray_common` loses `spa`, `backend` and the HTTP enrichment transports (requirement 28).
