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
