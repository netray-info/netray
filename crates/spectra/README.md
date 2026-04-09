# spectra

**HTTP header inspection and security audit — three probes, one report.**

spectra is a web-based tool that fires three concurrent HTTP/HTTPS probes against a target URL, analyzes every response header category, and produces a structured security report with per-check verdicts. No browser extensions, no manual curl juggling — just a URL and a result.

Live at [http.netray.info](https://http.netray.info) · Part of the [netray.info](https://netray.info) toolchain alongside [tls.netray.info](https://tls.netray.info), [dns.netray.info](https://dns.netray.info), and [ip.netray.info](https://ip.netray.info).

---

## What it does

Given a URL, spectra:

- **Validates and normalizes the input** — bare hostnames are auto-prefixed with `https://`; explicit `http://` is rejected (the HTTP probe fires automatically)
- **Fires three concurrent probes** — HTTPS chain, HTTP port-80 upgrade, and CORS probe — all within a single configurable timeout
- **Captures the full redirect chain** — every hop recorded: URL, status code, Location header, and HTTP version
- **Analyzes security headers** — HSTS (max-age, preload), Content Security Policy (directives, unsafe-inline, unsafe-eval, wildcards), X-Frame-Options, X-Content-Type-Options, Referrer-Policy, Permissions-Policy, COOP, COEP, CORP
- **Evaluates CORS policy** — wildcard origins, reflected origins, credentials flag; reflected origin combined with `Access-Control-Allow-Credentials: true` is an automatic Fail
- **Audits every cookie** — Secure, HttpOnly, SameSite, domain scope — per-cookie verdicts
- **Parses caching directives** — Cache-Control flags, ETag, Last-Modified, Vary, Age from upstream proxies
- **Detects CDN presence** — signature-based detection of Cloudflare, CloudFront, Fastly, Akamai, Azure, Vercel, Fly.io, Netlify, BunnyCDN — plus their cache status
- **Flags fingerprinting headers** — Server, X-Powered-By, X-Generator, X-Debug-Token, X-Runtime, and similar information-leakage candidates
- **Identifies deprecated headers** — X-XSS-Protection, Expect-CT, Public-Key-Pins — headers that do nothing useful and arguably make things worse
- **Scores each check independently** — Pass / Warn / Fail / Skip with an explanation; overall verdict is the highest severity across all checks
- **Enriches with IP metadata** — ASN, org, and network type (residential / datacenter / VPN / Tor) via the IP enrichment service

All three probes run concurrently. The entire inspection typically completes in under two seconds.

---

## The three probes

Most header scanners fire one request. spectra fires three, because one is not enough:

| Probe | What it finds |
|---|---|
| **HTTPS chain** | Full header analysis against the main HTTPS endpoint; captures the complete redirect chain |
| **HTTP port-80 upgrade** | Does port 80 redirect to HTTPS? On the same host? With an appropriate status code? |
| **CORS probe** | Sends `Origin: https://evil.example.com` and inspects what comes back — wildcard, reflection, or silence |

---

## Input syntax

```
https://hostname[/path][?query]
hostname    (auto-prefixed with https://)
```

| Example | What it does |
|---|---|
| `example.com` | Inspect `https://example.com` |
| `example.com/api/data` | Inspect a specific path |
| `https://example.com:8443` | Non-standard HTTPS port |

Explicit `http://` scheme is rejected — the HTTP port-80 upgrade probe fires automatically for any `https://` target. Internal IPs (RFC 1918, loopback, link-local) are blocked before any connection is made.

---

## API

```
POST /api/inspect    {"url": "https://example.com"}
GET  /api/inspect?url=https://example.com
```

Returns a structured JSON document with the full inspection result. See the [API docs](/docs) or [OpenAPI spec](/api-docs/openapi.json) for the full schema.

```sh
curl -s 'https://http.netray.info/api/inspect?url=example.com' | jq .quality
```

Additional endpoints:

| Endpoint | Description |
|---|---|
| `GET /health` | Liveness probe |
| `GET /ready` | Readiness probe |
| `GET /api/config` | Server version |
| `GET /api-docs/openapi.json` | OpenAPI 3.1 spec |
| `GET /docs` | Interactive API documentation |

### CI / Pipeline integration

Use in GitHub Actions to gate on security posture:

```yaml
# Fail the build if the overall verdict is not pass
- run: |
    curl -sf 'https://http.netray.info/api/inspect?url=$URL' \
      | jq -e '.quality.verdict == "pass"'

# Fail if port 80 does not redirect to HTTPS on the same host
- run: |
    curl -sf 'https://http.netray.info/api/inspect?url=$URL' \
      | jq -e '.http_upgrade.redirects_to_https and .http_upgrade.same_host'

# Check that no cookies are missing the Secure flag
- run: |
    curl -sf 'https://http.netray.info/api/inspect?url=$URL' \
      | jq -e '[.quality.checks[] | select(.name == "cookie_secure" and .status == "pass")] | length > 0'
```

---

## Building

Prerequisites: Rust toolchain, Node.js (for the frontend).

```sh
# Full production build (frontend + Rust binary)
make

# Run the built binary
make run

# Development (two terminals)
make frontend-dev   # Vite dev server on :5175, proxies /api/* to :3000
make dev            # cargo run with spectra.toml

# Tests and lints
make test           # Rust tests
make lint           # clippy + fmt check
make ci             # Full CI: lint + test + frontend build
```

The release binary embeds the compiled frontend. No separate static file hosting required.

---

## Configuration

Copy `spectra.example.toml` and adjust:

```toml
[server]
bind         = "127.0.0.1:3000"
metrics_bind = "127.0.0.1:9090"
# trusted_proxies = ["10.0.0.0/8"]

[inspect]
request_timeout_secs  = 10    # per-probe timeout
total_timeout_secs    = 30    # wall-clock cap across all three probes
max_redirects         = 10
user_agent            = "netray-spectra"

[limits]
per_ip_per_minute          = 10
per_ip_burst               = 5
per_target_per_minute      = 30
per_target_burst           = 10
max_concurrent_connections = 256

[enrichment]
ip_url     = "https://ip.netray.info"   # optional; omit to disable IP enrichment
timeout_ms = 500

[telemetry]
log_format   = "text"    # "text" | "json"
enabled      = false     # set true to enable OTLP export
# otlp_endpoint = "http://localhost:4318"
service_name = "spectra"
sample_rate  = 1.0
```

Configuration is loaded from `spectra.toml` by default. Override the path with `SPECTRA_CONFIG`. Environment variables take precedence over the file, using the `SPECTRA_` prefix with `__` as the section separator — e.g. `SPECTRA_SERVER__BIND=0.0.0.0:3000`.

---

## Quality checks

Each check produces a Pass / Warn / Fail / Skip verdict. The overall verdict is the highest severity across all checks.

**Security headers**
- HSTS — max-age parsed; Warn below 1 year (31 536 000 s); Fail if absent; preload flag noted
- CSP — enforced vs. report-only; detects `unsafe-inline`, `unsafe-eval`, missing `default-src`, wildcard schemes (`https:`, `*`), broad wildcards (`*.com`), and missing `object-src` restriction
- X-Frame-Options — DENY or SAMEORIGIN accepted; Fail if absent
- X-Content-Type-Options — must be `nosniff`; Fail if absent
- Referrer-Policy — Warn on unsafe values (`unsafe-url`, `no-referrer-when-downgrade`); Fail if absent
- Permissions-Policy — Warn if absent
- COOP / COEP / CORP — Warn if absent; required for cross-origin isolation

**CORS**
- Wildcard origin — Warn on `Access-Control-Allow-Origin: *`
- Origin reflection — Warn if the response mirrors the request `Origin` header
- Credentials — Fail if reflected origin and `Access-Control-Allow-Credentials: true` (credential theft vector)

**Cookies**
Per-cookie checks across all `Set-Cookie` response headers:
- `Secure` flag — Fail if absent (cookie can be sent over plain HTTP)
- `HttpOnly` flag — Warn if absent
- `SameSite` — Warn on `None` without `Secure`; Skip if not present

**Transport upgrade**
- HTTP redirect — Warn if port 80 does not respond; Fail if port 80 serves content without redirecting to HTTPS
- Same-host redirect — Warn if the redirect target is a different hostname

**Fingerprinting**
- Detects `Server`, `X-Powered-By`, `Via`, `X-AspNet-Version`, `X-AspNetMVC-Version`, `X-Generator`, `X-Debug-Token`, `X-Runtime`
- Warn if any exposing headers are present; includes a list of the offending header names

**Deprecated headers**
- `X-XSS-Protection` — deprecated and removed from all major browsers
- `Expect-CT` — obsolete since CT is mandatory
- `Public-Key-Pins` / `Public-Key-Pins-Report-Only` — deprecated, now a bricking risk

---

## Frontend features

- **Three-probe panel** — separate result tabs for the HTTPS chain, HTTP upgrade probe, and CORS probe
- **Quality verdict badge** — color-coded overall verdict (pass / warn / fail) with per-check breakdown and expandable explanations
- **Redirect chain viewer** — every hop shown with URL, status, and HTTP version
- **Cookie table** — per-cookie attribute breakdown with security flags highlighted
- **CDN badge** — detected CDN and cache status shown inline
- **IP enrichment row** — ASN, org, and network type surfaced in the overview bar
- **Copy to clipboard** — raw JSON or Markdown; individual header values have copy buttons
- **Keyboard shortcuts** — submit on Enter, clear on Escape

---

## Security

spectra makes outbound HTTP/HTTPS connections to user-specified targets. The security model is defense-in-depth:

1. **Input validation** — URL syntax check, scheme allowlist, no `javascript:` or `data:` URIs
2. **Target policy** — DNS-resolved IPs are checked against a blocklist (RFC 1918, loopback, link-local, CGNAT, multicast) before any connection is made. DNS rebinding is mitigated by resolving once and checking the result.
3. **Rate limiting** — GCRA per source IP (10/min, burst 5) and per target hostname (30/min, burst 10). Both 429 responses include `Retry-After`.
4. **Concurrency cap** — Tower `ConcurrencyLimitLayer` caps in-flight requests at 256.
5. **Body reads** — response bodies are not read beyond a small cap (1 KB by default). spectra analyzes headers only.

---

## Tech stack

**Backend**: Rust · axum · reqwest · tokio · tower-governor

**Frontend**: SolidJS · Vite · TypeScript (strict)

---

## License

MIT — see [LICENSE](LICENSE).
