# Spec: v2-cutover

Status: Ready for Implementation
Created: 2026-10-17

## Goal

`netray serve` is the binary's only subcommand. One process, configured by one `netray.toml` (env prefix `NETRAY_`, `deny_unknown_fields`), serves the apex, `dns.`, `tls.`, `http.`, `ip.`, `email.`, `lens.` and `mta-sts.` by `Host`. Every redirect and `410` of SDD §7.3 and §7.2 answers as specified, and each has a test row through the router. Every exported metric is named `netray_*`. The §9.2 set is registered at startup, and no metric label carries a domain or an address. A local agreement test reads one run through every view and finds the same grade and statuses. The narrow `just acceptance` project `prod` checks the same against `netray.info` and `example.com`. Gone from the tree: `crates/lens`, the per-service routers and `run(config)` functions, the six Solid apps, `packages/common-frontend`, `site/` and all `snapshots.db` code. V1 `/r/<shortid>` answers `410`. Every old config file, table and env prefix is refused with a message naming its replacement. `CHANGELOG.md` states 0.30.0 with every claim traced to code. `specs/sdd/v2.md` records that only `0.30.0` is released. `just release 0.30.0` has committed and tagged locally. This is SDD Phase 5, V5.1–V5.6, on the code side. V5.2's "the infrastructure runs one container", V5.5's production run and V5.6's deletion of the `snapshots.db` file on argus are operator handoff items, listed under Out of scope; V5.6's code half (V1 `/r/` IDs answer `410`, the snapshot code removed) is built here.

## Non-goals

- netray-ops changes: one container, the merged production config, Traefik routers, Prometheus scrape jobs, DNS. They are listed as exact handoff items under Out of scope.
- The deploy, pushing `main` or the tag, and running `just acceptance` against production.
- The second baseline pull (V0.3) and deleting the `snapshots.db` file on argus.
- New product behaviour. Pages, API, inspectors, history, admission and entries come from the eight Phase 2–4 features. This feature composes, routes, renames, deletes and documents.
- Re-setting the admission numbers (S33: after V2, from 30 days of metrics).
- `1.0.0` (S24).

## Context and constraints

- **Built last.** Build order (one order in every spec): lane A `v2-history` → `v2-admission` → `v2-entries` → `v2-api`; lane B `v2-spectrum` ∥ `v2-pages`, parallel to lane A; lane C, after the operator's UI review at midday 2026-10-18, `v2-inspectors` (its Phase 2 before `v2-result-page` Phase 1), then `v2-result-page`; then this feature. It starts after all eight are merged. Facts below are read at the `v2-history` worktree (HEAD `35f88fe`) and describe the V1 shape this feature removes. Where a sibling feature has already moved or replaced something, the requirement applies to wherever it lives at the start of Phase 1, and Phase 1 records that inventory in `report.md`.
- **Where the router lives (operator X1, 2026-10-17).** The axum router with host routing, API v1, the templates, the OG card and the badge live in `netray-web`, as SDD §3.1 says. There is no `netray-api` crate. Until this feature, lens mounts netray-web's routers ahead of its `spa::handler` fallback. This feature moves host routing into netray-web, and `serve` composes it. The siblings lift the Phase 4a guards in `tests/repo/test_web_crate.sh` (no `Router`, no axum in netray-web, `:51` the `netray` binary does not depend on `netray-web`): a `Router` is allowed under `crates/web/src/entries/`, `pages/` and `api/`, axum is allowed, and `:51` is lifted by `v2-pages` or `v2-api`, whichever landed first. This feature lifts under `ADLC-Test-Change` whatever is still in place and adds the host router's module to the allow-list.
- **What the sibling drafts leave for this feature.** These are drafts of 2026-10-17, and Phase 1 checks every item against the merged code.
  - `v2-admission`:
    - lens config `[admission]` with `client_budget_per_hour` (30) and `domain_run_slots` (8);
    - `[rate_limit]` with `per_ip_per_minute`, `per_ip_burst`, `global_per_minute` and `global_burst`;
    - `[cache]` with `enabled` and `ttl_seconds`, where `ttl_seconds` is now §4.4's cache window;
    - `crates/lens/src/admission.rs`: the in-flight map, the run task, the throttle, the per-client limiter, the slots and the `Sec-Purpose` refusal, behind `check_limits`, `admit` (`Trigger` `visit | recheck | api | badge | og`) and the `RunStream`; `netray_common::prefetch::is_prefetch`, which stays;
    - moka narrowed to the V1 holder of the head's V1 rendering (`CachedResult`), which goes with the V1 wire;
    - the series `lens_check_requests_total{result}` (with `joined`), `lens_admission_rejections_total{reason}`, `lens_runs_in_flight` and `lens_history_record_errors_total`, checked by `test_lens_admission_metrics.sh`;
    - the `[admission]` table in `crates/lens/tests/fixtures/lens.production.toml`.
  - `v2-entries`:
    - in netray-web (`crates/web/src/entries/`): Entry → `EntryPageView`, `entries::render::{entry, gone, privacy}`, `entries::router(EntriesDeps)` with `/r/{id}` and `/r/{id}/og.png`, and `entries::privacy_router()` with the one `/privacy` route, all mounted by lens;
    - in lens: the V1 snapshot lookup and the entry card renderer it hands to `EntriesDeps`, and the mount condition;
    - `/r/{id}` precedence: a malformed ID answers 404, an entry 200, a live V1 snapshot its V1 page, and anything else 410;
    - `[history] enabled = false` restoring the V1 `/r/` exactly (`snapshot_routes.rs`, protected).

    The coexistence rule ends here (V5.6, requirement 23).
  - `v2-api`:
    - the v1 router, `netray_web::api::router(ApiState)` in `crates/web/src/api/` (X1);
    - the trait `netray_web::api::DomainRuns` (`validate`, `start`), which lens implements over `lens::input::validate_domain` and `check_limits` then `admit`, and which `v2-result-page`'s run POST also calls;
    - lens config `[api]` with `enabled`, `per_ip_per_minute` and `per_ip_burst`;
    - `[api.dns]`, `[api.tls]`, `[api.http]`, `[api.email]` and `[api.ip]` (for example `batch_max_size`), which move into `netray.toml` (operator, 2026-10-17);
    - `tests/fixtures/contracts/api-v1.openapi.json` and `tests/repo/test_api_no_verdict.sh`, which stay;
    - the ip lookup, batch, range, diff and ASN functions in `netray-ip`, which "today's routes and v1" both call;
    - the gate test of the curl root on the `netray ip` router (`justfile:52`);
    - `lens::input::validate_domain` (behind `DomainRuns::validate`) and lens's client-address extractor, which v1 and the result routes receive.
  - `v2-pages`:
    - `netray_web::pages::router()`, merged by lens ahead of `spa::handler`. It serves the guides (`scoring` included), `/guide/`, `/compare`, `/status` and `/static/*` (no `/privacy`, which is `v2-entries`'), with the 404 fallback, the `?d=` 301, the robots handler and the generated `/sitemap.xml` exported as their own handlers for this feature's apex router;
    - `crates/web/islands` as the npm workspace that `check-frontend-dist` and `test_frontend_toolchain_majors.sh` now also cover;
    - `test_no_codenames.sh`;
    - deferred here explicitly: deleting `site/` (with `site/og/` and `site/scripts/build-sitemap.mjs`), `netray site`, `test_site.sh`, `validate-site`, `check-sitemap` and `COPY site` in the `Dockerfile`; the apex root files `/.well-known/mta-sts.txt`, `/bimi-logo.svg` and `/favicon.ico`; §5.1's reserved-path test; and the redirects for `/tools`, `/api/*.html` and `/guide/lens` → `/guide/scoring`.
  - `v2-spectrum`: `netray_web::spectrum` only. Nothing moves.
  - `v2-result-page`:
    - `GET /`, `GET /{domain}`, `POST /{domain}/run` and `GET /{domain}/history` on netray-web's router, merged into lens ahead of `spa::handler`;
    - the OG card switched to `spectrum::layout` (X4), wherever it is rendered then.
  - `v2-inspectors`:
    - `GET /<domain>/<protocol>`, `GET /r/<id>/<protocol>`, `GET /inspect/<tool>` and `POST /inspect/<tool>/run` on netray-web;
    - the `InspectorJobs` trait, with its real implementation "where the router is composed";
    - `[inspect.dns]`, `[inspect.tls]`, `[inspect.http]` and `[inspect.ip]`, each with `deadline_ms` and `max_concurrent`, in the composing process's config;
    - in `just acceptance-local`, the seed `cargo run -p netray-store --example seed` over `tests/acceptance/fixtures/history/` (`v2-result-page` adds its fixtures), run into lens's `LENS_HISTORY__DB_PATH`, the pages reached at `LOCAL_LENS_URL`; `v2-result-page`'s `LENS_EXTRA` pass-through;
    - `tests/acceptance/v2/inspectors.spec.ts`.

    Until this feature, the subdomains keep the V1 SPAs, and the inspectors exist only under `/inspect/<tool>`.
- **The binary has seven subcommands.** `lens dns tls http email ip site` (`crates/netray/src/main.rs:26-83`). Each of the first six takes `--check-config <path>` (`main.rs:85-99`, `:257-289`). `lens_registry` builds the module registry from lens's `[modules.*]` (`main.rs:144-238`), and the IP data reloads on SIGHUP (`main.rs:241-250`). `tests/repo/test_cli.sh:11` asserts the seven.
- **`netray site` serves `site/`.** It also serves the MTA-STS policy on any `mta-sts.` host and 404 for every other path there (`crates/netray/src/site.rs:74-85`, policy file `site/.well-known/mta-sts.txt`). SDD §5.1 reserves `/.well-known/`.
- **Services are libraries today.** Each has a router, `run(config)` and a per-service `Config` with its own loader: dns `crates/dns/src/config.rs:369-372`, tls `crates/tls/src/config.rs:363-366`, http `crates/http/src/config.rs:176-177`, email `crates/email/src/config.rs:235-236`, ip `crates/ip/src/config.rs:281-282`, lens `crates/lens/src/config.rs:464-475` (prefix `LENS_`, filtering the test switch `LENS_LIVE_TESTS` at `:474`).
- **One loader, one refusal helper.** All loading goes through `netray_common::config::load`/`load_with_env` (`crates/common/src/config.rs:18`, `:47`). `refuse_legacy_prefix(legacy, new, env)` fails naming the variable, `new*` and `<new>CONFIG` (`config.rs:24-44`). `<NAME>_CONFIG` names the file and is never a key (`config.rs:53-63`).
- **lens's config tables.** `server, backends, ecosystem, cache, rate_limit, scoring, site, badges, og_cards, snapshots, history, modules, telemetry` (`crates/lens/src/config.rs:11-42`). `backends.resolve_timeout_ms` is the resolve budget (`crates/lens/src/check.rs:110`). `backends.<m>.timeout_ms` are the section windows (`crates/lens/src/state.rs:106-142`). `ecosystem` and `site` feed the Solid apps' `/api/meta` (`routes.rs:445-455`).
- **The services' own top-level keys.** dns: `site_name, limits, circuit_breaker, dns, trace, ecosystem, backends`. tls: `site_name, limits, dns, validation, ecosystem, quality, backends`. http: `inspect, limits, enrichment, meta`. email: `dns, dnsbl, http, rate_limit, dkim, ecosystem, backends`. ip: `base_url, site_name, project_name, project_version`, the thirteen data-file keys `geoip_city_db, geoip_asn_db, user_agent_regexes, tor_exit_nodes, cloud_provider_ranges, feodo_botnet_ips, cins_army_ips, vpn_ranges, datacenter_ranges, bot_ranges, spamhaus_drop, asn_patterns, asn_info` (`crates/ip/src/config.rs:22-34`; ip's `ModuleConfig`, `:236-248`, has twelve of them, all but `user_agent_regexes`), `filtered_headers, internal_mode, watch_data_files, rate_limit, batch, cache, dns_base_url, tls_base_url`. Every one of them also has `server` and `telemetry` (each `crates/<m>/src/config.rs`, `pub struct Config`).
- **Routes that exist today and must answer per §7.2/§7.3.**
  - ip: `crates/ip/src/routes.rs:107-166`: root, `/json /yaml /toml /csv`, `/ip`, `/ip/cidr`, `/ip/{fmt}`, the field family `tcp location user_agent network host isp all country city asn timezone latitude longitude region headers ipv4 ipv6`, each bare and `/{fmt}` except `asn` (`/asn/{param}`), plus `/range`, `/diff`, `/batch[/{fmt}]`, `/meta`, `/api/meta`, `/health`, `/ready`.
  - dns: `/api/{authcompare,check,compare,config,dnssec,meta,parse,query,record-types,results/{key},servers,trace}`, `/docs`, `/docs/`, `/api-docs/openapi.json` (`crates/dns/src/api/mod.rs:198-215`). The `/test` route of `crates/dns/src/security/mod.rs:72` belongs to a unit-test router and is no route.
  - tls and http: `/api/inspect`, `/api/meta` (http also `/api/config`), `/docs`, `/api-docs/openapi.json` (`crates/tls/src/routes.rs:245-253`, `crates/http/src/routes.rs:119-130`).
  - email: `/inspect`, `/inspect/{domain}`, `/api/meta`, `/docs`, `/api-docs/openapi.json`.
  - lens: `/api/check`, `/api/check/{domain}`, `/api/meta`, `/docs`, `/api-docs/openapi.json` (`crates/lens/src/lib.rs:155-161`), `/og/{domain_png}`, `/r/{shortid}` and its sub-paths (`crates/lens/src/routes.rs:1570-1573`). V1 short IDs are 8 characters of `[0-9A-Za-z]` (`crates/lens/src/snapshot/shortid.rs:8`), the same shape as history entry IDs (`v2-history` R8). So a V1 ID can only be told apart by not being an entry.
