
## main..c620c7c

### Reader

COUNTS blockers=3 majors=1 minors=0
LENSES Engineering, Security, Testing
BLOCKER | crates/ifconfig-rs/src/config.rs:225 | `netray ip --check-config` prints `config ok` and exits 0 for a bind address that startup rejects. `validate()` never parses `server.bind` or `admin_bind`, and both are `String` fields. | Ran it. I took the ifconfig production fixture and set `bind = "0.0.0.0"`. `--check-config` printed `config ok` and exited 0. `netray ip` on the same file panicked at crates/ifconfig-rs/src/lib.rs:247 with `Invalid bind address: AddrParseError(Socket)` and exited 101.
BLOCKER | crates/beacon/src/config.rs:171 | beacon's `validate()` checks only `rate_limit.per_ip`. So `netray email --check-config` passes an unparsable `server.metrics_bind` or `server.bind` (both `String`), and startup refuses it. This contradicts commit c620c7c's claim to cover what beacon refuses at startup. | Ran it. I took the beacon production fixture and set `metrics_bind = "0.0.0.0"`. `--check-config` printed `config ok` and exited 0. `netray email` on the same file panicked at crates/beacon/src/lib.rs:141 with `invalid metrics_bind address` and exited 101.
BLOCKER | crates/lens/src/config.rs:440 | lens's `validate()` does not reject `badges.ttl_seconds = 0`, and `AppState::new` panics on that value. So `netray lens --check-config` approves a config that crash-loops, against R3's "every subcommand's validate() covers what its startup rejects". | Ran it. I added `[badges]` / `ttl_seconds = 0` to the lens production fixture. `--check-config` printed `config ok` and exited 0. `netray lens` on the same file panicked at crates/lens/src/state.rs:75 with `badge ttl_seconds must be non-zero` and exited 101.
MAJOR | crates/tlsight/src/config.rs:342 | The `custom_ca_dir` existence check now sits in `validate()`, which tlsight's SIGHUP reload calls through `Config::load`. A missing CA directory therefore aborts the whole reload: the old custom CAs stay trusted, every other config edit is dropped, and the reload path's tolerant branch at crates/tlsight/src/state.rs:161 can no longer be reached. | Traced through the code, not run. tlsight runs with `custom_ca_dir = "/etc/tlsight/cas"`. The operator deletes that directory to stop trusting its CA, changes `limits.per_ip_per_minute` and sends SIGHUP. Before this change: the trust store was rebuilt from the Mozilla roots only, a warning was logged, and the new limit applied. After: crates/tlsight/src/reload.rs:49 gets `Err` from `Config::load` and returns. The removed CA stays in the trust store and the old limit stays in force.

```quote crates/ifconfig-rs/src/config.rs:225
    pub fn validate(&self) -> Result<(), config::ConfigError> {
```

```quote crates/ifconfig-rs/src/lib.rs:247
    let bind_addr: SocketAddr = config.server.bind.parse().expect("Invalid bind address");
```

```quote crates/beacon/src/config.rs:171
    fn validate(&self) -> Result<(), config::ConfigError> {
```

```quote crates/beacon/src/lib.rs:141
        .expect("invalid metrics_bind address");
```

```quote crates/lens/src/config.rs:440
    pub fn validate(&mut self) -> Result<(), ConfigError> {
```

```quote crates/lens/src/state.rs:75
            .expect("badge ttl_seconds must be non-zero")
```

```quote crates/tlsight/src/config.rs:342
        if let Some(dir) = &self.validation.custom_ca_dir
```

```quote crates/tlsight/src/reload.rs:49
    let new_config = match Config::load(config_path) {
```

```quote crates/tlsight/src/state.rs:161
            tracing::warn!(dir = dir, "custom_ca_dir not found during reload");
```

### Refuted

None.

### Calibration

