# netray-http

The HTTP module of `netray serve`: a library crate (V1 codename spectra) with no binary of its
own. It provides

- `HttpModule`, the V2 engine module for protocol `http` (`netray_engine::Module`), which the
  binary's module registry builds from `[modules.http]`;
- the inspection job (`job::inspect`, trait `HttpInspect`) that netray-web runs for API v1
  `POST /api/v1/http/inspect` and the inspector page `/inspect/http`.

## What it measures

The module asks the engine for address records and connects to the target's `A`, then `AAAA`
addresses through the target policy; without them it resolves the name itself. Each run
charges the module's per-target limiter, then fires three probes concurrently under one total
timeout:

| Probe | Request |
|---|---|
| HTTPS | `https://<domain>`, following redirects up to `max_redirects`, every hop recorded |
| Port 80 | the same URL with scheme `http` on port 80: does it redirect to HTTPS |
| CORS | the HTTPS URL with `Origin: https://evil.example.com` |

Outbound requests go through `netray_common::fetch`; response bodies are not read. When the
binary has an IP module, the answering address is enriched in process (org, network type).

Evidence blocks: `http.redirects`, `http.port80`, `http.port80_probe`, `http.headers`,
`http.server`, `http.cors`, `http.cookies`, `http.csp`, `http.hsts`, `http.caching`, `http.cdn`,
`http.isolation` (`REGISTER` in `src/module.rs`).

## Checks

Thirteen ids (`checks()` in `src/module.rs`; texts in `crates/engine/catalogue/checks.toml`):

| Id | Title |
|---|---|
| `http.https_redirect` | HTTPS redirect |
| `http.hsts` | HSTS |
| `http.cors` | CORS policy |
| `http.cookie_secure` | Secure cookies |
| `http.csp` | Content Security Policy |
| `http.x_frame_options` | X-Frame-Options |
| `http.x_content_type_options` | X-Content-Type-Options |
| `http.referrer_policy` | Referrer-Policy |
| `http.permissions_policy` | Permissions-Policy |
| `http.coop` | Cross-Origin-Opener-Policy |
| `http.deprecated_headers` | Deprecated headers |
| `http.info_leakage` | Information leakage |
| `http.caching` | Cache control |

A probe that got no answer (error kind `timeout`, `blocked` or `unreachable`) makes its checks
`unmeasured`. A refused, reset or failed-TLS HTTPS probe fails the eleven response checks; the
same on the CORS probe makes `http.cors` not applicable (`translate` in `src/module.rs`).

## Configuration

All keys live in `netray.toml` (see `crates/netray/netray.example.toml`); every table is
optional and falls back to the defaults below.

`[modules.http]` (`ModuleConfig` in `src/config.rs`; `deadline_ms` is read by the binary):

| Key | Default | Meaning |
|---|---|---|
| `deadline_ms` | 2000 | the section window of the module in an engine run |
| `inspect.request_timeout_secs` | 10 | per-probe timeout |
| `inspect.total_timeout_secs` | 30 | cap across all three probes |
| `inspect.max_redirects` | 10 | redirect hops followed per probe |
| `inspect.user_agent` | `netray-spectra` | sent as `<user_agent>/<version>` |
| `limits.per_target_per_minute` | 30 | per-host quota of the module; zero is refused |
| `limits.per_target_burst` | 10 | per-host burst; zero is refused |

`[api.http]` (`ApiHttpConfig` in `crates/web/src/api/config.rs`), the limiters of
`POST /api/v1/http/inspect`; zero is refused:

| Key | Default |
|---|---|
| `per_ip_per_minute` | 10 |
| `per_ip_burst` | 5 |
| `per_target_per_minute` | 30 |
| `per_target_burst` | 10 |

`[inspect.http]` (`InspectLimits` in `crates/web/src/pages/inspect.rs`), the budget of the
`/inspect/http` page: `deadline_ms` (30000), `max_concurrent` (8).

## Testing

From the repository root:

```sh
just check                                                  # gate plus the full Rust suite
cargo test -p netray-http --test it <module>                # one integration test module
cargo test -p netray-http --features testing --test module  # the testing-feature targets
cargo test -p netray-http --features testing --test job
```

## License

MIT
