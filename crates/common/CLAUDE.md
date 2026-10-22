# CLAUDE.md -- netray-common

Shared library of the `netray` binary (used by `netray-web`, `netray-engine`, the module crates
and `crates/netray`). Workspace rules, verbs and the gate: the root [`CLAUDE.md`](../../CLAUDE.md);
they are not repeated here. Module table: [`README.md`](README.md).

- Edition 2024 (workspace), `rust-version = "1.88"` in `Cargo.toml`; the toolchain is `stable`
  (`rust-toolchain.toml`). CI is the root `.github/workflows/` (`ci.yml`, `release.yml`).
- Version comes from `[workspace.package]`; the crate has no release of its own, changes go to
  the root `CHANGELOG.md`.

## Rules

- **Integration tests** go into `tests/it/` with a `mod` line in `tests/it/main.rs` (one test
  binary `it`). A separate target only with `required-features` (the `fetch*`,
  `enrichment_in_process` and `metrics_recorder` targets).
- **Feature-gated modules** (`cors`, `enrichment`, `fetch`, `middleware`, `server`, `telemetry`)
  are enabled by the consuming crate's `features = [...]`; features unify across the binary.
- **`config::load` / `load_with_env`** is the only loader: precedence env > TOML file > serde
  defaults, key separator `__`, `<NAME>_CONFIG` is never a key. The binary loads with prefix
  `NETRAY_` and calls `refuse_legacy_prefix` for retired prefixes. Config structs defined here
  (`TelemetryConfig`) carry `deny_unknown_fields`.
- **`IpExtractor`**: an empty trusted-proxy list ignores every proxy header; bare IPs are
  promoted to `/32` or `/128`; invalid entries are skipped with a warning; `X-Forwarded-For` is
  walked right to left, skipping trusted proxies. `CF-Connecting-IP` is never read.
- **`target_policy`** is the one SSRF blocklist; `ip_filter::is_blocked_ip` and
  `enrichment::is_private_ip` delegate to it. A new refused range goes there only.
- **Outbound fetches** of user- or domain-controlled URLs go through `fetch`; it takes
  `ClientSettings`, never a reqwest builder, so no caller can route around the checking resolver.
- **Errors**: each caller defines its own error type and implements `ApiError`;
  `build_error_response` is for callers that log themselves.
- **Rate-limit and HTTP metrics** take a `metrics_prefix`: `{prefix}_rate_limit_hits_total`
  (label `scope`), `{prefix}_http_requests_total`, `{prefix}_http_request_duration_seconds`.
- **Security headers** follow [`specs/rules/architecture-rules.md`](../../specs/rules/architecture-rules.md)
  §Security Headers; HSTS defaults to one year with `includeSubDomains; preload`.
- **Telemetry** follows [`specs/rules/logging-rules.md`](../../specs/rules/logging-rules.md);
  the caller calls `telemetry::shutdown()` on graceful shutdown to flush spans.