| Finding | Refuter | Confidence | Result | Command |
|---|---|---|---|---|
| F1 ifconfig-rs bind not validated | CONFIRMED | 9 | held | `netray ip --check-config` on fixture with `bind = "0.0.0.0"` → `config ok`, rc 0 |
| F2 beacon metrics_bind not validated | CONFIRMED | 8 | held | `netray email --check-config` on fixture with `metrics_bind = "0.0.0.0"` → `config ok`, rc 0 |
| F3 lens `badges.ttl_seconds = 0` not validated | CONFIRMED | 9 | held | `netray lens --check-config` on fixture plus `[badges] ttl_seconds = 0` → `config ok`, rc 0 |
| F4 tlsight reload drops the whole config on a missing `custom_ca_dir` | CONFIRMED | 8 | held | read `crates/tlsight/src/reload.rs:49-56` and `state.rs:155-161` |

### Summary

Before refutation: 3 blockers, 1 major, 0 minors. After: 3 / 1 / 0. Verified 4, held 4. No `[principles]` declared, no roll call.

## c620c7c..3544642

### Reader

COUNTS blockers=1 majors=0 minors=1
LENSES Engineering, Security, Testing
BLOCKER | crates/common/src/telemetry.rs:135 | `netray <sub> --check-config` still prints `config ok` and exits 0 for a config that startup refuses. The commit and spec R3 promise that check-config rejects "every value startup rejects", but no `validate()` checks the OTLP endpoint, and startup panics on a bad one. | I confirmed this on the existing `target/debug/netray`. I took `crates/beacon/tests/fixtures/beacon.production.toml` and added `otlp_endpoint = "http://bad host:4318"` under `[telemetry]`. `netray email --check-config` then returned rc=0, while `netray email <same file>` returned rc=101 with `failed to initialize OpenTelemetry: InvalidUri("http://bad host:4318", "invalid uri character")`. Doing the same to the lens fixture (adding `enabled = true` plus the bad endpoint) also returned rc=0. The panic site is shared code that all six subcommands use, so a config that crash-loops passes the operator check on every one of them.
MINOR | crates/tlsight/src/config.rs:276 | The new `check_startup` copies only the `is_dir` half of the startup check. Startup also panics when `read_dir` fails (`crates/tlsight/src/state.rs:89`), so `--check-config` passes a CA directory that startup refuses. | I confirmed this on the binary. I set `custom_ca_dir` to an existing directory with mode 000. `netray tls --check-config` returned rc=0 (`config ok`), while `netray tls <same file>` returned rc=101 with `failed to read custom_ca_dir …: Permission denied (os error 13)`. I rate this minor because spec R3 commits only to a *missing* CA directory and puts host-side file state out of scope.

```quote crates/common/src/telemetry.rs:135
        let otel_layer = init_otel_layer(config).expect("failed to initialize OpenTelemetry");
```

```quote crates/tlsight/src/config.rs:276
            && !std::path::Path::new(dir).is_dir()
```

```quote crates/tlsight/src/state.rs:89
        .unwrap_or_else(|e| panic!("failed to read custom_ca_dir {ca_dir}: {e}"));
```

### Refuted

None.

### Calibration

| Finding | Refuter | Confidence | Result | Command |
|---|---|---|---|---|
| F1 invalid OTLP endpoint passes `--check-config`, panics at startup | CONFIRMED | 9 | held | `netray email --check-config` on beacon fixture with `otlp_endpoint = "http://bad host:4318"` → `config ok`, rc 0 |

### Summary

Before refutation: 1 blocker, 0 majors, 1 minor. After: 1 / 0 / 1. Verified 1, held 1. No `[principles]` declared, no roll call. The minor (tlsight CA dir readable, not only present) is host state, outside R3.

## 3544642..d2c4218

### Reader

COUNTS blockers=0 majors=0 minors=1
LENSES Engineering, Testing

MINOR | tests/repo/test_check_config.sh:114 | The `grep 'otlp_endpoint'` check is meant to stop a row from passing on an unknown-field error (the comment at lines 93-94 says so), but it cannot catch that case. serde's unknown-field message lists every expected field, and `otlp_endpoint` is one of them. | A beacon row with `enabled = true` injected exits 1 with "unknown field `enabled`, expected one of `log_format`, `otlp_endpoint`, …" and passes both checks; the six committed rows avoid this, so each does exercise its service's validate wiring today.

```quote tests/repo/test_check_config.sh:114
    grep -q 'otlp_endpoint' <<<"$out" || fail "$sub: invalid otlp_endpoint error does not name otlp_endpoint ($out)"
```

