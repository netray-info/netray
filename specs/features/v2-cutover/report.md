# Report: v2-cutover

## Phase 1 — `netray serve` and `netray.toml`

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: `netray serve [<config>]` takes the file from the argument, else `NETRAY_CONFIG`, else defaults; `serve --check-config <path>` prints `config ok: <path>` (exit 0) or `config error: <path>: <error>` (exit 1); the check opens no file the config names; SIGHUP IP reload kept | green | serve_check_config.rs, serve_config_env.rs |
| C2 | R9: one config type in `crates/netray/src/config.rs`, prefix `NETRAY_`, tables `[server]` (bind, metrics_bind, trusted_proxies, apex), `[engine]`, `[store]`, `[admission]` (8 keys), `[badges]`, `[og_cards]`, `[telemetry]`, `[modules.<m>]` with `deadline_ms`; defaults, `validate()` rules and `startup_rejects` rows kept; `deny_unknown_fields` held by `test_config_strict.sh` | green | serve_config_env.rs; test_config_strict.sh |
| C3 | R10: the eleven legacy env prefixes refused before loading through `refuse_legacy_prefix(<legacy>, "NETRAY_", …)` | green | serve_env_prefix.rs |
| C4 | R11: legacy key paths refused before deserialization from one table in `crates/netray/src/legacy.rs`, naming the key path and its destination or removal in 0.30.0, per origin, pointing to the CHANGELOG 0.30.0 table | green | serve_legacy_keys.rs |
| C5 | R12: `netray.example.toml` holds every top-level table with every key at its default, commented; `netray.dev.toml` exists; `--check-config` exits 0 on both; startup refusals become `startup_rejects` rows of `test_check_config.sh` for `serve` | green | serve_check_config.rs; test_check_config.sh |
| C6 | GIVEN `netray.example.toml` WHEN `serve --check-config` THEN exit 0 printing `config ok: <path>` | green | serve_check_config.rs |
| C7 | GIVEN `[server] binds = "x"` WHEN checked THEN exit 1 naming `binds` | green | serve_check_config.rs |
| C8 | GIVEN `NETRAY_SERVER__BIND=127.0.0.1:<port>` WHEN `serve` starts THEN it listens on that port | green | serve_startup.rs |
| C9 | GIVEN `NETRAY_MODULES__IP__GEOIP_CITY_DB=/x` WHEN loaded THEN `[modules.ip]` holds it, no legacy refusal | green | serve_config_env.rs |
| C10 | GIVEN `LENS_SERVER__BIND` (and one variable per other legacy prefix) WHEN `serve` or `serve --check-config` THEN exit 1 naming the variable, `NETRAY_*`, `NETRAY_CONFIG` | green | serve_env_prefix.rs |
| C11 | GIVEN `NETRAY_DNS_SERVER__BIND` WHEN `serve` starts THEN exit 1 naming it | green | serve_env_prefix.rs |
| C12 | GIVEN `[snapshots] enabled = true` WHEN checked THEN exit 1, `snapshots` removed in 0.30.0 | green | serve_legacy_keys.rs |
| C13 | GIVEN `[history] db_path` WHEN checked THEN exit 1 naming `[store] db_path` | green | serve_legacy_keys.rs |
| C14 | GIVEN `[cache] ttl_seconds = 300` THEN names `[admission] cache_window_seconds`; GIVEN `[rate_limit] per_ip_burst = 3` THEN names `[admission] per_ip_burst` | green | serve_legacy_keys.rs |
| C15 | GIVEN `[api.ip] batch_max_size = 50` and `[inspect.tls] deadline_ms = 15000` THEN exit 0; `batch_max_size = 101` at `serve` exits 1 naming the key | test-unwritable (deferred until v2-api and v2-inspectors merge: their types do not exist on main; Decision log 2026-10-18) | |
| C16 | GIVEN top-level `geoip_city_db` THEN exit 1 naming `[modules.ip].geoip_city_db` | green | serve_legacy_keys.rs |
| C17 | GIVEN top-level `[dns]` THEN exit 1 pointing to the CHANGELOG table | green | serve_legacy_keys.rs |
| C18 | GIVEN top-level `[inspect] max_redirects = 5` THEN exit 1 naming `[modules.http.inspect].max_redirects`; GIVEN `[inspect.http] max_concurrent = 4` THEN exit 0 | green | serve_legacy_keys.rs (first half; `[inspect.http]` half deferred with C15) |
| C19 | GIVEN `[server] admin_bind` THEN exit 1 naming `[server] metrics_bind`; GIVEN `[server] admin_token` THEN removed in 0.30.0 | green | serve_legacy_keys.rs |
| C20 | GIVEN `[cache] ttl_secs = 60` THEN ifconfig's cache removed in 0.30.0; GIVEN `[cache] enabled = true` THEN names `[admission] cache_enabled` and ifconfig's removal | green | serve_legacy_keys.rs |
| C21 | GIVEN `[rate_limit] per_target_per_minute = 120` THEN exit 1 naming `[api.ip]` | green | serve_legacy_keys.rs |
| C22 | GIVEN top-level `user_agent_regexes` THEN exit 1 pointing to the CHANGELOG table | green | serve_legacy_keys.rs |
| C23 | GIVEN `[store] db_path` in a nonexistent directory WHEN `serve` starts THEN non-zero naming the history store; `--check-config` exits 0 | green | serve_startup.rs |
| C24 | GIVEN `[modules.ip]` naming missing GeoIP files WHEN `serve` starts THEN non-zero naming the file; `--check-config` exits 0 | green | serve_startup.rs |

