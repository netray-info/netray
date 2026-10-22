# netray-common

Shared library of the `netray` binary: code used by `netray-web`, `netray-engine`, the module
crates and `crates/netray` itself. A workspace member, versioned with the workspace; crates depend
on it through the root `[workspace.dependencies]`:

```toml
netray-common = { workspace = true, features = ["..."] }
```

## Modules

| Module | Feature | Purpose |
|--------|---------|---------|
| `config` | | The one config loader: `load::<T>(path, prefix)` reads an optional TOML file and environment variables under `prefix` (`<PREFIX><SECTION>__<KEY>`; `<NAME>_CONFIG` names the file and is no key). `load_with_env` takes the variables as an argument; `refuse_legacy_prefix` fails when a variable of a retired prefix is set and names the new one |
| `cors` | `cors` | `cors_layer()`: any origin, `GET`/`POST`/`OPTIONS`, `content-type` and `accept` request headers, preflight cached 600 s |
| `domain` | | `validate()`: the domain-name input rule. Maps the input with UTS #46 to its A-label form without a trailing dot; refuses IP addresses (also bracketed or with a numeric last label), wildcards, labels over 63 and names over 253 characters. No DNS resolution |
| `ecosystem` | | `EcosystemConfig`, `EcosystemMeta`, `EcosystemUrls`, `RateLimitSummary`: the per-service meta document (service URLs, feature flags, limits, rate-limit summary). No workspace crate uses it |
| `enrichment` | `enrichment` | `EnrichmentClient::in_process(source)`: ASN, cloud and threat-flag metadata (`IpInfo`) answered by an in-process `EnrichmentSource`; private and blocked addresses never reach the source |
| `error` | | `ApiError` trait; `into_error_response` (logs, then builds) and `build_error_response` produce `{"error": {"code": "...", "message": "..."}}` with `Retry-After` when the error carries one |
| `fetch` | `fetch` | Outbound HTTP to user-supplied URLs: `fetch` / `fetch_traced` build one reqwest client per call whose resolver checks every address, on every redirect hop; IP-literal hosts are checked before the request and in the redirect policy; environment proxies are ignored |
| `ip_extract` | | `IpExtractor`: the client IP from `X-Real-IP`, then the rightmost untrusted `X-Forwarded-For` entry, only when the peer is in the trusted-proxy CIDR list; an empty list returns the peer address |
| `ip_filter` | | `is_blocked_ip()`: the complement of `target_policy::is_allowed_target` |
| `metrics` | | `MetricsNamespace`: sealed marker trait supplying a static metrics prefix. No workspace crate uses it |
| `middleware` | `middleware` | `request_id` (reuses a valid incoming `X-Request-Id`, else a UUID v7; stored as `RequestId` and echoed on the response) and `http_metrics` (`{prefix}_http_requests_total`, `{prefix}_http_request_duration_seconds`, labelled by matched route) |
| `prefetch` | | `is_prefetch()`: true when `Sec-Purpose` holds `prefetch` or `prerender` |
| `rate_limit` | | `KeyedLimiter`, `check_keyed_cost`, `check_direct_cost`, `check_direct_cost_scoped` over `governor`; a rejection carries `retry_after_secs` and its scope and counts `{prefix}_rate_limit_hits_total` |
| `security_headers` | | `security_headers_layer(SecurityHeadersConfig)`: middleware setting CSP (relaxed by `extra_script_src` under `relaxed_csp_path_prefix`, default `/docs`), HSTS, `X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`, `Permissions-Policy`, COOP and CORP; `keep_inner` keeps headers the inner response already set |
| `server` | `server` | `shutdown_signal()` (SIGINT or SIGTERM); with `prometheus`: `metrics_recorder`, `install_metrics` and `serve_metrics_handle` (a metrics-only server at `/metrics`) |
| `target_policy` | | `is_allowed_target()` / `refusal_reason()`: the SSRF blocklist over every reserved or special-purpose IPv4 and IPv6 range; IPv4-mapped addresses are judged by their IPv4 address |
| `telemetry` | `telemetry` | `TelemetryConfig`, `validate`, `init_subscriber` (env filter, `text` or `json` output, OTLP export when enabled) and `shutdown` (flushes spans) |

Other features: `schema` derives `utoipa::ToSchema` on the error and ecosystem types; `otel` adds
the OpenTelemetry dependencies; `prometheus` enables `server` and the Prometheus exporter.

## Testing

From the repository root (verbs in the root `justfile`):

```sh
just check                                    # the full suite, this crate included
cargo test -p netray-common --test it <module> # one integration-test module (crates/common/tests/it/)
```

The `fetch*`, `enrichment_in_process` and `metrics_recorder` test targets declare
`required-features`; `cargo test --workspace` enables those features through feature
unification, `cargo test -p netray-common` alone skips them.

## License

MIT