Traced and sound: `validate` parses with the same `http` 1.5.0 `Uri` parser opentelemetry-otlp 0.31.1 uses; all six `Config::load` validate before `init_subscriber` (also `ip --check`, SIGHUP reloads); `otlp_endpoint = ""` with telemetry enabled is now refused.

### Summary

Before refutation: 0 / 0 / 1. After: 0 / 0 / 1. Verified 0, held 0 (no blocker or major). No roll call. The minor is repaired: the check now greps for `telemetry.otlp_endpoint`, which the unknown-field message never contains.

Measurement note: the review row for `3544642..d2c4218` was recorded with insertions 0 / deletions 0; the range is 89 / 1.

## main..6915288

### Reader

COUNTS blockers=0 majors=3 minors=3
LENSES Engineering, Security, Testing
MAJOR | tests/repo/test_header_parity.sh:153 | The `netray site` server started for the D4 check is never killed: `start_bg` runs inside a `( … )` subshell, so its PID is added to the subshell's `_NETRAY_PIDS`, not the parent's, and `_netray_cleanup` at EXIT never sees it. | Every `just test-repo` / `adlc verify` run leaves a `netray site` listening on 127.0.0.1:<free_port> after the script exits. Reproduced with the same helpers and `sleep 30` in place of the binary: the parent recorded 0 PIDs and the child was still alive after the script exited. `test_site.sh` calls `start_bg` without a subshell and does not leak.
MAJOR | crates/beacon/src/lib.rs:131 | beacon's new CORS layer sits outside `request_id`, so CORS preflight responses no longer carry `X-Request-Id`. `specs/rules/architecture-rules.md:52` requires it on every response. | Before the change, `OPTIONS /health` (with Origin and Access-Control-Request-Method: POST) reached the router and returned 405 through the `request_id` layer. Running the HEAD binary (`netray email beacon.dev.toml`), the same request now returns 200 with `access-control-*` and all security headers but no `x-request-id`; `GET /health` still has one.
MAJOR | crates/common/src/cors.rs:9 | prism and tlsight now answer any Origin with `access-control-allow-origin: *`. This contradicts the security checklists in their crate CLAUDE.md files ("CORS restricted to same origin" / "restricted to configured origins"); the change does not update either file, and the plan's docs list does not name them. | Before, `cors_layer()` had no allow_origin, so no ACAO was sent. On the HEAD binary, `netray dns prism.dev.toml` with `GET /api/meta` and `Origin: https://evil.example` returns `access-control-allow-origin: *`. tlsight shares the same layer. Any website can now read prism/tlsight API responses from a visitor's browser, against the stated crate convention.
MINOR | tests/repo/test_header_parity.sh:172 | The R7/C7 guard ("ifconfig-rs sets no HSTS/CSP of its own") only greps for the hyphenated header names in two files, so it passes code that sets those headers through axum's constants. The runtime checks cannot catch this either, because the outer shared layer overwrites any inner value. | Append `h.insert(axum::http::header::STRICT_TRANSPORT_SECURITY, …)` or `CONTENT_SECURITY_POLICY` to a copy of `crates/ifconfig-rs/src/middleware.rs` and run the guard's grep: no match, so the guard passes. The same applies to any new file under `crates/ifconfig-rs/src/` other than `middleware.rs`/`lib.rs`.
MINOR | tests/acceptance/fixtures/security-headers.ts:48 | The acceptance preflight assertion never checks `access-control-allow-headers`, although spec item 8 says the suite asserts item 6 (D3 methods, headers and max-age) per host. | If production answers the preflight with `access-control-allow-headers: accept` (content-type dropped), a browser's cross-origin JSON `POST` fails its preflight, but `smoke/security-headers.spec.ts` "CORS preflight on /health" still passes.
MINOR | tests/acceptance/fixtures/security-headers.ts:35 | `assertToolHeaders` checks that `Server` is absent but not `X-Powered-By`, which spec item 6 forbids and item 8 requires the acceptance suite to assert. | A tool response carrying `X-Powered-By: Express` passes every acceptance header test on both production and local.

```quote tests/repo/test_header_parity.sh:153
(cd "$REPO_ROOT" && start_bg "$tmp/site.log" "$bin" site --bind "127.0.0.1:$port" --root site)
```