### Inventory (main at df5a5a7e, 2026-10-18)

Merged: v2-history, v2-admission, v2-entries, v2-spectrum, v2-pages. Open: v2-api, v2-inspectors, v2-result-page; their items are marked *pending* and are re-read when each lands (Phase 6 folds them in).

| item | where today | target (§3.1) |
|---|---|---|
| `[server]` bind, metrics_bind, trusted_proxies | lens `config.rs` | `netray.toml [server]` (+ `apex`) |
| `[backends] resolve_timeout_ms` | lens | `[engine] resolve_timeout_ms` |
| `[backends.<m>] timeout_ms` (section windows) | lens `state.rs:106-150` | `[modules.<m>] deadline_ms` |
| `[backends.<m>] url` | lens, already refused | legacy: removed (V1.4) |
| `[scoring] profile_path` | lens | `[engine] profile_path` |
| `[history]` enabled, db_path | lens (v2-history) | `[store]` |
| `[admission]` client_budget_per_hour, domain_run_slots | lens (v2-admission) | `[admission]` |
| `[rate_limit]` four keys | lens | `[admission]` same names |
| `[cache]` enabled, ttl_seconds | lens (moka V1 holder, `state.rs:106`) | `[admission] cache_enabled`, `cache_window_seconds` |
| `[badges]`, `[og_cards]`, `[telemetry]` | lens | same tables |
| `[snapshots]`, `[site]`, `[ecosystem]` | lens | legacy: removed |
| `[modules.*]` | lens, built by `lens_registry` (`crates/netray/src/main.rs`) | `[modules.*]`; `lens_registry` stays in `crates/netray` |
| `[api]`, `[api.<inspector>]` | *pending* v2-api (structs in `netray_web::api`) | `netray.toml [api]` |
| `[inspect.<tool>]` | *pending* v2-inspectors | `netray.toml [inspect]` |
| `admission.rs` (`check_limits`, `admit`, `RunStream`, slots, throttle, per-client limiter, `Sec-Purpose`) | `crates/lens/src/admission.rs` | `netray-web` (Phase 6) |
| `check.rs` orchestration, run step | `crates/lens/src/check.rs` | `netray-engine` (Phase 6) |
| recording (`history.rs`) | `crates/lens/src/history.rs` | `netray-store` (Phase 6) |
| `input::validate_domain` | `crates/lens/src/input.rs:17` | `netray-web` (Phase 6) |
| `client_ip_from` | `crates/lens/src/routes.rs:713` | `netray-web` (Phase 6) |
| V1 snapshot lookup + entry OG card renderer handed to `EntriesDeps` | `crates/lens/src/lib.rs:218-246` | snapshot lookup deleted (13j, Phase 3/7); card renderer to `netray-web` |
| `pages::router()`, `entries::router`, `privacy_router()` mounts | `crates/lens/src/lib.rs:193-246` | composed by netray-web host router (Phase 2) |
| badge, OG (`badge/`, `og/`) | `crates/lens/src` | `netray-web` (Phase 6) |
| middleware stack (security, layer order) | `crates/lens/src/lib.rs`, `security/` | `netray-web` (Phase 6) |
| `DomainRuns` implementation | *pending* v2-api | `netray-web` |
| real `InspectorJobs` | *pending* v2-inspectors | `crates/netray` |
| `GET /`, `/{domain}`, run POST, history routes | *pending* v2-result-page | netray-web, composed by the host router |
| series `lens_check_requests_total`, `lens_badge_requests_total`, `lens_rate_limit_hits_total`, `lens_admission_rejections_total`, `lens_runs_in_flight`, `lens_history_record_errors_total` | `crates/lens/src/metrics.rs` | Phase 4 renames / ends |
| services' own `Config` + loaders (dns, tls, http, email, ip) | `crates/<m>/src/config.rs` | deleted Phase 7; their keys in the legacy table |
| guide samples naming V1 subdomain APIs (e.g. `guides/domain-security-audit.html:83` `dns.netray.info/api/query`, the `email.netray.info/inspect/…` sample) | `crates/web/templates/pages/guides/` | rewritten to `/api/v1/…` in Phase 3 with a repo check (orchestrator, 2026-10-18) |
| `scoring.html:222` `?d=example.com&dkim_selectors=…` sample | `crates/web/templates/pages/guides/scoring.html` | sample without `dkim_selectors`, Phase 3 (Decision log) |
| `scoring.html` "Testing DKIM selectors" section and `domain-security-audit.html:79`'s `jq '{grade, complete, sections}'` over the runs answer: prose that V2 makes false (no selector parameter; the run answer is `{recorded, entry_id, run, diff}`) | `crates/web/templates/pages/guides/` | Phase 8 prose pass, against the merged v2-api |
| CHANGELOG: every ended series and its successor (`lens_check_requests_total{result}` → `netray_runs_total{trigger}`, `cache_hit`/`rate_limited`/`joined` without successor; every `lens_`, module and ip rename) | `CHANGELOG.md` | Phase 8 (requirement 17's last sentence, requirement 24) |
| UA parser for the in-process ip module: v2-api's `?field=user_agent` (and the caller form's user-agent data) renders `Other, , Other, ` because `IpModule` has no UA parser; V1 ifconfig parsed it from its `user_agent_regexes` data file. Move the parser from the ifconfig service code into the ip module, add `user_agent_regexes` as a `[modules.ip]` key (the legacy row for top-level `user_agent_regexes` then names `[modules.ip].user_agent_regexes`), and test that `?field=user_agent` equals V1's answer (orchestrator, 2026-10-18) | `crates/ip/src/` (V1 ifconfig code → module) | Phase 6, before Phase 7 deletes the ifconfig service; needs v2-api merged |
| `just acceptance-local`: still starts `netray site` and six subcommands with `LOCAL_<NAME>_URL`; must start one `netray serve` and pass `LOCAL_URL` (Phase 5's `env.ts`), with the `v2-inspectors` seed and `ACCEPTANCE_ENV` (requirement 7) | `justfile` | Phase 7 |
| `tests/acceptance/schemas/ecosystem-meta.schema.json`, read by `tests/repo/test_meta_schema.sh` | `tests/` | Phase 7 (requirement 7 removes or rewrites `test_meta_schema`) |
| `agreement/prod.spec.ts` selectors (`.grade__letter`, `.check-row__code`/`__tone`, `og:url` entry ID) on pages main lacks | `tests/acceptance/agreement/prod.spec.ts` | confirm at the v2-result-page merge (`TODO(v2-result-page)`) |
| wiring test: `lens_registry` hands `netray_ip::enrichment_client` to the four modules (review AMENDMENT, Phase 2) | `crates/netray/src/main.rs` | Phase 6, with the registry's move into the composition root |
| host-router answers bypass the layers' `http_metrics` (no `path` label for probes and host 404s) | `crates/web/src/host/` | Phase 4 metrics / Phase 6 middleware move |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1–G4 (one coder; the groups compile only together) | 1 | sonnet | 118022 | 274 |
| review fix (bare legacy tables, window-only module tables) | 1 | sonnet | 31596 | 74 |

### Behavioural verification

```
$ netray serve --check-config crates/netray/netray.example.toml
(WARN lines: modules not configured)
config ok: crates/netray/netray.example.toml
exit 0
$ netray serve --check-config c.toml   # [cache] enabled = true
config error: …/c.toml: legacy configuration key `cache.enabled` (lens: now `[admission] cache_enabled`; ifconfig: removed in 0.30.0); see the CHANGELOG 0.30.0 configuration table
exit 1
$ LENS_SERVER__BIND=x netray serve
failed to load configuration: LENS_SERVER__BIND is set: these variables are now NETRAY_* (config file: NETRAY_CONFIG)
exit 1
$ NETRAY_SERVER__BIND=127.0.0.1:18123 netray serve s.toml &; curl -H "Host: netray.info" 127.0.0.1:18123/health
200
```

### Review

- BLOCKER (fixed, second pass): a bare legacy table (`[scoring]` as in lens's production fixture, `[history]`) failed only through `deny_unknown_fields`. Now every legacy table is refused by name with its fate per origin; rows `bare-*` in `serve_legacy_keys.rs`.
- AMENDMENT (repaired in phase): bare `[cache]`, `[rate_limit]`, `[backends.<m>]` — same fix.
- AMENDMENT (repaired in phase): a `[modules.<m>]` table holding only `deadline_ms` switched the module on; `to_lens` drops tables that are empty after stripping it (`c2-window-only`). A bare `[inspect]` is not refused as a legacy table, because the new `[inspect.<tool>]` tables share the name.
- DEFERRED: the legacy messages point to the CHANGELOG 0.30.0 configuration table, written in Phase 8 (requirement 24).
- NIT: a config path given without `.toml` skips the legacy check (config-rs appends the extension); the exit stays 1 with the generic message.

## Phase 2 — Host routing

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R2: one listener at `[server] bind` serves every host by `Host` (port stripped, ASCII-lowercased, compared against `[server] apex`); the host router lives in netray-web, `serve` composes and owns no route; apex serves the apex routers; `mta-sts.` only the policy; unknown or no `Host` 404; `[server] metrics_bind` serves only `/metrics` | green | serve_hosts.rs |
| C2 | R3: `/health` and `/ready` before host routing on any `Host` or none; apex reserved root files: `/favicon.svg` 200 (bytes of `/static/favicon.svg`), `/favicon.ico` 404, `/bimi-logo.svg` 200 from `crates/web/static/bimi-logo.svg`, every `/.well-known/` path 404; no §5.1 reserved segment or root file reaches the result page | green | serve_hosts.rs, serve_reserved_paths.rs |
| C3 | R28: enrichment in-process through the registered ip module (no HTTP, no base URL); without an ip module enrichment is off; `[modules.dns.backends.ip]`, `[modules.tls.backends.ip]`, `[modules.http.enrichment]`, `[modules.email.backends] ip_url` leave the module configs and join the legacy table | green | enrichment_in_process.rs (crates/common, crates/netray), serve_legacy_enrichment.rs |
| C4 | GIVEN `serve` WHEN `GET /` with `Host: netray.info` THEN the apex's root page; WITH `dns.`/`tls.`/`http.`/`ip.` THEN that inspector | green | serve_hosts.rs (apex half; inspector half deferred with C8) |
| C5 | GIVEN `Host: NETRAY.INFO:443` WHEN `GET /` THEN the apex's root page | green | serve_hosts.rs |
| C6 | GIVEN `Host: mta-sts.netray.info` WHEN `GET /.well-known/mta-sts.txt` THEN 200 policy; `GET /` 404 | green | serve_hosts.rs |
| C7 | GIVEN `Host: www.example.com` or `www.netray.info` WHEN `GET /` THEN 404 | green | serve_hosts.rs |
| C8 | GIVEN `Host: dns.netray.info` WHEN `GET /?q=example.com` THEN the same markup as apex `GET /inspect/dns?q=example.com` | test-unwritable (deferred until v2-inspectors merges: no `/inspect/<tool>` route on main; Decision log 2026-10-18) | |
| C9 | GIVEN the apex WHEN `GET /favicon.svg` THEN 200 `image/svg+xml`; `/bimi-logo.svg` 200; `/favicon.ico` and `/.well-known/mta-sts.txt` 404 | green | serve_hosts.rs |
| C10 | GIVEN every §5.1 reserved segment and root file WHEN requested on the apex THEN none answers the result page | green | serve_reserved_paths.rs |
| C11 | GIVEN each of apex, `dns.`, `tls.`, `http.`, `ip.`, `email.`, `lens.`, `mta-sts.`, `www.example.com`, `127.0.0.1:<port>`, no `Host` WHEN `GET /health`, `GET /ready` THEN 200 | already_implemented (lens answers both on every Host today; kept as protection) | serve_hosts.rs |
| C12 | GIVEN the composed router WHEN its route order is read THEN `/health`, `/ready` match before host dispatch and before row 13a's `lens.` redirect | green | serve_hosts.rs (health on every host incl. lens.) |
| C13 | GIVEN `[server] apex = "localhost"` WHEN `GET /` with `Host: dns.localhost` THEN the DNS inspector | green | serve_hosts.rs (host match half; inspector half deferred with C8) |
| C14 | GIVEN the main listener WHEN `GET /metrics` THEN 404; GIVEN the metrics listener THEN 200 | green | serve_hosts.rs |
| C15 | GIVEN the ip module over stub data WHEN a run reports a server address THEN server org and network type are filled, no HTTP request leaves; GIVEN no ip module THEN absent and the run completes | green | crates/netray/tests/enrichment_in_process.rs |
| C16 | GIVEN `[modules.http.enrichment] ip_url` or `[modules.dns.backends.ip] url` WHEN checked THEN exit 1, removed in 0.30.0, enrichment in-process | green | serve_legacy_enrichment.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1+G2 (one coder; both edit `crates/netray/src/main.rs`) | 1 | sonnet | 145460 | 513 |
| review fix (lens layers around the host router, MTA-STS `no-cache`) | 1 | sonnet | n/a | n/a |

### Behavioural verification

`netray serve s.toml` with `NETRAY_SERVER__BIND=127.0.0.1:18123`, `NETRAY_SERVER__METRICS_BIND=127.0.0.1:18124`; `curl -H "Host: <h>" 127.0.0.1:18123<p>`:

```
netray.info            /                          200 text/html; charset=utf-8
netray.info            /health                    200 application/json
netray.info            /.well-known/mta-sts.txt   404 text/html; charset=utf-8
netray.info            /favicon.svg               200 image/svg+xml
dns.netray.info        /                          404
dns.netray.info        /health                    200 application/json
mta-sts.netray.info    /                          404
mta-sts.netray.info    /.well-known/mta-sts.txt   200 text/plain; charset=utf-8
www.example.com        /                          404
www.example.com        /health                    200 application/json
lens.netray.info       /                          404
lens.netray.info       /health                    200 application/json
metrics main: 404  metrics listener: 200
```

### Review

- BLOCKER (fixed, second pass): the host router's own answers (`/health`, `/ready`, root files, MTA-STS, host 404s) bypassed lens's layer stack: no security headers, CORS preflight 405, no `X-Request-Id`. lens's `app` is now `layers(routes(state))`; `serve` wraps the whole host router in `layers` once, and `http_metrics` stays on lens's routes. Test `serve_probe_headers.rs`.
- AMENDMENT (not repaired here; Decision log 2026-10-18): requirement 28's test does not exercise `lens_registry`'s wiring; carried to Phase 6's inventory.
- DEFERRED: apex `/api` and `/api/` answer 404 in the host router before the apex router; Phase 3's row 13m (`/api/` → 301) lands in the host router.
- NIT (fixed): MTA-STS policy `Cache-Control: no-cache`, as `netray site` served it.
- NIT: an absolute-form request target loses to the `Host` header (RFC 9112 §3.2.2); Traefik sends origin-form.
- NIT: `netray lens`'s kept `ip_url` warning stays silent when `ip_url` is set without `[modules.ip]` (enrichment off); `netray lens` goes in Phase 7.

## Phase 3 — Redirects and 410

### Criteria

Each row also covers the Phase 3 scenarios that name it. Redirects apply to GET and HEAD, a 410 to any method; every `Location` is absolute `https://<apex>…`; every 410 from an `/api` or `/api-docs` path carries `{"error":{"code":"gone","message":…}}` naming the successor; `/health`, `/ready` match first.

| id | criterion | status | test file |
|---|---|---|---|
| C1 | 13a: `lens.<apex>` any path and query but probes → 301 `https://<apex><path>[?<query>]` | green | crates/web/tests/host_redirects.rs |
| C2 | 13b: apex `/?d=<value>` → 301 absolute `https://<apex>/<domain>`; unparsable → 400 with the query refilled | green | crates/web/tests/host_redirects.rs |
| C3 | 13c: `email.<apex>/?q=<domain> <selectors…>` → 301 `https://<apex>/<domain>/email` from the first token | green | crates/web/tests/host_redirects.rs |
| C4 | 13d: `email.<apex>/?domain=<domain>` → the same; `q` wins over `domain` | green | crates/web/tests/host_redirects.rs |
| C5 | 13e: `email.<apex>/` without q/domain, unparsable domain, or any other path (not f–h, probes) → 301 `https://<apex>/` | green | crates/web/tests/host_redirects.rs |
| C6 | 13f: `/api/*` on `dns.`, `tls.`, `http.`, `email.`, `ip.` → 410 naming `https://<apex>/api/v1` | green | crates/web/tests/host_redirects.rs |
| C7 | 13g: `email.<apex>/inspect`, `/inspect/<domain>` → 410 naming `https://<apex>/api/v1/email/inspect` | green | crates/web/tests/host_redirects.rs |
| C8 | 13h: `/docs`, `/docs/` on the five subdomains → 301 `https://<apex>/api/v1/docs`; `/api-docs/openapi.json` there → 410 naming `https://<apex>/api/v1/openapi.json` | green | crates/web/tests/host_redirects.rs |
| C9 | 13i: apex `/api/check`, `/api/check/<domain>` → 410 naming `https://<apex>/api/v1/domains/{domain}/runs`; other apex `/api/<x>` (x ≠ v1) → 410 naming `https://<apex>/api/v1`; row m first | green | crates/web/tests/host_redirects.rs |
| C10 | 13j: apex `/r/<id>`, `/r/<id>/og.png`, `/r/<id>/<protocol>` for a well-formed non-entry ID → 410 (page names no domain; og.png empty body); malformed → 404; stored entry → 200 | green | crates/netray/tests/serve_entries_gone.rs |
| C11 | 13k: `dns./?q=…`, `tls./?h=…`, `http./?url=…`, `ip./?ip=…` → 200 inspector; `dns./?r=` ignored | test-unwritable (deferred until v2-inspectors merges; Decision log 2026-10-18) | |
| C12 | 13l: apex `/tools`, `/tools/` → 301 `https://<apex>/` | green | crates/web/tests/host_redirects.rs |
| C13 | 13m: apex `/api/`, `/api/<name>.html` (index, dns, tls, http, email, ip, lens) → 301 `https://<apex>/api/v1/docs` | green | crates/web/tests/host_redirects.rs |
| C14 | 13n: apex `/guide/lens` → 301 `https://<apex>/guide/scoring` | green | crates/web/tests/host_redirects.rs |
| C15 | 13o: apex `/docs`, `/docs/` → 301 `https://<apex>/api/v1/docs`; apex `/api-docs/openapi.json` → 410 naming `https://<apex>/api/v1/openapi.json` | green | crates/web/tests/host_redirects.rs |
| C16 | 14: every `ip.` route of the V1 router but `/`, probes and `/docs` → 410 naming its successor (json/yaml/toml/csv/all/ip → `/api/v1/ip`; `/ip/cidr`, `/range` → `/api/v1/ip/range/{cidr}`; `/asn[/…]` → `/api/v1/asn/{n}`; field family bare and `/{fmt}` → `/api/v1/ip?field=<name>`; `/ipv4`, `/ipv6` → `/api/v1/ip/{address}` naming `curl -4`/`curl -6`; `/batch` → `/api/v1/ip/batch`; `/diff` → `/api/v1/ip/diff`; `/meta`, `/api/meta` → `/api/v1/meta`; `/api-docs/openapi.json` → `/api/v1/openapi.json`) | green | crates/web/tests/host_redirects.rs |
| C17 | 14 neighbour: `ip./` with a curl user agent → 200 `text/plain` caller address; a browser gets the inspector | test-unwritable (deferred until v2-api and v2-inspectors merge: `lookup_caller` and the inspector are theirs; Decision log 2026-10-18) | |
| C18 | 15: a repo test fails when a V1 route (list kept in the test) or a `site/` path of rows l–n has no row in the table-driven test | already_implemented (the check passes once the table exists; its self-tests fail on a removed row) | tests/repo/test_v1_route_coverage.sh |
| C19 | orchestrator 2026-10-18: no template under `crates/web/templates/` names a V1 subdomain API path (`<sub>.netray.info/api/…`, `email.netray.info/inspect…`, `ip.netray.info/<cut route>`); the guide samples name their `/api/v1/…` equivalents; a repo check holds it | green | tests/repo/test_no_v1_samples.sh |
| C20 | Decision log 2026-10-18: `scoring.html`'s `?d=` sample carries no `dkim_selectors` | green | tests/repo/test_no_v1_samples.sh |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1–G3 (one coder; disjoint files, one build) | 1 | sonnet | 67403 | 210 |
| review fix (extensionless docs URLs, failed lookup 500, scoring sentence) | 1 | sonnet | n/a | n/a |

### Behavioural verification

`netray serve` on 127.0.0.1:18123; `curl -H "Host: <h>" <path>`:

```
lens.netray.info     /r/abcd1234?x=1          301 https://netray.info/r/abcd1234?x=1
netray.info          /?d=Example.COM          301 https://netray.info/example.com
email.netray.info    /?q=example.com%20google 301 https://netray.info/example.com/email
dns.netray.info      /api/query               410 {"error":{"code":"gone","message":"This endpoint has been removed. Use https://netray.info/api/v1."}}
ip.netray.info       /country                 410 {"error":{"code":"gone","message":"… Use https://netray.info/api/v1/ip?field=country."}}
netray.info          /api/check/example.com   410 {"error":{"code":"gone","message":"… Use https://netray.info/api/v1/domains/{domain}/runs."}}
netray.info          /tools                   301 https://netray.info/
```

### Review

- AMENDMENT (repaired in phase): the V1 site served its API docs extensionless (`/api/dns`, sitemap and canonicals) and at `/api`; row 13m now covers `/api`, `/api/index` and `/api/<name>` beside `/api/<name>.html` (rows and coverage list extended).
- AMENDMENT (repaired in phase): `/r/<id>/<protocol>` folded a failed store lookup into 404; it answers 500 with a WARN naming the ID, as `/r/<id>` does.
- DEFERRED (repaired in phase): the scoring guide's new clause tied `dkim_selectors` to the runs endpoint; reverted to main's wording, the section stays with Phase 8.
- DEFERRED: until `v2-result-page` replaces lens's SPA at the apex `/` (or Phase 7 deletes it), a build of this branch runs no check from the apex page: the SPA calls `/api/meta` and `/api/check/<domain>`, now 410. The branch merges only after all eight features (X6).
- NIT: the DNS sample still says "lint results" (false in V1 too); Phase 8 prose.
- NIT: `POST /?d=…` falls through to the apex router instead of 405.
- NIT: `ip./all/cidr` answers 404, not 410.
- NIT: `email./?domain=..` redirects to `https://<apex>/./email`.

## Phase 4 — Metrics

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R16: every §9.2 metric registered at startup with its HELP line and §9.2's label names; the exposition shows each name before the first request | green | crates/netray/tests/serve_metrics.rs |
| C2 | R17: every name `serve` exports starts with `netray_`; no `lens_`, `prism_`, `tlsight_`, `spectra_`, `beacon_`, `ifconfig_` name; surviving operational series renamed `netray_<suffix>`; `lens_admission_rejections_total` → `netray_admission_rejections_total`, `lens_runs_in_flight` → `netray_runs_in_flight`, `lens_history_record_errors_total` → `netray_history_record_errors_total`; `lens_check_requests_total`, `lens_badge_requests_total` end; `test_lens_admission_metrics.sh` rewritten | green | crates/netray/tests/serve_metrics.rs; lens metric tests renamed; tests/repo/test_lens_admission_metrics.sh |
| C3 | R18: a registry test drives one stub run of `example.com` (192.0.2.1) through page, run, badge, OG, `/api/v1` and one inspector job; no label name in {domain, host, address, ip, client, target, url, query, name, id}; no label value `example.com`, `192.0.2.1` or an entry ID; every `endpoint` value a route template | green | crates/lens/tests/netray_metrics.rs (page, run, badge, OG; `/api/v1` and inspector halves deferred) |
| C4 | R19: `netray_runs_total{outcome,trigger}` (visit, recheck, api, badge, og), `netray_runs_with_predecessor_total`, `netray_run_interval_seconds` on record, `netray_fixes_observed_total{section}`, `netray_admission_rejections_total{reason}` (throttle, budget, limit, slots), `netray_history_entries`/`netray_history_domains` at startup and after every sweep — each held by a test | green | crates/lens/tests/netray_metrics.rs |
| C5 | GIVEN `serve` started WHEN `/metrics` is read before any request THEN every §9.2 name has a HELP line | green | crates/netray/tests/serve_metrics.rs |
| C6 | GIVEN the exposition WHEN every name is listed THEN each starts with `netray_`, none with a V1 prefix | green | crates/netray/tests/serve_metrics.rs |
| C7 | GIVEN one stub run of `example.com` at 192.0.2.1 through page, run, badge, OG, API and an inspector job WHEN read THEN no forbidden label name and no label value `example.com`, `192.0.2.1` or the entry ID | green | crates/lens/tests/netray_metrics.rs |
| C8 | GIVEN `GET /api/v1/domains/example.com` WHEN read THEN `netray_api_requests_total{endpoint="/api/v1/domains/{domain}"}` | test-unwritable (deferred until v2-api merges: no `/api/v1` on main; the name is registered at startup now) | |
| C9 | GIVEN a recheck recording a run with a predecessor and one fixed transition in tls WHEN read THEN `netray_runs_total{trigger="recheck",outcome="complete"}`, `netray_runs_with_predecessor_total`, `netray_fixes_observed_total{section="tls"}` each +1 | green | crates/lens/tests/netray_metrics.rs |
| C10 | GIVEN a badge request that starts a run WHEN read THEN `netray_runs_total{trigger="badge"}` +1; same for an OG card with `trigger="og"` | green | crates/lens/tests/netray_metrics.rs |
| C11 | GIVEN a throttled run WHEN read THEN `netray_admission_rejections_total{reason="throttle"}` +1 | green | crates/lens/tests/netray_metrics.rs |
| C12 | GIVEN a sweep deleting one domain WHEN read THEN `netray_history_domains` −1 | green | crates/lens/tests/netray_metrics.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 2 | sonnet | 140254 | 825 |
| review fix (section outcomes, OG refusal) | 1 | sonnet | n/a | n/a |

### Behavioural verification

`netray serve` started; the metrics listener read before any request but the `/health` poll:

```
$ curl -s 127.0.0.1:18124/metrics | grep -c '^# HELP'
18
$ … | grep '^# TYPE' | awk '{print $3}' | grep -v '^netray_'
(none)
# TYPE netray_api_requests_total counter
# TYPE netray_runs_total counter
# TYPE netray_history_domains gauge
# TYPE netray_run_interval_seconds histogram
netray_history_entries 0
```

### Review

- AMENDMENT (repaired in phase): `netray_section_outcomes_total` counted every answering section `measured`, also one the scorer makes incomplete; it now follows the scorer (`incomplete` for a weighted unmeasured check, `not_tested` when every check is not tested).
- AMENDMENT (repaired in phase): a protocol without a registered module counted `not_tested` on every run; it counts nothing.
- AMENDMENT (repaired in phase): a refused OG card counted `netray_badge_requests_total{cache="throttled"}` (carried from `v2-admission`); it counts only the rejection.
- DEFERRED: lens maps `NotTested` to `Skip` → `NotApplicable` before the run is built (`modules.rs:393`, `score.rs:38`), so `count_run` counts `not_tested` for a measured section whose checks are all not tested or not applicable; a section of only genuine not-applicable checks is labelled `not_tested` too. Exact once the verdict mapping leaves lens (Phase 6).
- DEFERRED: under `netray lens` the ip module's load gauges are set before the recorder is installed and stay absent until a SIGHUP; `netray serve` installs first. `netray lens` goes in Phase 7.
- NIT: the predecessor head is read outside the store's write lock (a sweep racing a record can count a predecessor for a `First` record); deriving it from `Recorded::Identical | Changed` would avoid it.
- NIT: the history gauges move only at startup and after the hourly sweep (R19's wording), not on record.
- NIT: `netray_run_interval_seconds`' top finite bucket is 30 days under a 90-day retention.

## Phase 5 — Agreement

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R20: `crates/netray/tests/agreement.rs` builds the `serve` application with stub modules and a temporary store and runs four states (complete with warn and fail; a fixed check against a stored predecessor; N/A by design, no MX; not tested from here); for each, the result page, open issues, every evidence view, the badge, the OG card's geometry and `/api/v1` show the same grade, the same status per check ID and the same warn/fail set; runs in `just adlc-verify`, offline | already_implemented for the views on main (entry page, badge, OG card, entry card, stored run); the result page, issues, evidence views, `/api/v1` and the stream join at the v2-api/v2-inspectors/v2-result-page merges (Decision log 2026-10-18) | crates/netray/tests/agreement.rs |
| C2 | R21: an incomplete state (one section times out after a stored entry exists): the stream and `/api/v1`'s run answer say `incomplete` with the same statuses; badge, OG card and `GET /api/v1/domains/{domain}` show the stored entry byte-for-byte unchanged | already_implemented for the views on main (entry page, badge, OG card, entry card, stored run); the result page, issues, evidence views, `/api/v1` and the stream join at the v2-api/v2-inspectors/v2-result-page merges (Decision log 2026-10-18) | crates/netray/tests/agreement.rs |
| C3 | R22: `tests/acceptance/agreement/prod.spec.ts` (netray.info and example.com, one run each via `POST /api/v1/domains/{domain}/runs` without streaming; page grade = API grade = badge letter; open issues = API warn/fail; every evidence view names the same entry; request contexts only, no browser navigation); `prod` = that spec + `smoke/health`, `smoke/ready`, `smoke/security-headers`, `smoke/tls-certs`, held by `test_acceptance_prod_subset.sh`; `fixtures/env.ts` drops the `lens` origin (production: apex + four inspector hosts; local: one origin by `Host`); V1 specs (`services/*`, `api-contract` against `/api/meta`, `integration/cross-links`, `static-site`) removed or rewritten | green | tests/repo/test_acceptance_prod_subset.sh |
| C4 | GIVEN each of the four complete states WHEN read through page, issues, evidence views, badge, OG geometry and `/api/v1` THEN grade, per-check statuses and warn/fail set equal | already_implemented for the views on main (entry page, badge, OG card, entry card, stored run); the result page, issues, evidence views, `/api/v1` and the stream join at the v2-api/v2-inspectors/v2-result-page merges (Decision log 2026-10-18) | crates/netray/tests/agreement.rs |
| C5 | GIVEN a stored entry and a run whose email section times out WHEN streamed THEN stream and API run say `incomplete` with equal statuses; badge, OG and `GET /api/v1/domains/{domain}` equal their answers from before the run | already_implemented for the views on main (entry page, badge, OG card, entry card, stored run); the result page, issues, evidence views, `/api/v1` and the stream join at the v2-api/v2-inspectors/v2-result-page merges (Decision log 2026-10-18) | crates/netray/tests/agreement.rs |
| C6 | GIVEN the fixed-check state WHEN the page and `/api/v1/domains/{domain}/diff` are read THEN both name the same check as fixed | test-unwritable (deferred until v2-api and v2-result-page merge: no result page or `/diff` on main; Decision log 2026-10-18) | |
| C7 | GIVEN `playwright.config.ts` WHEN `test_acceptance_prod_subset.sh` runs THEN `prod` lists exactly the five specs of requirement 22 | green | tests/repo/test_acceptance_prod_subset.sh |
| C8 | GIVEN `agreement/prod.spec.ts` WHEN grepped THEN it navigates no page with a browser and names no domain other than `netray.info` and `example.com` | green | tests/repo/test_acceptance_prod_subset.sh |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 75362 | 99 |

### Behavioural verification

skipped: the Phase 5 entry point is the Playwright `prod` project against a deployed 0.30.0 (the operator's run after the deploy) or `just acceptance-local`, which still starts the V1 processes until Phase 7 rewrites it. Offline, the reviewer type-checked the staged suite (`tsc --noEmit` clean) and listed it (`playwright test --list`: 33 tests in `prod`); `crates/netray/tests/agreement.rs` runs in the gate.

### Review

- AMENDMENT (repaired in phase): `viewIssues` collected every check row, so passed checks and the evidence copies would fail the issues comparison on a page that follows the SDD; it now takes warn and fail rows once each (selector scope stays a `TODO(v2-result-page)`).
- AMENDMENT (repaired in phase): health, ready and header checks on `dns.`, `tls.`, `http.`, `ip.` pass against the V1 containers as well; `smoke/health.spec.ts` adds a per-host `/docs` → 301 to the apex's `/api/v1/docs` (row 13h), which only the one process answers.
- DEFERRED: `just acceptance-local` neither sets `LOCAL_URL` nor starts `serve`, and its default list names the removed `static-site/assets.spec.ts`; Phase 7 (inventory).
- Kept in the rewrite: the CORS preflight on `/health` (lens's layers answer it under `serve` since Phase 2).