- **IP enrichment goes over HTTP today.** The dns, tls, http and email modules enrich server addresses (org, network type) through `netray_common::enrichment::EnrichmentClient`, which requests `{base}/network/json?ip=` from the ifconfig-rs container (`crates/common/src/enrichment.rs:223`). lens's module configs name that base: `[modules.dns.backends.ip] url`, `[modules.tls.backends.ip] url` (each with `timeout_ms`, `max_concurrent`, `cache_ttl_secs`, `cache_capacity`), `[modules.http.enrichment] ip_url` and `[modules.email.backends] ip_url` (`crates/lens/tests/fixtures/lens.production.toml:72-135`). The standalone prism and tlsight configs carry the same `[backends.ip]`, spectra `[enrichment] ip_url`, beacon `[backends] ip_url`. After the one-container cutover no ifconfig-rs host exists.
- **The error shape.** `{"error": {"code", "message"}}` (`specs/rules/architecture-rules.md:18-21`).
- **Metrics today, per service.** `lens_*`, `prism_*`, `tlsight_*`, `spectra_*`, `beacon_*`, `ifconfig_*`, plus `{prefix}_rate_limit_hits_total` (`crates/common/src/rate_limit.rs:44`). Each service has its own metrics listener: `[server] metrics_bind` for dns, tls, http, email and lens; ifconfig's is `[server] admin_bind` (default `127.0.0.1:9090`) with an optional bearer `admin_token` (`crates/ip/src/config.rs:82-116`, `crates/ip/src/lib.rs:203`). CLAUDE.md: `describe_*!` alone renders no HELP line, so metrics are also registered at startup. `metrics_util` 0.20's `Snapshotter` zeroes on read.
- **Frontends.** Root npm workspaces `crates/*/frontend` and `packages/common-frontend` (`package.json`). Seven `package.json` depend on the Solid toolchain. `just adlc-verify` runs `check-frontend-dist`, `test-frontend`, `validate-site` and `check-sitemap` (`justfile:10`, `:31-41`, `:83-103`). `test_suite_size.sh:6-7` floors Rust tests at 1650 and frontend tests at 171.
- **Image and release.** `Dockerfile`: a node stage builds six `frontend/dist`, the image copies six example configs and `site/`, `ENTRYPOINT ["netray"]`. `.github/workflows/release.yml:100-111` smoke-tests every subcommand from the image. `specs/rules/workflow-rules.md:76`, `:81` (R-R4) say "the subcommand selects the service" and "every subcommand is smoke-tested". A workflow change is verified by its first CI run or release tag (CLAUDE.md).
- **Acceptance.** Playwright projects in `tests/acceptance/playwright.config.ts:19-40`. `prod` is the fail2ban-safe subset, guarded by `tests/repo/test_acceptance_prod_subset.sh`. `fixtures/env.ts:3-31` holds seven origins, `lens` included. `just acceptance-local` starts seven processes (`justfile:113-150`).
- **Release.**
  - `just release X.Y.Z` refuses a dirty tree or a lower version. It then rewrites `[workspace.package] version`, inserts `## [X.Y.Z] - date` under `## [Unreleased]`, commits `chore: release vX.Y.Z` and signs the tag `vX.Y.Z`. It never pushes (`justfile:153-177`).
  - The workspace is at `0.26.0` (`Cargo.toml:6`). `## [Unreleased]` already holds the Phase 4a entry (`CHANGELOG.md:8-16`).
  - `fetch.pruneTags=true` is set globally, so no `git fetch` may run while the tag is unpushed (operator memory).
- **SDD.** `specs/sdd/v2.md`:
  - S1, S3, S10, S11, S24, S37 and S38 (§2);
  - §3.1 (crates table);
  - §5.1 (reserved paths: no `/health`, `/ready`);
  - §7.2–7.4, §9.1–9.2;
  - the §10 phase table (Phase 2 → `0.27.0`, 3 → `0.28.0`, 4 → `0.29.0`, 4a "expected `0.27.0`");
  - V0.3 ("the second immediately before Phase 2 deploys");
  - Phase 5, V5.1–V5.6.

  Amend it in the same commit as the code that changes what it says (CLAUDE.md).
- **Operator decision 2026-10-17.** Only `0.30.0` is released. `0.27.0`, `0.28.0` and `0.29.0` are never cut. Phases 2, 3, 4 and 4a ship together with Phase 5 in `0.30.0`.
- **Gate.**
  - `just adlc-verify` is the gate, offline; `just check` is the full suite.
  - Test invalid input and unmatched paths through the router, not the handler.
  - `tests/repo/*` read `git ls-files`, so stage deletions before running them.
  - A protected test that is removed or weakened needs `ADLC-Test-Change: <path> -- <reason>` (AGENTS.md).

## Requirements

### V5.1: one subcommand, one process

1. `netray serve [<config>]` starts the product. The config file is the argument, else `NETRAY_CONFIG`, else defaults only.
   - `netray serve --check-config <path>` exits 0 printing `config ok: <path>`, or exits 1 printing `config error: <path>: <error>`.
   - The check opens no file the config names: no store, no GeoIP or reputation data. The IP data reload on SIGHUP is kept.
2. One listener at `[server] bind` serves every host by `Host`. The host router lives in `netray-web` (X1). `serve` in `crates/netray` builds the module registry, the store and the real `InspectorJobs`, and hands them to netray-web. It owns no route. `/health` and `/ready` match first (requirement 3). For every other path, the port is stripped and the name compared ASCII-lowercase against `[server] apex` (default `"netray.info"`):
   - the apex serves §5.1's routes, `/api/v1` (§7.1) and the static pages: `pages::router()`, the result, evidence, `/r/` and inspector routes, and API v1, all from netray-web;
   - `dns.`, `tls.`, `http.` and `ip.` serve their inspector (§6.2, S3) at `/`, through the same handler as the apex's `/inspect/<tool>` (R33), plus the rows of requirements 13–14;
   - `email.` and `lens.` serve only the rows of requirement 13;
   - `mta-sts.` serves only `/.well-known/mta-sts.txt` and answers 404 for every other path except `/health` and `/ready`, as `crates/netray/src/site.rs:78-85` does today;
   - any other `Host`, or none, answers 404 on every path but `/health` and `/ready`. `www.` stays with Traefik (operator, 2026-10-17).

   `[server] metrics_bind` is a second listener that serves only `/metrics`, without a bearer token, as lens's is today. It replaces ifconfig's `[server] admin_bind`; `admin_token` has no successor, and the operator keeps the listener on a private address (requirement 11).
3. `/health` and `/ready` answer before host routing, on any `Host` header or none: the apex, every subdomain of requirement 2 including `lens.`, `email.` and `mta-sts.`, an unknown name, and `127.0.0.1:<port>`. They are never redirected and never answer 404 for the host. `/health` answers 200; `/ready` answers 200 once `serve` is ready. Container health checks probe `http://127.0.0.1:<port>/health` (netray-ops `lens-compose.yml.j2:45`), and Traefik's health checks and release.yml's smoke step address hosts, so all keep working. SDD §5.1 does not list them today: this spec reserves them, and Phase 2 amends §5.1 to list them (default, not interviewed).

   The apex answers §5.1's reserved paths with their own routes. `v2-pages` deferred these to this feature:
   - `/favicon.svg` answers 200 with the bytes of `/static/favicon.svg`;
   - `/favicon.ico` answers 404;
   - `/bimi-logo.svg` answers 200 from `crates/web/static/bimi-logo.svg`, moved from `site/` unchanged;
   - every apex path under `/.well-known/` answers 404, because the MTA-STS policy is served on `mta-sts.` only.

   These are defaults, not interviewed. A router test asserts that every reserved segment and root file of §5.1, as amended by `v2-pages` (`guide`, `privacy`), never reaches the result page.
