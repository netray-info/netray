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