```quote crates/beacon/src/lib.rs:131
        .layer(security::cors_layer())
```

```quote specs/rules/architecture-rules.md:52
- Emit `X-Request-Id` on every response.
```

```quote crates/common/src/cors.rs:9
        .allow_origin(Any)
```

```quote crates/mhost-prism/CLAUDE.md:189
- [ ] CORS restricted to same origin
```

```quote crates/tlsight/CLAUDE.md:221
- [ ] CORS restricted to configured origins
```

```quote tests/repo/test_header_parity.sh:172
    if grep -niE 'strict-transport-security|content-security-policy' "$REPO_ROOT/crates/ifconfig-rs/src/$f" >/dev/null 2>&1; then
```

```quote tests/acceptance/fixtures/security-headers.ts:48
export function assertCorsPreflight(response: APIResponse): void {
```

```quote tests/acceptance/fixtures/security-headers.ts:35
  expect(h['server'], 'no server header').toBeUndefined();
```

### Refuted

- F3 `crates/common/src/cors.rs:9` (ACAO `*` on prism/tlsight) — REFUTED, confidence 8: D3/M19 require ACAO `*` on all six; production already sends it via Traefik's `cors-public-api` (observed `curl -sI https://dns.netray.info/` → `access-control-allow-origin: *`, 2026-10-08). The crate CLAUDE.md checklist lines are stale prose, fixed in the spec's closing docs commit.

### Calibration

| Finding | Refuter | Confidence | Result | Command |
|---|---|---|---|---|
| F1 `netray site` leaked by `start_bg` in a subshell | CONFIRMED | 8 | held | read `tests/repo/test_header_parity.sh:153` and `tests/repo/lib/netray.sh:39` |
| F2 preflight lacks `X-Request-Id` | CONFIRMED (five of six services, not only beacon) | 8 | held | started email, dns, http from dev configs; `curl -X OPTIONS … /health` → no `x-request-id` in all three |
| F3 ACAO `*` on prism/tlsight | REFUTED | 8 | not held | production `dns.netray.info` already answers `access-control-allow-origin: *` |

### Summary

Before refutation: 0 blockers, 3 majors, 3 minors. After: 0 / 2 / 3. Verified 3, held 2. No roll call. F1 and F2 are repaired on this branch before finish; the three minors (R7 grep misses axum constants, acceptance preflight misses allow-headers, acceptance misses `X-Powered-By`) are repaired with them.

## 6915288..7862293

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Security, Testing

No wrong result. Traced: `request_id` now wraps CORS, security headers, the body limit and `TraceLayer` in beacon, lens, prism, spectra and tlsight (only `ConcurrencyLimitLayer` outside); tower-http 0.6.11's `CorsLayer` answers `OPTIONS` itself, so preflights now carry the id; `make_span_with` reads the generated id; no route is left unwrapped. The test's site server is started without a subshell and is killed at exit; the R7 grep covers every file and the constant spellings.

### Summary

0 / 0 / 0. Verified 0, held 0. No roll call.

Measurement note: the review row for `6915288..7862293` was recorded with tokens 0 / seconds 0; the reader cost 82,327 tokens, 209 s.

## main..a6813b4

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Security, Testing

No wrong result. Traced: the handler answers the 404 page for a path rejection (`/r/%C0`), a shortid failing `^[0-9A-Za-z]{8}$`, and an unknown or expired id (`SnapshotStore::get` filters `created_at > now - TTL`; the expired test inserts `now - TTL - 3600`). Rebuilt the merged router with the SPA fallback (axum 0.8.9, matchit 0.8.4): `/r/`, `/r//`, `/r//x`, `/r/a/b`, `/r/AAAAAAAA/`, `/r/AAAAAAAA//`, `/r/%C0`, `HEAD /r/a/b`, `/r/%2F` reach the 404 page; only `/r`, `//r/x`, `/R/x` reach the SPA, none a `/r/<id>` path. No route collision, no reference to the removed `INVALID_SHORTID`. Each new test row fails without its fix. With `snapshots.enabled = false` the router is not mounted and `/r/…` keeps the SPA — an operator opt-out, unchanged.

### Summary

0 / 0 / 0. Verified 0, held 0. No roll call.
