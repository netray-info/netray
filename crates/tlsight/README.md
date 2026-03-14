# tlsight

**TLS certificate inspection and diagnostics — in one view.**

tlsight is a web-based tool that performs a full TLS handshake against a hostname, extracts and validates the certificate chain, cross-checks DNS records, and surfaces everything in a structured, readable interface. No dependencies, no plugins — just a URL and a result.

Live at [tls.pdt.sh](https://tls.pdt.sh) · Part of the [netray.info](https://netray.info) toolchain alongside [dns.pdt.sh](https://dns.pdt.sh) and [ip.netray.info](https://ip.netray.info).

---

## What it does

Given a hostname (with optional ports), tlsight:

- **Resolves all IPs** — A and AAAA records, with DNSSEC awareness
- **Connects to each IP** — concurrent TLS handshakes, one per IP per port
- **Extracts the full certificate chain** — leaf, intermediates, root; subject, SANs, key type, expiry, fingerprint
- **Validates chain trust** — against the Mozilla root bundle (plus optional custom CA directory)
- **Checks OCSP stapling** — parses the stapled response if present
- **Extracts Certificate Transparency SCTs** — from the TLS extension
- **Cross-checks DNS** — CAA records (is the issuing CA authorized?) and TLSA/DANE records
- **Compares across IPs** — detects cert mismatches, TLS version or cipher suite inconsistencies between servers
- **Runs health checks** — 15 checks per port (certificate, protocol, configuration) plus 2 hostname-scoped checks (HSTS, HTTPS redirect)
- **Checks HSTS and HTTPS redirect** — makes a live HEAD request to verify security headers

All of this happens in a single request, typically in under two seconds.

---

## Input syntax

```
hostname[:port[,port...]]
```

| Example | What it does |
|---|---|
| `example.com` | Inspect port 443 |
| `example.com:8443` | Inspect a non-standard TLS port |
| `example.com:443,465,993` | Inspect HTTPS, SMTPS, and IMAPS side by side |
| `mail.example.com:25` | Inspect SMTP (plain TLS, not STARTTLS) |
| `192.0.2.1` | Inspect an IP directly (skips hostname validation checks) |

Maximum 7 ports per request. Ports must be in the range 1–65535. Internal IPs (RFC 1918, loopback, link-local, CGNAT, multicast) are blocked.

---

## API

```
GET /api/inspect?h=hostname[:port[,port...]]
```

Returns a structured JSON document with the full inspection result. See the [API docs](/docs) or [OpenAPI spec](/api-docs/openapi.json) for the full schema.

```sh
curl 'https://tls.pdt.sh/api/inspect?h=example.com'
```

Additional endpoints:

| Endpoint | Description |
|---|---|
| `GET /api/health` | Liveness probe |
| `GET /api/ready` | Readiness probe |
| `GET /api/meta` | Server capabilities and configured limits |
| `GET /api-docs/openapi.json` | OpenAPI 3.1 spec |
| `GET /docs` | Interactive API documentation |

---

## Building

Prerequisites: Rust toolchain, Node.js (for the frontend).

```sh
# Full production build (frontend + Rust binary)
make

# Run the built binary
make run

# Development (two terminals)
make frontend-dev   # Vite dev server on :5174, proxies /api/* to :8081
make dev            # cargo run with tlsight.dev.toml

# Tests
make test           # Rust + frontend
make ci             # Full CI: lint + test + frontend build
```

The release binary embeds the compiled frontend. No separate static file hosting required.

---

## Configuration

Copy `tlsight.example.toml` and adjust:

```toml
[server]
bind = "0.0.0.0:8080"
metrics_bind = "127.0.0.1:9090"

[limits]
per_ip_per_minute = 30
per_ip_burst = 10
max_ports = 5
max_ips_per_hostname = 10
handshake_timeout_secs = 5
request_timeout_secs = 15

[dns]
resolver = "cloudflare"   # "cloudflare" | "google" | "system"

[validation]
expiry_warning_days = 30
check_dane = true
check_caa = true
check_ct = false
# custom_ca_dir = "/etc/tlsight/ca.d/"   # load private CAs from *.pem files

[ecosystem]
dns_base_url = "https://dns.pdt.sh"   # cross-links to DNS tool
ip_base_url  = "https://ip.pdt.sh"    # cross-links to IP tool
# ip_api_url = "https://ip.pdt.sh"    # enables IP enrichment (geo, ASN, rDNS badges)

[quality]
# enabled = true                    # health checks always-on (default: true)
# http_check_timeout_secs = 5       # timeout for HSTS/redirect checks (hard cap: 5s)
# skip_http_checks = false          # set true if outbound HTTP is blocked
```

Configuration is loaded from a TOML file (default: `tlsight.toml`, override with `TLSIGHT_CONFIG`). Environment variables take precedence over the file, using the `TLSIGHT_` prefix with `__` as the section separator — e.g. `TLSIGHT_SERVER__BIND=0.0.0.0:8080`.

Hardcoded safety caps (handshake timeout 5s, request timeout 15s, max 7 ports, max 10 IPs) cannot be exceeded by configuration.

### Custom CA directory

For inspecting internal infrastructure with private CAs:

```toml
[validation]
custom_ca_dir = "/etc/tlsight/ca.d/"
```

All `*.pem` files in the directory are loaded at startup and added to the trust store alongside the Mozilla root bundle.

---

## Security

tlsight makes outbound TCP+TLS connections to user-specified targets. The security model is defense-in-depth:

1. **Input validation** — hostname syntax, port range, character whitelist
2. **Target policy** — DNS-resolved IPs are checked against a blocklist (RFC 1918, loopback, link-local, CGNAT, multicast) before connecting. DNS rebinding is mitigated by resolving once and checking the result.
3. **Rate limiting** — GCRA per source IP and per target hostname. Multi-IP fan-out uses cap-and-warn instead of rejecting.
4. **Transport** — Connections are TLS-only (using rustls, pure Rust). No application data is sent after the handshake. The `AcceptAnyCert` verifier is used exclusively for inspection connections, never for internal HTTPS.

---

## Tech stack

**Backend**: Rust · axum · rustls · x509-parser · mhost · tower-governor

**Frontend**: SolidJS · Vite · TypeScript (strict)

---

## License

MIT — see [LICENSE](LICENSE).