4. `netray --help` lists `serve` only. `netray lens`, `dns`, `tls`, `http`, `email`, `ip` and `site` exit non-zero as unknown subcommands.
5. Before anything is deleted, whatever the Phase 2–4 features left in `crates/lens` and the `serve` path still calls moves to the crate §3.1 assigns it, unchanged in behaviour:
   - orchestration (`check.rs`, the run task's run step) to `netray-engine`;
   - recording to `netray-store`;
   - to `netray-web`: routes, templates, badge and OG; admission (`admission.rs`: in-flight map, throttle, per-client limiter, slots, `Sec-Purpose`), which §3.1 lists as "bulkheads, rate limits"; the `DomainRuns` implementation; the entry and domain OG card renderers lens hands to netray-web; `input::validate_domain`; `client_ip_from`; lens's middleware stack in its layer order;
   - the module registry (`lens_registry`) and the real `InspectorJobs` to `crates/netray`.

   Its tests move with it, each move under `ADLC-Test-Change`. Phase 1's inventory names each item and its target, starting from the Context list. Only one `/privacy` route remains, on netray-web's router.
6. The following are gone from `git ls-files`:
   - all of `crates/lens/`, including its snapshot store, moka cache, SPA, OG and badge, `migrations/`, tests and `tests/fixtures/contracts/lens-*.json`;
   - `crates/*/frontend/`, `packages/` and `site/` (with `site/og/` and `site/scripts/build-sitemap.mjs`; `bimi-logo.svg` moves first, requirement 3);
   - `crates/netray/src/site.rs`;
   - in `crates/{dns,tls,http,email,ip}`: the router, `run(config)`, the per-service `Config` with its loader, its `*.dev.toml`/`*.example.toml`/`beacon.toml`, and the V1 response types and contract goldens that only the deleted routers and lens used.

   The module crates keep their `Module` implementation, inspector functions and module config. No `package.json` or `Cargo.toml` names `solid-js` or `@netray-info/common-frontend`. None of the module crates (`dns`, `tls`, `http`, `email`, `ip`) exposes `pub async fn run`, and `crates/lens` is gone; `netray-engine`'s orchestrator `run` (`crates/engine/src/lib.rs:261`) stays. The root `package.json` workspaces name only `crates/web/islands` (`v2-pages`).
7. Every gate script and recipe that started a service starts `netray serve` instead, and none writes into the checkout (`git status --porcelain` empty after `just adlc-verify`).
   - `just acceptance-local` starts one `netray serve` on a free port with a temporary store and data directory, and passes its origin and the apex name to the suite. The store is seeded by `v2-inspectors`' `cargo run -p netray-store --example seed` over `tests/acceptance/fixtures/history/`, and the seven `LOCAL_<NAME>_URL`, `LOCAL_LENS_URL` included, collapse into that one origin. `v2-result-page`'s `LENS_EXTRA` pass-through becomes `ACCEPTANCE_ENV` with `NETRAY_` keys (its screenshot run: `ACCEPTANCE_ENV="NETRAY_ADMISSION__CLIENT_BUDGET_PER_HOUR=0"`), since requirement 10 refuses every `LENS_` variable.
   - `validate-site` and `check-sitemap` leave `adlc-verify`, because `v2-pages` generates the sitemap in netray-web and tests it there. `check-frontend-dist` checks only `crates/web/islands/dist`.
   - netray-ip's gate test of the curl root (`v2-api`, `justfile:52`) drives `serve`'s router with `Host: ip.<apex>` instead of the `netray ip` router.
   - The `tests/repo` scripts written for the subcommands, the Solid apps or lens are rewritten for `serve`, or removed when their subject is gone: `test_cli`, `test_smoke_services`, `test_header_parity`, `test_check_config`, `test_site`, `test_env_prefixes`, `test_service_libs`, `test_service_config_and_metrics`, `test_history_db_path`, `test_lens_admission_metrics`, `test_lens_names_no_module`, `test_no_presentation` (it targets `crates/lens/src`), `test_npm_workspaces`, `test_frontend_toolchain_majors` (islands only), `test_checkmeta_catalogue`, `test_no_raw_html_sinks`, `test_meta_schema`, `test_suite_size` (floors re-set to the counts after deletion), `test_config_strict`, and `test_web_crate` (any guard still in place lifted, the host router's module allowed, X1). Each change or removal carries `ADLC-Test-Change: <path> -- <reason>`.
8. The `Dockerfile` builds no Solid frontend. The image carries `netray.example.toml` and no `site/`, and its entrypoint runs `netray serve`. `release.yml` smoke-tests the built image by starting `serve` once and requesting `/health` with the apex `Host` and with each of `dns.`, `tls.`, `http.` and `ip.`. `specs/rules/workflow-rules.md` lines 76 and 81 (R-R4) say "`netray serve`, every host" instead of "every subcommand". The image still carries no data files (`test_image_data.sh`, release.yml data steps unchanged).
28. IP enrichment runs in-process (numbered 28 so that existing references stay stable). The module registry the binary builds (`lens_registry`, used by `serve` and, until Phase 7, by `netray lens`) hands the dns, tls, http and email modules an enrichment client that calls the registered ip module's lookup in-process: no HTTP request, no base URL, no `ip_url`. Without a registered ip module, enrichment is off, as an empty `ip_url` is today.
   - The keys `[modules.dns.backends.ip]`, `[modules.tls.backends.ip]` (with `timeout_ms`, `max_concurrent`, `cache_ttl_secs`, `cache_capacity`), `[modules.http.enrichment]` and `[modules.email.backends] ip_url` leave the module configs and join the legacy table of requirement 11 as removed in 0.30.0, the message saying that enrichment calls the in-process ip module. The `Transport::Plain` path of `EnrichmentClient` that only these callers used goes in Phase 7.
   - A test builds `serve`'s application with the ip module registered over its `testing` stub data and runs one domain whose stub section reports a server address. The run's server `org` and network type are filled; with the ip module unregistered they are absent and the run still completes.

### V5.2: one config

9. One config type in `crates/netray/src/config.rs` (default, not interviewed) loads through `netray_common::config::load` with prefix `NETRAY_`, as in `NETRAY_SERVER__BIND`. `NETRAY_CONFIG` names the file. Its top-level tables:
   - `[server]`: `bind`, `metrics_bind`, `trusted_proxies`, `apex`;
   - `[engine]`: `resolve_timeout_ms`, `profile_path`;
   - `[store]`: `enabled`, `db_path`, from `v2-history`'s `[history]`;
   - `[admission]`:
     - `client_budget_per_hour` and `domain_run_slots` (`v2-admission`);
     - `per_ip_per_minute`, `per_ip_burst`, `global_per_minute` and `global_burst`, from `[rate_limit]`;
     - `cache_enabled` and `cache_window_seconds`, from `[cache] enabled` and `ttl_seconds`.

     The key names are defaults, not interviewed.
   - `[api]` (`enabled`, `per_ip_per_minute`, `per_ip_burst`) and `[api.dns]`, `[api.tls]`, `[api.http]`, `[api.email]` and `[api.ip]`, as `v2-api` defined them (operator, 2026-10-17);
   - `[inspect.dns]`, `[inspect.tls]`, `[inspect.http]` and `[inspect.ip]` (`deadline_ms`, `max_concurrent`), as `v2-inspectors` defined them;
   - `[badges]`, `[og_cards]`, `[telemetry]`;
   - `[modules.<name>]`, holding each module's config and its section window `deadline_ms`, which was `[backends.<m>] timeout_ms`;
   - any further table a Phase 2–4 feature added to lens's config that Phase 1's inventory finds, under its own name.

   Every key keeps its default and its `validate()` rule and `startup_rejects` row.

   Every struct carries `deny_unknown_fields`, and `test_config_strict.sh` holds it. SDD S10 and §3.4 list the final set of tables.
10. Before loading, `serve` and `serve --check-config` refuse with exit 1 when any variable starts, ASCII case-insensitively, with one of `LENS_`, `NETRAY_DNS_`, `NETRAY_TLS_`, `NETRAY_HTTP_`, `NETRAY_EMAIL_`, `NETRAY_IP_`, `PRISM_`, `TLSIGHT_`, `SPECTRA_`, `BEACON_` or `IFCONFIG_`. Each prefix is checked through `refuse_legacy_prefix(<legacy>, "NETRAY_", …)`, whose message names the variable, `NETRAY_*` and `NETRAY_CONFIG`. If live reference tests survive, their switch is `NETRAY_LIVE_TESTS`, filtered out of config keys as `LENS_LIVE_TESTS` is today.
11. A config file holding a key path of the legacy table is refused at load and by `--check-config` with exit 1, before deserialization. The message names the key path and says where its value lives now, or that it was removed in 0.30.0, and points to the CHANGELOG's 0.30.0 configuration table. Where the same key path came from several old files with different fates, the message names each origin's destination (for example `[cache] enabled`: lens's → `[admission] cache_enabled`, ifconfig's → removed). The legacy table lives once in code (`crates/netray/src/legacy.rs`, default, not interviewed) and covers at least these rows, by origin:
   - `backends`:
     - lens: `resolve_timeout_ms` → `[engine] resolve_timeout_ms`; `<m>.timeout_ms` → `[modules.<m>] deadline_ms`; the `url` keys were removed with the HTTP fan-out (V1.4);
     - prism and tlsight `[backends.ip]`, beacon `[backends] ip_url` and `timeout_ms`: removed, enrichment is in-process (requirement 28).
   - `cache`:
     - lens: `enabled` → `[admission] cache_enabled`, `ttl_seconds` → `[admission] cache_window_seconds`. The history is the cache (S17), and moka is gone;
     - ifconfig (`enabled`, `ttl_secs`, `max_entries`): removed with ifconfig's routes, its only reader (`crates/ip/src/routes.rs:336`, `state.rs:185-186`).
   - `rate_limit`:
     - lens: `[admission]`, under the same four key names;
     - ifconfig (`per_ip_*`, `per_target_*`, `exempt_cidrs`): `[api.ip]`, which holds ifconfig's limits (`v2-api` requirement 25);
     - beacon (`per_ip` and its other keys): removed with `email.`'s `/inspect` routes; API v1's email endpoint is limited by `[api]`.
   - `scoring`: `[engine] profile_path`.
   - `history`: `[store]`.
   - `snapshots`: removed (V5.6).
   - `site`, `ecosystem`, `site_name`, `meta`, `base_url`, `project_name`, `project_version`: removed with the Solid apps.
   - spectra's top-level `inspect` with its keys `request_timeout_secs`, `total_timeout_secs`, `max_redirects`, `user_agent` → `[modules.http.inspect].<key>`. Only these four key paths are legacy: the new top-level `[inspect.<tool>]` tables (requirement 9) are not, and their tool names never collide with these keys.
   - `limits`, `circuit_breaker`, `trace`, `validation`, `quality`, `enrichment`, `dns`, `dnsbl`, `http`, `dkim`, `batch`, `filtered_headers`, `internal_mode`, `watch_data_files`, `dns_base_url`, `tls_base_url`, and every path key of ifconfig's `Config`: `geoip_city_db`, `geoip_asn_db`, `user_agent_regexes`, `tor_exit_nodes`, `cloud_provider_ranges`, `feodo_botnet_ips`, `cins_army_ips`, `vpn_ranges`, `datacenter_ranges`, `bot_ranges`, `spamhaus_drop`, `asn_patterns`, `asn_info`. The message names `[modules.<m>].<key>` when exactly one module's config has a field of that name, otherwise points to the CHANGELOG table, which gives each key's fate (`user_agent_regexes`, absent from ip's `ModuleConfig`, included).
   - `[server] admin_bind` (ifconfig) → `[server] metrics_bind`; `[server] admin_token` → removed, the metrics listener has no token (requirement 2).
   - `[modules.dns.backends.ip]`, `[modules.tls.backends.ip]`, `[modules.http.enrichment]`, `[modules.email.backends] ip_url`: removed, enrichment is in-process (requirement 28, added to the table in Phase 2).

   Apart from the key paths named above, `[server]`, `[telemetry]`, `[badges]`, `[og_cards]`, `[admission]`, `[api]`, `[inspect]` and `[modules]` are not legacy. Any other old key inside them that the new type lacks fails through `deny_unknown_fields`.
12. `crates/netray/netray.example.toml` holds every top-level table with every key at its default, commented. `crates/netray/netray.dev.toml` is the dev config. `--check-config` exits 0 on both. Startup refusals keep their tests: an unopenable `[store] db_path` and missing GeoIP files exit non-zero at startup while `--check-config` on the same file exits 0. They become `startup_rejects` rows of `test_check_config.sh` for `serve`.

### V5.3: redirects and 410

13. Each row below answers as stated, with one test row per line through the router. Redirects apply to GET and HEAD; a `410` applies to any method. Every `Location` is absolute, `https://<apex>…`. Every `410` from an `/api` or `/api-docs` path carries the error shape of `architecture-rules.md` with code `gone` and a message naming the successor URL (code name default, not interviewed). `/health` and `/ready` match before every row (requirement 3), on every host.
    - a. Host `lens.<apex>`, any path and query except `/health` and `/ready` → 301 `https://<apex><path>[?<query>]` (§7.3; answered by netray-web in the binary, operator, 2026-10-17).
    - b. Apex `/?d=<value>` → 301 `https://<apex>/<domain>`, the value normalised and percent-encoded as `v2-pages` requirement 15 does. Any other value answers `400` with the 404 page's query input refilled (`v2-pages` requirement 15, its scenario of the 400). `v2-pages` already answers this with its exported `?d=` handler; this feature mounts it on the apex router, makes its `Location` absolute, and adds the rows.
    - c. `email.<apex>/?q=<domain> <selectors…>` → 301 `https://<apex>/<domain>/email`, from the first whitespace token of the decoded `q`. Selectors are dropped.
    - d. `email.<apex>/?domain=<domain>` → the same. When both are present, `q` wins (`crates/beacon/frontend/src/App.tsx:58` as cited by §7.3).
    - e. `email.<apex>/` without `q` or `domain`, with an unparsable domain, or on any path not covered by f–h, `/health` and `/ready` excepted, → 301 `https://<apex>/` (the unparsable and other-path cases are defaults, not interviewed).
    - f. `/api/*` on `dns.`, `tls.`, `http.`, `email.` or `ip.` → 410 naming `https://<apex>/api/v1`.
    - g. `email.<apex>/inspect` and `/inspect/<domain>` → 410 naming `https://<apex>/api/v1/email/inspect`.
    - h. `/docs` and `/docs/` on `dns.`, `tls.`, `http.`, `email.` and `ip.` → 301 `https://<apex>/api/v1/docs`, `v2-api`'s interactive docs page over `/api/v1/openapi.json`. `/api-docs/openapi.json` on the same hosts → 410 naming `https://<apex>/api/v1/openapi.json`. This replaces §7.2's 410 for `ip.`'s `/docs` and extends the rule to every subdomain (default, not interviewed).
    - i. Apex `/api/check` and `/api/check/<domain>` → 410 naming `https://<apex>/api/v1/domains/{domain}/runs`. Any other apex `/api/<x>` with `x` ≠ `v1` (for example `/api/meta`) → 410 naming `https://<apex>/api/v1` (default, not interviewed). Row m comes first.
    - j. Apex `/r/<id>`, `/r/<id>/og.png` and `/r/<id>/<protocol>` for a well-formed ID that is no stored entry → 410. The page routes answer with the §4.6 page, which names no domain, and `og.png` with an empty body (as `v2-entries` does). A V1 snapshot ID is such an ID. `v2-entries`' V1-snapshot step is removed. A malformed ID still answers 404.
    - k. `dns./?q=…`, `tls./?h=…`, `http./?url=…` and `ip./?ip=…` answer 200 with the inspector (no redirect, no 410). `dns./?r=<key>` answers 200 and ignores `r`.
    - l. Apex `/tools` and `/tools/` (the V1 tool list, `site/tools/index.html`) → 301 `https://<apex>/` (default, not interviewed).
    - m. Apex `/api/` and `/api/<name>.html` for `index`, `dns`, `tls`, `http`, `email`, `ip` and `lens` (the V1 API docs, `site/api/*.html`) → 301 `https://<apex>/api/v1/docs`. They are human pages, so they redirect and do not answer a JSON 410 (default, not interviewed).
    - n. Apex `/guide/lens` → 301 `https://<apex>/guide/scoring` (`v2-pages`).
    - o. Apex `/docs` and `/docs/` (lens's V1 docs, reached from `lens.` through row a) → 301 `https://<apex>/api/v1/docs`; apex `/api-docs/openapi.json` (lens's V1 OpenAPI) → 410 naming `https://<apex>/api/v1/openapi.json` (default, not interviewed).
14. On `ip.`, every route of `crates/ip/src/routes.rs:107-166` other than `/`, `/health`, `/ready` and `/docs` (row 13h) answers 410 naming its successor (§7.2, R35):
    - `/json`, `/yaml`, `/toml`, `/csv`, `/all[/{fmt}]`, `/ip[/{fmt}]` → `/api/v1/ip`;
    - `/ip/cidr` and `/range` → `/api/v1/ip/range/{cidr}`;
    - `/asn[/{param}]` → `/api/v1/asn/{n}`;
    - the field family `tcp location user_agent network host isp country city timezone latitude longitude region headers`, bare and `/{fmt}` → `/api/v1/ip?field=<name>`, the caller form, which `v2-api` answers for every name of the family (address-derived names also on `/api/v1/ip/{address}?field=`; the caller-dependent `tcp`, `user_agent`, `host` and `headers` only on the caller form). The bare forms answer 410 as well, because S23 moved "one field as text" into the API (operator, 2026-10-17);
    - `/ipv4[/{fmt}]` and `/ipv6[/{fmt}]` → `/api/v1/ip/{address}`, the message also naming `curl -4`/`curl -6` against the root;
    - `/batch[/{fmt}]` → `/api/v1/ip/batch`;
    - `/diff` → `/api/v1/ip/diff`;
    - `/meta` and `/api/meta` → `/api/v1/meta`;
    - `/api-docs/openapi.json` → `/api/v1/openapi.json`; `/docs` redirects per row 13h.

    A curl user agent on `/` still gets the caller's address as text, and a browser gets the inspector (V3.3, held here as a neighbour row). The successors where §7.2 names none are defaults, not interviewed.
15. The tests of requirements 13–14 are table-driven. A repo test fails when a route of the deleted V1 routers listed in Context (the §7.2/§7.3 sources), or a `site/` path of rows l–n, has no row. The list is kept in the test, because the source files are deleted.

### V5.4: metrics

16. Every metric of §9.2 is registered at startup with its HELP line and the label names §9.2 gives. The exposition shows each name before the first request.
17. Every metric name `serve` exports starts with `netray_`. No `lens_`, `prism_`, `tlsight_`, `spectra_`, `beacon_` or `ifconfig_` name remains (S11). An operational series that survives the deletion (rate-limit hits, inspector durations, config reloads) is renamed `netray_<suffix>` (default, not interviewed). From `v2-admission`, the renames are:
    - `lens_admission_rejections_total{reason}` → §9.2's `netray_admission_rejections_total{reason}`;
    - `lens_runs_in_flight` → `netray_runs_in_flight`;
    - `lens_history_record_errors_total` → `netray_history_record_errors_total`;
    - `lens_check_requests_total{result}` and `lens_badge_requests_total` end, succeeded by `netray_runs_total` and the endpoint counters.

    `test_lens_admission_metrics.sh` is rewritten for the new names. The CHANGELOG lists every ended series and its successor, if any.
18. A registry test drives one stub run of `example.com` (address `192.0.2.1`) through page, run, badge, OG, `/api/v1` and one inspector job, then reads the exposition. It asserts:
    - no label name is one of `domain`, `host`, `address`, `ip`, `client`, `target`, `url`, `query`, `name`, `id`;
    - no label value equals `example.com`, `192.0.2.1` or an entry ID;
    - every `endpoint` value is a route template, never a path.
19. The §9.2 counters mean what §9.2 says, each held by a test:
    - `netray_runs_total{outcome,trigger}` counts every run, with `trigger` one of visit, recheck, api, badge or og (M1);
    - `netray_runs_with_predecessor_total` counts recorded runs with a predecessor;
    - `netray_run_interval_seconds` is observed when a run is recorded;
    - `netray_fixes_observed_total{section}` counts fixed transitions in recorded diffs;
    - `netray_admission_rejections_total{reason}` uses the reasons throttle, budget, limit and slots;
    - `netray_history_entries` and `netray_history_domains` are set at startup and after every sweep.

    A test written by a sibling feature counts for this. This requirement adds only the missing ones.

### V5.5: agreement

20. A Rust test in `crates/netray/tests/agreement.rs` (default, not interviewed) builds the `serve` application with stub modules and a temporary store. It runs four states:
    - a complete run with warn and fail checks;
    - a run with a fixed check against a stored predecessor;
    - N/A by design (no MX);
    - not tested from here.

    For each, it reads the run through the result page, the open issues, every evidence view, the badge, the OG card's geometry and `/api/v1`. It asserts the same grade, the same status per check ID, and the same set of warn and fail checks. It runs in `just adlc-verify`, offline.
21. The same test runs an incomplete state, where one section times out after a stored entry exists. The stream and `/api/v1`'s run answer both say `incomplete` with the same statuses. Badge, OG card and `GET /api/v1/domains/{domain}` still show the stored entry, byte-for-byte unchanged (SC12, V2.5).
22. `tests/acceptance` gains `agreement/prod.spec.ts`. For `netray.info` and `example.com`, it starts one run each through `POST /api/v1/domains/{domain}/runs` without streaming, then asserts:
    - the page grade equals the API grade equals the badge letter;
    - the open issues equal the API's warn and fail checks;
    - every evidence view names the same entry.

    It uses only request contexts on known paths and no browser navigation that could post a run. The `prod` project is that spec plus `smoke/health`, `smoke/ready`, `smoke/security-headers` and `smoke/tls-certs`, and `test_acceptance_prod_subset.sh` holds the new list. `fixtures/env.ts` drops the `lens` origin: production is the apex plus the four inspector hosts of one process, and local is one origin addressed by `Host`. Specs whose subject is gone (`services/*.spec.ts` of the V1 APIs, `api-contract` against `/api/meta`, `integration/cross-links`, `static-site` against `site/`) are removed or rewritten for V2. `just acceptance-local agreement/prod.spec.ts` runs the spec against a local `serve` with network. That is the local equivalent before deploy, and it is not part of the gate. The production run is the operator's, after the deploy.

### V5.6: snapshots

23. No tracked file outside `specs/` and `CHANGELOG.md` names `snapshots.db`, `[snapshots]` (except the legacy table of requirement 11) or the snapshot store. lens's `nanoid` and `sqlx` dependencies go with the crate, unless another crate uses them. V1 `/r/` IDs answer per 13j.

### Release, documentation, SDD

24. `CHANGELOG.md` `## [Unreleased]` states 0.30.0:
    - a lead paragraph saying this is the breaking release (one process, one config, every V1 API and app gone) and that it carries Phases 2–4, 4a and 5, with no 0.27–0.29;
    - `### Removed`: every subcommand, every endpoint of requirements 13–14 with its successor (§7.4), the Solid apps, `site/`, the snapshot store, the per-service config files and env prefixes;
    - `### Changed`: the config migration table of requirement 11 (old key → `netray.toml` key, or removed), the env prefix, and the metrics renames of requirement 17;
    - the sibling features' entries, kept and checked.

    Each claim is traced to code at the release commit, not to specs or the SDD (CLAUDE.md "Release notes trace to code"), and `report.md` records it as a claim → path table. Only `netray.info` and `example.com` appear.
25. `specs/sdd/v2.md` is amended:
    Only S37, S38 and the §10 table carry release numbers for Phases 2–4a; the Phase 2 intro names none and stays.
    - S37: "Operator, 2026-10-17: only `0.30.0` is released after `0.26.0`; `0.27.0`, `0.28.0` and `0.29.0` are not cut; Phases 2, 3, 4 and 4a ship with Phase 5 in `0.30.0`."
    - S38: 4a ships with `0.30.0`.
    - The §10 table's Release column: 4a, 2, 3 and 4 → "none of its own; ships in `0.30.0`".
    - V0.3: "the second immediately before `0.30.0` deploys, which deploys Phase 2".
    - The Phase 5 table marks V5.1–V5.6 as built by `v2-cutover`, with three operator steps: V5.2's container half, V5.5's production half, and V5.6's file half (deleting `snapshots.db` on argus after the second pull). V5.6's code half, V1 `/r/` IDs answering `410` and the snapshot code removed, is marked built.
    - §9.2's `netray_runs_total` trigger list reads visit, recheck, api, badge, og. Per M1 this is `v2-admission`'s amendment, the feature that emits the triggers; Phase 8 confirms it stands and writes it only if it is missing.
    - S24 stays unchanged: 0.30.0 is a `0.x` minor.

    The code-bound amendments land with their phases: S10/§3.4 (requirement 9), §5.1 reserved paths, including `/health` and `/ready`, which §5.1 does not list today (3), §7.3 rows h and i and the unknown host (2, 13), §9.1's production run after deploy (22), §3.1 without `crates/lens` (6).
26. `CLAUDE.md` is corrected wherever the cutover makes it false:
    - **"What this is":** six services, a shared frontend package and a static site → one process, `netray serve`.
    - **Layout:**
      - drop the `crates/lens`, `packages/common-frontend` and `site/` rows;
      - `crates/{dns,tls,http,email,ip}` become module crates with inspector functions, no frontend;
      - `crates/netray` has the `serve` subcommand only, with `--check-config`;
      - add `crates/store` and `crates/web` if they are still missing.
    - **Verbs:**
      - drop "crates embed `frontend/dist`", the frontend `tsc` line and the `common-frontend` types line (replaced by whatever the islands need);
      - "`netray ip` gets `/netray/data`" → `netray serve`;
      - `just acceptance-local` starts one process.
    - **Conventions:**
      - "Services are libraries" → module crates plus a composition root;
      - "Config stays per service" → one `netray.toml`, `NETRAY_`, legacy prefixes and tables refused (requirements 9–11);
      - "Contract goldens" and "lens goldens" → removed with the V1 APIs and lens;
      - "Carry new internal data on V1 response structs" → removed;
      - "A new check id needs … `CHECK_LABELS` in `checkMeta.ts`" → catalogue only;
      - "Data files … as `netray ip`" → `netray serve`;
      - "A service moved in-process keeps every check its route applied to lens's calls" → removed;
      - "Crate docs point at … `netray <subcommand>`" → `netray serve`;
      - "Features unify … every service" → every module;
      - "beacon orders verdicts … lens" → reworded without lens.
    - **Rules table:** the `frontend-rules.md` trigger loses `packages/common-frontend` and `crates/*/frontend`.

    `README.md` (layout, run, release lines 3, 9, 11, 32, 36) and `specs/rules/architecture-rules.md:10-11` (env prefixes) follow, as does `specs/rules/logging-rules.md` wherever it names per-service targets or `lens` backends: the services-in-scope line (`:7`), A3 (default filter, `{crate}` examples), A5 (metrics port), A7 (service names), B3 (the service table), C3 (inter-service calls, which end with in-process enrichment) and the C-section intro on lens calling prism.
27. After `adlc feature finish` merges onto `main`, `just release 0.30.0` runs on `main`. It sets the version to `0.30.0`, inserts `## [0.30.0] - <date>`, commits and signs the tag `v0.30.0`. Nothing is pushed, and no `git fetch` runs until the operator has pushed the tag.

## Phases

One branch; review and finish run once, after Phase 8. Requirement 27 runs on `main` after finish and is no phase.

## Phase 1 — `netray serve` and `netray.toml`

**Depends on:** lanes A (v2-api may still be open) and B merged; Phases 1–5 start now on main and rebase as v2-api, v2-inspectors and v2-result-page land; Phase 6 waits until all eight are merged (operator, 2026-10-18, X6)
**Requirements:** 1, 9, 10, 11, 12

`crates/netray`: the `serve` subcommand beside the old ones (they go in Phase 7), `config.rs`, `legacy.rs`, `netray.example.toml`, `netray.dev.toml`, and serve's `--check-config`. `report.md` records the inventory of what the Phase 2–4 features left in `crates/lens` and in the services' configs, item by item with its target crate. It starts from the Context list (`[admission]`, `[rate_limit]`, `[cache]`, `[api]` and `[api.<inspector>]`, `[inspect.<tool>]`, `[history]`, `admission.rs`, the `DomainRuns` implementation, the V1 snapshot lookup and card renderers lens hands to `EntriesDeps`, `InspectorJobs`'s real implementation, `validate_domain`, `client_ip_from`, the moka V1 holder, the `lens_*` series), and the legacy table is built from it. SDD S10 and §3.4.

### Test Scenarios

GIVEN `netray.example.toml` WHEN `netray serve --check-config` runs on it THEN it exits 0 printing `config ok: <path>`.
GIVEN the same file with `[server] binds = "x"` WHEN checked THEN it exits 1 naming `binds`.
GIVEN `NETRAY_SERVER__BIND=127.0.0.1:<port>` WHEN `serve` starts THEN it listens on that port.
GIVEN `NETRAY_MODULES__IP__GEOIP_CITY_DB=/x` WHEN the config loads THEN `[modules.ip]` holds it and no legacy refusal fires.
GIVEN `LENS_SERVER__BIND` set WHEN `serve` or `serve --check-config` runs THEN it exits 1 naming `LENS_SERVER__BIND`, `NETRAY_*` and `NETRAY_CONFIG`; the same for one variable of each other legacy prefix of requirement 10.
GIVEN `NETRAY_DNS_SERVER__BIND` set WHEN `serve` starts THEN it exits 1 naming it (the old module prefix is refused although it starts with `NETRAY_`).
GIVEN a config file with `[snapshots] enabled = true` WHEN checked THEN it exits 1 saying `snapshots` was removed in 0.30.0.
GIVEN a config file with `[history] db_path = "h.db"` WHEN checked THEN it exits 1 naming `[store] db_path`.
GIVEN a config file with `[cache] ttl_seconds = 300` WHEN checked THEN it exits 1 naming `[admission] cache_window_seconds`; GIVEN `[rate_limit] per_ip_burst = 3` THEN it names `[admission] per_ip_burst`.
GIVEN `[api.ip] batch_max_size = 50` and `[inspect.tls] deadline_ms = 15000` WHEN checked THEN it exits 0; GIVEN `[api.ip] batch_max_size = 101` WHEN `serve` starts THEN it exits 1 naming the key.
GIVEN a file with `geoip_city_db = "x"` at top level WHEN checked THEN it exits 1 naming `[modules.ip].geoip_city_db`.
GIVEN a file with top-level `[dns]` WHEN checked THEN it exits 1 pointing to the CHANGELOG table (the name is ambiguous across modules).
GIVEN a file with top-level `[inspect] max_redirects = 5` (spectra's) WHEN checked THEN it exits 1 naming `[modules.http.inspect].max_redirects`; GIVEN `[inspect.http] max_concurrent = 4` THEN it exits 0.
GIVEN `[server] admin_bind = "127.0.0.1:9090"` WHEN checked THEN it exits 1 naming `[server] metrics_bind`; GIVEN `[server] admin_token = "t"` THEN it exits 1 saying it was removed in 0.30.0.
GIVEN `[cache] ttl_secs = 60` WHEN checked THEN it exits 1 saying ifconfig's cache was removed in 0.30.0; GIVEN `[cache] enabled = true` THEN the message names lens's destination `[admission] cache_enabled` and ifconfig's removal.
GIVEN `[rate_limit] per_target_per_minute = 120` WHEN checked THEN it exits 1 naming `[api.ip]`.
GIVEN top-level `user_agent_regexes = "x"` WHEN checked THEN it exits 1 pointing to the CHANGELOG table, which states its fate.
GIVEN `[store] db_path` in a nonexistent directory WHEN `serve` starts THEN it exits non-zero naming the history store, and `--check-config` on the same file exits 0.
GIVEN `[modules.ip]` naming missing GeoIP files WHEN `serve` starts THEN it exits non-zero naming the file, and `--check-config` exits 0.

## Phase 2 — Host routing

**Depends on:** Phase 1
**Requirements:** 2, 3, 28

`crates/netray` (composition root) and `crates/web` (router): host dispatch over the routers the Phase 3–4 features built, the `mta-sts.` host, `/health` and `/ready` ahead of host dispatch, and the metrics listener. The in-process enrichment client in the binary's registry (requirement 28), with the enrichment keys leaving the module configs and joining the legacy table, before anything is deleted in Phase 7. SDD §5.1 reserved paths (adding `/health` and `/ready`), and §7.3's unknown-host line.

### Test Scenarios

GIVEN `serve` on one port WHEN `GET /` arrives with `Host: netray.info` THEN the idle page answers; WITH `Host: dns.netray.info` THEN the DNS inspector; the same for `tls.`, `http.` and `ip.`.
GIVEN `Host: NETRAY.INFO:443` WHEN `GET /` THEN the idle page answers (port stripped, case folded).
GIVEN `Host: mta-sts.netray.info` WHEN `GET /.well-known/mta-sts.txt` THEN 200 with the policy; WHEN `GET /` THEN 404.
GIVEN `Host: www.example.com` WHEN `GET /` THEN 404; GIVEN `Host: www.netray.info` THEN 404 too (Traefik's).
GIVEN `Host: dns.netray.info` WHEN `GET /?q=example.com` THEN the same markup as the apex's `GET /inspect/dns?q=example.com`.
GIVEN the apex WHEN `GET /favicon.svg` THEN 200 `image/svg+xml`; WHEN `GET /bimi-logo.svg` THEN 200; WHEN `GET /favicon.ico` or `GET /.well-known/mta-sts.txt` THEN 404, never a result page.
GIVEN every reserved segment and root file of §5.1 WHEN requested on the apex THEN none answers the result page.
GIVEN each `Host` of the apex, `dns.`, `tls.`, `http.`, `ip.`, `email.`, `lens.` and `mta-sts.netray.info`, `www.example.com`, `127.0.0.1:8082`, and no `Host` WHEN `GET /health` and `GET /ready` THEN 200 each, never a redirect or a 404.
GIVEN the composed router WHEN its route order is read THEN `/health` and `/ready` match before host dispatch and before row 13a's `lens.` redirect.
GIVEN `[server] apex = "localhost"` WHEN `GET /` arrives with `Host: dns.localhost` THEN the DNS inspector answers.
GIVEN the main listener WHEN `GET /metrics` THEN 404; GIVEN the metrics listener THEN 200.
GIVEN the ip module registered over its stub data WHEN a run reports a server address THEN the run's server org and network type are filled, and no HTTP request leaves the process; GIVEN no ip module THEN they are absent and the run completes.
GIVEN `[modules.http.enrichment] ip_url = "http://x"` or `[modules.dns.backends.ip] url = "http://x"` WHEN checked THEN it exits 1 saying the key was removed in 0.30.0 because enrichment is in-process.

## Phase 3 — Redirects and 410

**Depends on:** Phase 2
**Requirements:** 13, 14, 15

`crates/web`: the redirect and gone handlers per host, and the table-driven router test with one row per line of requirements 13–14. `tests/repo`: the coverage check of requirement 15. SDD §7.3 rows h and i.

### Test Scenarios

GIVEN `Host: lens.netray.info` WHEN `GET /r/abcd1234?x=1` THEN 301 `https://netray.info/r/abcd1234?x=1`.
GIVEN the apex WHEN `GET /?d=example.com` THEN 301 `https://netray.info/example.com`; WHEN `GET /?d=not%20a%20domain` THEN 400 with the query input refilled (`v2-pages`).
GIVEN `Host: lens.netray.info` or `Host: email.netray.info` WHEN `GET /health` THEN 200, no redirect.
GIVEN `Host: email.netray.info` WHEN `GET /?q=example.com%20google%20selector1` THEN 301 `https://netray.info/example.com/email`.
GIVEN `Host: email.netray.info` WHEN `GET /?q=example.com&domain=netray.info` THEN the Location names `example.com`.
GIVEN `Host: email.netray.info` WHEN `GET /` THEN 301 `https://netray.info/`.
GIVEN `Host: email.netray.info` WHEN `POST /inspect/example.com` THEN 410 with code `gone`, naming `/api/v1/email/inspect`.
GIVEN `Host: dns.netray.info` WHEN `POST /api/check` THEN 410 naming `/api/v1`; WHEN `GET /docs` THEN 301 `https://netray.info/api/v1/docs`; WHEN `GET /api-docs/openapi.json` THEN 410 naming `/api/v1/openapi.json`.
GIVEN `Host: lens.netray.info` WHEN `GET /docs` THEN 301 `https://netray.info/docs`; GIVEN the apex WHEN `GET /docs` THEN 301 `https://netray.info/api/v1/docs`; WHEN `GET /api-docs/openapi.json` THEN 410 naming `/api/v1/openapi.json`.
GIVEN the apex WHEN `GET /api/check/example.com` THEN 410 naming `/api/v1/domains/{domain}/runs`; WHEN `GET /api/v1/meta` THEN 200 (neighbour: v1 is not gone).
GIVEN a stored entry `e1` WHEN `GET /r/e1` THEN 200; GIVEN a well-formed ID that is no entry WHEN `GET /r/<id>` and `GET /r/<id>/tls` THEN 410, and the body names no domain; WHEN `GET /r/<id>/og.png` THEN 410 with an empty body; GIVEN `GET /r/x` THEN 404.
GIVEN the apex WHEN `GET /tools` THEN 301 `https://netray.info/`; WHEN `GET /api/dns.html` THEN 301 `https://netray.info/api/v1/docs`; WHEN `GET /api/meta` THEN 410; WHEN `GET /guide/lens` THEN 301 `https://netray.info/guide/scoring`.
GIVEN `Host: ip.netray.info` WHEN `GET /country` THEN 410 naming `/api/v1/ip?field=country`.
GIVEN `Host: ip.netray.info` WHEN each cut route of requirement 14 is requested THEN 410 naming its successor; WHEN `GET /` with a curl user agent THEN 200 `text/plain` with the caller's address.
GIVEN `Host: tls.netray.info` WHEN `GET /?h=example.com` THEN 200, no redirect.
GIVEN `Host: dns.netray.info` WHEN `GET /?q=example.com&r=k1` THEN 200 and `r` is ignored.
GIVEN the coverage check WHEN a V1 route is removed from the test table THEN the check fails naming it.

## Phase 4 — Metrics

**Depends on:** Phase 2
**Requirements:** 16, 17, 18, 19

`crates/web`, `crates/engine`, `crates/store` and the module crates: startup registration of §9.2, the `netray_` rename of surviving series, the missing counters, and the registry test.

### Test Scenarios

GIVEN `serve` started WHEN `/metrics` is read before any request THEN every §9.2 name has a HELP line.
GIVEN the exposition WHEN every name is listed THEN each starts with `netray_` and none with a V1 prefix of requirement 17.
GIVEN one stub run of `example.com` at `192.0.2.1` through page, run, badge, OG, API and an inspector job WHEN the exposition is read THEN no forbidden label name exists and no label value equals `example.com`, `192.0.2.1` or the entry ID.
GIVEN `GET /api/v1/domains/example.com` WHEN the exposition is read THEN `netray_api_requests_total` has `endpoint="/api/v1/domains/{domain}"`.
GIVEN a recheck that records a run with a predecessor and one fixed transition in section tls WHEN read THEN `netray_runs_total{trigger="recheck",outcome="complete"}`, `netray_runs_with_predecessor_total` and `netray_fixes_observed_total{section="tls"}` each rose by 1.
GIVEN a badge request that starts a run WHEN read THEN `netray_runs_total{trigger="badge"}` rose by 1; the same for an OG card with `trigger="og"`.
GIVEN a throttled run WHEN read THEN `netray_admission_rejections_total{reason="throttle"}` rose by 1.
GIVEN a sweep that deletes one domain WHEN read THEN `netray_history_domains` fell by 1.

## Phase 5 — Agreement

**Depends on:** Phase 3, Phase 4
**Requirements:** 20, 21, 22

`crates/netray/tests/agreement.rs` with stub modules. `tests/acceptance`: `agreement/prod.spec.ts`, the `prod` project, `env.ts`, and removal or rewrite of the V1 specs. `test_acceptance_prod_subset.sh` under `ADLC-Test-Change`. SDD §9.1.

### Test Scenarios

GIVEN each of the four complete states WHEN read through page, issues, evidence views, badge, OG geometry and `/api/v1` THEN grade, per-check statuses and the warn/fail set are equal across all.
GIVEN a stored entry and a run whose email section times out WHEN streamed THEN the stream and the API run answer say `incomplete` with equal statuses, while badge, OG and `GET /api/v1/domains/{domain}` equal their answers from before the run.
GIVEN the fixed-check state WHEN the page and `/api/v1/domains/{domain}/diff` are read THEN both name the same check as fixed.
GIVEN `playwright.config.ts` WHEN `test_acceptance_prod_subset.sh` runs THEN `prod` lists exactly the five specs of requirement 22.
GIVEN `agreement/prod.spec.ts` WHEN grepped THEN it navigates no page with a browser and names no domain other than `netray.info` and `example.com`.

## Phase 6 — Survivors out of lens

**Depends on:** Phase 5
**Requirements:** 5

Move what the inventory of Phase 1 found still called from `serve` out of `crates/lens`, to the crates §3.1 names. Tests move with their code. Behaviour is unchanged: Phase 5's agreement test and the sibling features' tests stay green on the new paths.

### Test Scenarios

GIVEN the moved code WHEN `just adlc-verify` runs THEN it passes, and the agreement test of Phase 5 is unchanged.
GIVEN `crates/netray/Cargo.toml` and `crates/web/Cargo.toml` WHEN read THEN neither depends on `lens`.

## Phase 7 — Deletion

**Depends on:** Phase 6
**Requirements:** 4, 6, 7, 8, 23

Delete everything listed in requirement 6, along with the old subcommands, `site.rs` and the Solid workspaces. Rewrite or remove the gate scripts, the recipes, the `Dockerfile` and `release.yml`'s smoke step. Amend `workflow-rules.md` R-R4 and SDD §3.1. Every protected test removed or changed carries `ADLC-Test-Change`.

### Test Scenarios

GIVEN `netray --help` WHEN read THEN it lists `serve` only; GIVEN `netray lens` WHEN run THEN it exits non-zero.
GIVEN `git ls-files` WHEN filtered THEN nothing lies under `crates/lens/`, `crates/*/frontend/`, `packages/` or `site/`, and `crates/netray/src/site.rs` is absent.
GIVEN every `Cargo.toml` and `package.json` WHEN read THEN none names `solid-js` or `@netray-info/common-frontend`, and none of `crates/{dns,tls,http,email,ip}` exposes `pub async fn run` (`netray-engine`'s orchestrator `run` is not a service entry point and stays).
GIVEN tracked files outside `specs/` and `CHANGELOG.md` WHEN grepped THEN none names `snapshots.db`, except the legacy table's `snapshots` row.
GIVEN a clean checkout WHEN `just adlc-setup` then `just adlc-verify` run THEN they pass and `git status --porcelain` is empty.
GIVEN `just acceptance-local` WHEN it starts THEN exactly one `netray serve` process runs.
GIVEN the `Dockerfile` WHEN read THEN it copies `netray.example.toml`, no `site/` and no `frontend/dist`, and its entrypoint runs `netray serve`.
GIVEN `release.yml` WHEN `test_workflows.sh` runs THEN the smoke step starts `serve` once and requests `/health` with the apex and four inspector `Host`s (behaviour verified by the release tag's run).
GIVEN `test_suite_size.sh` WHEN run THEN its floors equal the counts after deletion, and the commit carries `ADLC-Test-Change` for it.

## Phase 8 — Documentation, SDD and release notes

**Depends on:** Phase 7
**Requirements:** 24, 25, 26

`CHANGELOG.md`, `specs/sdd/v2.md` (S37, S38, §10 table, V0.3, the Phase 5 table, §9.2's trigger list confirmed), `CLAUDE.md`, `README.md`, `specs/rules/architecture-rules.md` and `specs/rules/logging-rules.md`. `report.md`'s claim → path table.

### Test Scenarios

GIVEN `## [Unreleased]` WHEN read THEN it holds the lead paragraph, `### Removed` with every row of requirements 13–14 and its successor, and `### Changed` with the config table and metric renames.
GIVEN each CHANGELOG claim WHEN checked against the code at HEAD THEN `report.md` names the path that makes it true, and no claim lacks one.
GIVEN `CHANGELOG.md` WHEN grepped for domains THEN only `netray.info` and `example.com` appear.
GIVEN `specs/sdd/v2.md` WHEN read THEN S37 carries the 2026-10-17 decision, and the §10 table shows no release `0.27.0`–`0.29.0`.
GIVEN `CLAUDE.md` WHEN grepped THEN it no longer contains `netray lens`, `run(config)`, "Config stays per service", `checkMeta.ts`, `packages/common-frontend` or `lens goldens`.
GIVEN `architecture-rules.md` WHEN read THEN the env prefix rule names `NETRAY_` only, with the refused legacy prefixes.

## Decision log

- **netray-web holds the host router and API v1, and `serve` composes it, over a `netray-api` crate or routes in the binary.** SDD §3.1 assigns the router, host routing and the apex API v1 to netray-web, and the binary is the composition root only (operator, X1, 2026-10-17).
- **An unknown `Host` answers 404, with `www.` left to Traefik, over a 301 to the apex.** A 404 costs nothing and serves no content under foreign names. A 301 would redirect any name pointed at argus (operator, 2026-10-17).
- **The bare `ip.` field routes answer 410 naming `/api/v1/ip?field=<name>`, over keeping them as `text/plain`.** S23 moved "one field as text" into the API, and keeping them would leave a V1 surface outside `/api/v1`. The cost is that curl one-liners break (operator, 2026-10-17).
- **The `lens.` and `email.` redirects live in the binary, over Traefik alone.** V5.3 asks for a test per row, and only the binary can carry one. Traefik routes `lens.` and `email.` to the one container (operator, 2026-10-17).
- **`[api]`, `[api.<inspector>]` and `[inspect.<tool>]` move into `netray.toml` under their names, over folding them into `[modules.<m>]`.** The sibling features defined them as separate tables, and S10 only merges the files (operator for `[api.*]`, 2026-10-17; `[inspect.*]` default, not interviewed).
- **lens's `[rate_limit]` and `[cache]` fold into `[admission]`, over keeping them as tables.** §4.4 names the window and the limits as admission, and moka is gone. `[cache] enabled` survives as `cache_enabled`, so no switch is lost (default, not interviewed).
- **The V1 docs and tool pages (`/api/*.html`, `/tools`) redirect, over a JSON 410.** They are human pages, and their successors exist (default, not interviewed).
- **Refuse legacy env prefixes and legacy top-level keys with a named replacement, over ignoring them or accepting both.** This matches the existing `refuse_legacy_prefix` convention (CLAUDE.md), and a silently ignored `LENS_` variable would run production on defaults.
- **One legacy table in code, mirrored in the CHANGELOG, over prose only.** The refusal and the operator's migration read the same rows. The CHANGELOG copy is checked by Phase 8's trace (P36 in spirit, without a generator, under the Rule of Three).
- `/health` and `/ready` answer before host routing on any `Host` header, over the apex only or per-host routes behind the redirects: the production container health check probes `http://127.0.0.1:<port>/health` (netray-ops `lens-compose.yml.j2:45`), which no host row would match, and Traefik's health checks and release.yml's smoke step address `lens.`, `email.` and the inspector hosts, which rows 13a and 13e would otherwise redirect; §5.1 does not list them, so this spec reserves them and amends §5.1 (implementer under the operator's autonomy mandate, 2026-10-17).
- **Subdomain `/api-docs/openapi.json` answers 410 like `ip.`'s, over 404.** §7.2 cuts it on `ip.`, and the other four hosts serve the same V1 document today (default, not interviewed).
- `/docs` on every subdomain and on the apex, and `site/api/*.html`, redirect 301 to `https://<apex>/api/v1/docs` over a 410 or a redirect to the raw `openapi.json`: `v2-api` builds an interactive docs page at `GET /api/v1/docs`, the human successor of the V1 docs pages (implementer under the operator's autonomy mandate, 2026-10-17).
- **The production acceptance run belongs to the operator after deploy, with `just acceptance-local agreement/prod.spec.ts` as its pre-deploy local equivalent, over gating the feature on production.** Production cannot run 0.30.0 before the deploy, and the deploy follows the release.
- **Run 0.30.0 through API request contexts in `prod`, over browser navigation.** A page visit can post a run through the streaming island and trip admission. Request contexts hit only known paths (fail2ban, `test_acceptance_prod_subset.sh`).
- **Move the survivors out of lens (Phase 6) before deletion (Phase 7), over one phase.** Each step stays bisectable, and a move with unchanged tests proves that behaviour is unchanged.
- **S24 stays unchanged, over amending it.** 0.30.0 is a `0.x` minor as S24 says. The skipped numbers are S37's record (operator, 2026-10-17).
- **`just release 0.30.0` after finish on `main`, over a phase on the branch.** The tag must name the merged commit, and a release is an operation without a failing test.

- `client_budget_per_hour`, `v2-admission`'s key, in every spec over `client_runs_per_hour`: one
  name for one key, the one `v2-admission` builds and `v2-result-page` sets through
  `LENS_ADMISSION__CLIENT_BUDGET_PER_HOUR` (implementer under the operator's autonomy mandate, 2026-10-17).
- One acceptance seed, `cargo run -p netray-store --example seed` over JSON run fixtures in
  `tests/acceptance/fixtures/history/`, created by `v2-inspectors` (first in build order to need
  it) and extended by `v2-result-page`, over a second, `sqlite3`-loaded `history-seed.sql`:
  `HistoryStore::record` runs the migrations and computes fingerprints and diffs, so fixtures
  cannot drift from the schema, and no `sqlite3` CLI joins the toolchain (implementer under the operator's autonomy mandate, 2026-10-17).
- Each spec asserts the `test_web_crate.sh` shape it leaves (Router allowed under
  `crates/web/src/entries/`, `pages/` and `api/`; axum allowed; the `netray` → `netray-web` guard
  lifted by `v2-pages` or `v2-api`, whichever lands first) and lifts only what is still in place,
  over asserting guards a parallel lane may already have lifted (implementer under the operator's autonomy mandate, 2026-10-17).
- `Trigger` gains `api`, spending the budget like `visit` and `recheck`, over v1's run POST passing
  `recheck`: SDD §9.2's `netray_runs_total{trigger}` names `api`, which `v2-cutover` counts (implementer under the operator's autonomy mandate, 2026-10-17).
- `v2-result-page`'s `LENS_EXTRA` pass-through of `acceptance-local` becomes `ACCEPTANCE_ENV`, carrying `NETRAY_` keys, over keeping its name: requirement 10 refuses any variable starting with `LENS_`, so the old name in the recipe's environment would stop `serve` (implementer under the operator's autonomy mandate, 2026-10-17).
- One build order in every spec (lane A `v2-history` → `v2-admission` → `v2-entries` → `v2-api`;
  lane B `v2-spectrum` ∥ `v2-pages`, parallel to lane A; lane C `v2-inspectors`, its Phase 2 before
  `v2-result-page` Phase 1, then `v2-result-page`; then `v2-cutover`) over each spec's own reading
  of the program: every **Depends on** and Context names the same order (implementer under the operator's autonomy mandate, 2026-10-17).
- Legacy detection by key path, with `inspect` legacy only as spectra's four top-level keys, over listing `inspect` as a legacy table: the new top-level `[inspect.<tool>]` is valid, and the old keys never collide with a tool name (implementer under the operator's autonomy mandate, 2026-10-17).
- Per-origin rows for `backends`, `cache` and `rate_limit` (lens's into `[engine]`/`[admission]`, ifconfig's `rate_limit` into `[api.ip]`, ifconfig's `[cache]` removed with its only reader, prism's and tlsight's `[backends.ip]` and beacon's `[backends] ip_url` removed), over lens-only rows: the six old files share these names with different meanings, and a lens-only message would misdirect the other five (implementer under the operator's autonomy mandate, 2026-10-17).
- ifconfig's `[server] admin_bind` maps to `[server] metrics_bind` and `admin_token` is removed, over a token on the merged metrics listener: lens's metrics listener has none today, and the operator binds it to a private address (implementer under the operator's autonomy mandate, 2026-10-17).
- IP enrichment calls the in-process ip module (requirement 28, Phase 2), over keeping `ip_url` pointed at a host: after the one-container cutover no ifconfig-rs host exists, and an HTTP loop back into the same process would pass admission and rate limits it does not need (implementer under the operator's autonomy mandate, 2026-10-17).
- `?d=` keeps `v2-pages`' `400` for an unparsable value, with an absolute `Location` on the valid case, over this spec's earlier `301 /`: one handler, one answer, and requirement 13's absolute-`Location` rule (implementer under the operator's autonomy mandate, 2026-10-17).
- The `pub async fn run` ban covers the module crates and lens, over every crate but `netray`: `netray-engine`'s orchestrator `run` is the engine's API, not a service entry point (implementer under the operator's autonomy mandate, 2026-10-17).
- V5.6 splits into a code half built here and the file deletion on argus as an operator step, like V5.2 and V5.5, over marking V5.6 wholly built (implementer under the operator's autonomy mandate, 2026-10-17).
- §9.2's trigger list (visit, recheck, api, badge, og) stays `v2-admission`'s amendment per M1, with requirement 25 confirming it, over amending it a second time here (implementer under the operator's autonomy mandate, 2026-10-17).
- Only S37, S38 and the §10 table are amended for the release numbers, over also editing the Phase 2 intro, which names no release (implementer under the operator's autonomy mandate, 2026-10-17).

- V2.8's `privacy-policy.md` half was committed in netray-ops (a9cea7b, local, operator pushes) on 2026-10-17: the SDD amendment of this feature marks V2.8 done in the Phase 2 table (implementer under the operator's autonomy mandate, 2026-10-17).

- Phases 1–5 built ahead on main while lane C and v2-api finish, Phase 6 onward after all eight merged (operator, 2026-10-18, X6): the branch rebases onto each landing and re-runs the gate; what a later feature adds to crates/lens or config is folded into Phase 6's inventory then.

- Phase 1 builds `netray.toml` without `[api]`, `[api.<inspector>]` and `[inspect.<tool>]`, and their scenarios (C15, C18's `[inspect.http]` half) wait for the merge of `v2-api` and `v2-inspectors`, over defining interim structs in `crates/netray`: `v2-api` requirement 8 defines the `[api]` structs in `netray_web::api` and `v2-inspectors` the `[inspect.*]` ones, so a second definition here would fork them and conflict on merge; until then a top-level `[api]` or `[inspect.<tool>]` fails through `deny_unknown_fields`, and the merge commit embeds the siblings' types and adds the tests (implementer under the operator's autonomy mandate, 2026-10-18).
- In Phase 1 `serve` translates `netray.toml` into lens's config and starts lens's application in-process (one listener, no host routing yet), over a new composition before Phase 2: behaviour stays lens's, Phase 2 replaces the composition with netray-web's host router, and Phase 6 moves the survivors out of lens; every registered module's section runs, with `[modules.<m>] deadline_ms` as its window (lens's `[backends.http]`/`[backends.email]` presence switch has no successor, and production sets both) (implementer under the operator's autonomy mandate, 2026-10-18).
- Phase 2's host router gives `dns.`, `tls.`, `http.` and `ip.` an optional inspector router each, empty until `v2-inspectors` merges, so those hosts answer 404 at every path but `/health` and `/ready` until then; the inspector halves of the Phase 2 scenarios (C4, C8, C13) are tested at that merge, over keeping the V1 SPAs on those hosts or inventing an interim inspector: the V1 SPAs run in other processes that `serve` does not start, and an interim page would be replaced within days (implementer under the operator's autonomy mandate, 2026-10-18).
- Phase 2 copies `site/.well-known/mta-sts.txt` and `site/bimi-logo.svg` into `crates/web/static/` and leaves the `site/` originals until Phase 7 deletes `site/`, over moving them now: `netray site` serves them until Phase 7 (implementer under the operator's autonomy mandate, 2026-10-18).
- Phase 3 rewrites every guide sample in `crates/web/templates/` that names a V1 subdomain API (`dns.netray.info/api/…`, `email.netray.info/inspect/…` and the like) to its `/api/v1/…` equivalent in the commit that makes those paths 410, and adds a repo check that no template names a V1 subdomain `/api/` or `/inspect` path, over leaving the samples to Phase 8's prose pass: a sample that answers 410 the moment the router lands is a wrong result (orchestrator, 2026-10-18; implementer under the operator's autonomy mandate, 2026-10-18).
- `scoring.html`'s sample `https://netray.info/?d=example.com&dkim_selectors=…` changes to a sample without `dkim_selectors`, over carrying `dkim_selectors` through the `?d=` 301 onto `/<domain>`: no V2 route on main reads a `dkim_selectors` parameter, so a carried parameter would promise behaviour nothing implements, and the change stays in one template instead of `v2-pages`' handler; if `v2-result-page` adds selector input, the sample follows it there (implementer under the operator's autonomy mandate, 2026-10-18).
- Phase 2 continues past the reviewer's AMENDMENT that requirement 28's test exercises the enrichment client and `HttpModule` but not `lens_registry`'s wiring, over halting with `amendment-forward`: the registry lives in the binary with no library seam, and a run through the real modules needs the network; Phase 6 moves `lens_registry` into `crates/netray`'s composition root (requirement 5), where a wiring test over `stub_ip_module` gets its seam, and Phase 6's inventory carries the item (implementer under the operator's autonomy mandate, 2026-10-18).
- Phase 3 builds every redirect and 410 row now, including those that retire V1 routes whose successors arrive with `v2-api` (apex `/api/check`, `/docs`, the `ip.` field routes), over holding those rows until `v2-api` merges: the branch merges only after all eight features (X6), so no build without the successors ships, and the rows are pure routing; row 13k and the `ip.` curl root (requirement 14's neighbour) wait for `v2-inspectors` and `v2-api` (`lookup_caller`), and the `/api/v1/meta` neighbour of row 13i is tested at the `v2-api` merge (implementer under the operator's autonomy mandate, 2026-10-18).
- Phase 3 makes row 13j hold under `serve` (no `[snapshots]` in `netray.toml`, so the V1 step never runs; `/r/<id>/<anything>` for a non-entry answers 410), and leaves deleting `EntriesDeps`' V1 lookup to Phase 7 with requirement 23, over removing it now: `netray lens` still mounts it and its protected `snapshot_routes.rs` holds `[history] enabled = false` restoring V1 `/r/` until lens is deleted (implementer under the operator's autonomy mandate, 2026-10-18).
- A method other than GET or HEAD on a redirect row (`POST lens.<apex>/x`, `POST email.<apex>/`) answers 405 with `Allow: GET, HEAD`, over falling through to 404 or redirecting it: requirement 13 limits redirects to GET and HEAD, and a 405 tells a client the path exists for reading (implementer under the operator's autonomy mandate, 2026-10-18).
- Phase 4 registers every §9.2 name at startup and renames every series the `serve` process emits now, and tests the run, badge, OG and page halves of requirement 18 on lens's application (the apex app until Phase 6), over waiting for the composition root: `netray_api_requests_total` is registered now but counted, and requirement 18's `/api/v1` and inspector-job halves tested, at the `v2-api` and `v2-inspectors` merges; the V1 standalone subcommands share the renamed module code and export the new names until Phase 7 deletes them (implementer under the operator's autonomy mandate, 2026-10-18).
- lens's V1 `/api/check` (GET and POST) counts its runs as `trigger="api"`, over `visit`: it is an API client's run, as `v2-admission`'s `Trigger::Api` defines, and `visit` stays the page's trigger (implementer under the operator's autonomy mandate, 2026-10-18).
- A module's surviving series is renamed `netray_<module>_<suffix>` (`prism_check_duration_seconds` → `netray_dns_check_duration_seconds`, `beacon_check_duration_seconds` → `netray_email_check_duration_seconds`), over requirement 17's plain `netray_<suffix>`: the plain form collides across modules (two `netray_check_duration_seconds` with different labels, three per-target rate-limit series); dns's `endpoint` label (values like `check`, not route templates) becomes `operation`, and email's `target` label (a forbidden name, requirement 18) becomes `fetch` (implementer under the operator's autonomy mandate, 2026-10-18).
- `netray_admission_rejections_total{reason="limit"}` is counted where a route refuses on `check_limits`, not inside `check_limits`, over counting in the check: a check called twice for one request would count twice (implementer under the operator's autonomy mandate, 2026-10-18).
- Phase 5 builds `crates/netray/tests/agreement.rs` over the views main has — the entry page `/r/<id>`, the badge, the OG card and the stored run — composed as `serve` composes them (netray-web's host router over lens's routes, stub modules, a temporary store), and the four states plus the incomplete state's "badge, OG card unchanged" half; the result page, open issues, evidence views, `/api/v1` (incl. `/diff`, C6) and the stream join the test at the `v2-api`, `v2-inspectors` and `v2-result-page` merges, over deferring the whole test: the states, the stub modules and the comparison are built once and each merge adds its views as rows (implementer under the operator's autonomy mandate, 2026-10-18).
- The `v2-api` merge (7b6772b6) leaves `netray.toml` without `[api]`: `to_lens` sets lens's `api` to `ApiConfig::default()`, so `serve` mounts `/api/v1` with the defaults, and C15's `[api]` half is built in Phase 6 test-first, over adding the table in the merge: the merge compiles and the gate passes without it, and C15 is a requirement with its own failing test, not a merge repair (implementer under the operator's autonomy mandate, 2026-10-18).

- Phase 6 folds in what the Phases 1–5 deferrals and the merge inventory left, now that all eight features are merged: `[api]` and `[inspect.<tool>]` in `netray.toml` (Phase 1 C15, C18), `netray_api_requests_total` counted (Phase 4 C8), the inspector hosts and row 13k (Phase 2 C4/C8/C13, Phase 3 C11/C17), the agreement test's result page, issues, evidence and `/api/v1` rows (Phase 5), the ip module's UA parser, the registry wiring test, CI's check-job headroom and the inspector back-link; the guide samples and the `dkim_selectors` sample were already rewritten in Phase 3, and their repo check (`test_no_v1_samples.sh`) holds after the merge (implementer under the operator's autonomy mandate, 2026-10-19).
- Operator, 2026-10-19: stop after Phase 6; V1 stays runnable as the reference (the live review found V2 diverging from the design canvas and V1 inspector functions lost). Phases 7 and 8, review and finish wait. Phase 6 therefore copies the survivors out of `crates/lens` instead of moving them: `serve` switches to the copies, `crates/lens`, its tests and `netray lens` stay untouched, and the deletions with their `ADLC-Test-Change` trailers happen in Phase 7, over moving and rewiring lens onto the moved code: lens's V1 wire needs `BackendExtra`, which the survivors drop, and rewiring a crate Phase 7 deletes buys nothing (implementer under the operator's autonomy mandate, 2026-10-19).
- Phase 6's scenario "neither `crates/netray` nor `crates/web` depends on `lens`" holds for `crates/web`; for `crates/netray` it reads "only `src/v1.rs`, the `netray lens` subcommand, names lens", held by `test_no_lens_dependency.sh`, until Phase 7 deletes the subcommand and the dependency with it, over deleting `netray lens` now: the operator keeps V1 runnable (implementer under the operator's autonomy mandate, 2026-10-19).
- The real `InspectorJobs` stays in netray-web (`netray_web::api::InspectJobs`), over moving it to `crates/netray` as requirement 5 says: it already lives outside lens, and SDD §3.1 lets netray-web name module crates for inspector jobs (implementer under the operator's autonomy mandate, 2026-10-19).
- The inspector back-link after a refused job is fixed client-side: the stream island empties `[data-stream-reset]` elements before it posts, and the back-link carries the marker, so a refusal leaves no link and a started job's fragment re-fills it, over a second server fragment: v2-inspectors requirement 10's one-fragment refusal and its router tests stay (implementer under the operator's autonomy mandate, 2026-10-19).
- CI's check job gets `timeout-minutes: 60`, over splitting the job: one line, no change to what runs; verified by the branch's first CI run (implementer under the operator's autonomy mandate, 2026-10-19).
- Under `serve` the apex answers unmatched paths with netray-web's 404 page and `/robots.txt` with `pages::robots` (with the `Sitemap:` line), over lens's SPA fallback and its bare robots file: `v2-pages` exported both for this feature's apex router, and the SPA is V1 (implementer under the operator's autonomy mandate, 2026-10-19).
- The ip host's curl root answers the caller's socket address as text without `[modules.ip]`, over requiring the module: the address needs no GeoIP data, as V1's root did (implementer under the operator's autonomy mandate, 2026-10-19).

## Open decisions

None.

## Out of scope

Operator handoff, in this order:

1. **Before the deploy, the second baseline pull (V0.3, S33).** On argus, run the Phase 0 script (netray-ops, beside the production watch, S27) against a copy of `snapshots.db` taken with SQLite's online backup, read-only on the original. Keep only the aggregate JSON in the private planning records. Then 0.30.0 may deploy.
2. **Push.** `git push && git push origin v0.30.0` from `main`. Do not `git fetch` before the tag is pushed. `release.yml` builds and smoke-tests `ghcr.io/netray-info/netray:0.30.0`.
3. **netray-ops config: one `netray.toml`, merged from the six production configs** per the CHANGELOG 0.30.0 configuration table. This includes `[api]`, `[api.<inspector>]` and `[inspect.<tool>]`, and `[rate_limit]` and `[cache]` folded into `[admission]`.
   - `[server] bind`, `metrics_bind`, `trusted_proxies` (lens's values) and `apex = "netray.info"`.
   - `[store] db_path` on the persistent volume.
   - `[modules.ip]` naming the files under `/netray/data`.
   - lens's `[modules.*]` tables; `[admission]` (S33 start values); `[badges]`, `[og_cards]`, `[telemetry]`.

   Verify with `docker run --rm -v <file>:/netray/netray.toml ghcr.io/netray-info/netray:0.30.0 serve --check-config /netray/netray.toml`.
4. **netray-ops env.** Remove every `LENS_*`, `NETRAY_{DNS,TLS,HTTP,EMAIL,IP}_*`, `PRISM_*`, `TLSIGHT_*`, `SPECTRA_*`, `BEACON_*` and `IFCONFIG_*` variable. `serve` refuses to start while any is set. Set the `NETRAY_*` equivalents only where a value must stay out of the file.
5. **One container.** It replaces the seven (lens, dns, tls, http, email, ip, site) and runs `serve /netray/netray.toml`. Mounts: the config, the persistent volume for `history.db`, `/netray/data` (GeoIP, from the data image), and the TLS custom CA directory if one is used.
6. **Traefik.** Route `netray.info`, `dns.`, `tls.`, `http.`, `ip.`, `email.`, `lens.` and `mta-sts.netray.info` to the container's bind port, with health checks on `/health`. Remove the per-service routers and services. Keep any `www.` redirect at Traefik (operator, 2026-10-17).
7. **DNS.** No change expected. Verify that each host above resolves to argus.
8. **Prometheus, dashboards and alerts.** One scrape target at `metrics_bind` replaces the six jobs. Dashboards and alerts move to `netray_*` without continuity (S11).
9. **After the deploy:** `TEST_ENV=production just acceptance`, project `prod`. This is V5.5's production half.
10. **After the deploy and a verified pull:** delete `snapshots.db` from argus's volume (S2).

Elsewhere:

- Pages, API, inspectors, history, admission and entries: `v2-history`, `v2-admission`, `v2-entries`, `v2-api`, `v2-spectrum`, `v2-pages`, `v2-result-page`, `v2-inspectors` (`specs/features/<name>/spec.md`).
- Re-setting the admission numbers: after V2 (S33).
- The privacy SDD and PRD: private planning records.
