# tlsight

**TLS certificate inspection and diagnostics — in one view.**

tlsight is a web-based tool that performs a full TLS handshake against a hostname, extracts and validates the certificate chain, cross-checks DNS records, and surfaces everything in a structured, readable interface. No dependencies, no plugins — just a URL and a result.

Live at [tls.netray.info](https://tls.netray.info) · Part of the [netray.info](https://netray.info) toolchain alongside [dns.netray.info](https://dns.netray.info) and [ip.netray.info](https://ip.netray.info).

---

## What it does

Given a hostname (with optional ports), tlsight:

- **Resolves all IPs** — A and AAAA records, with DNSSEC awareness
- **Connects to each IP** — concurrent TLS handshakes, one per IP per port
- **STARTTLS** — auto-negotiates STARTTLS upgrade on SMTP ports 25 and 587 before the TLS handshake
- **Extracts the full certificate chain** — leaf, intermediates, root; subject, SANs, key type, expiry, fingerprint
- **Certificate policy classification** — EV / OV / DV detection via Certificate Policies extension and subject O field
- **AIA URL extraction** — OCSP responder URL and CA Issuers URL from each certificate's Authority Information Access extension
- **Certificate lifetime checks** — warns at 398 days, fails at 825 days (CA/B Forum limits)
- **Validates chain trust** — against the Mozilla root bundle (plus optional custom CA directory)
- **Checks OCSP stapling** — parses the stapled response if present, shows staple age and validity window
- **Extracts Certificate Transparency SCTs** — from the TLS extension
- **Cross-checks DNS** — CAA records (is the issuing CA authorized?) and TLSA/DANE records
- **ECH detection** — queries the HTTPS DNS record for an Encrypted Client Hello advertisement
- **Compares across IPs** — detects cert mismatches, TLS version, cipher suite, and ALPN inconsistencies between servers
- **Key exchange group** — named curve (X25519, P-256, P-384, …) shown in TLS parameters
- **Runs health checks** — per-port checks (certificate, protocol, configuration) plus hostname-scoped checks (HSTS, HTTPS redirect)
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
| `mail.example.com:25` | STARTTLS/SMTP — auto-negotiated before handshake |
| `192.0.2.1` | Inspect an IP directly (skips hostname validation checks) |

Maximum 7 ports per request. Ports must be in the range 1–65535. Internal IPs (RFC 1918, loopback, link-local, CGNAT, multicast) are blocked.

---

## API

```
GET /api/inspect?h=hostname[:port[,port...]]
```

Returns a structured JSON document with the full inspection result. See the [API docs](/docs) or [OpenAPI spec](/api-docs/openapi.json) for the full schema.

```sh
curl 'https://tls.netray.info/api/inspect?h=example.com'
```

Additional endpoints:

| Endpoint | Description |
|---|---|
| `GET /api/health` | Liveness probe |
| `GET /api/ready` | Readiness probe |
| `GET /api/meta` | Server capabilities and configured limits |
| `GET /api-docs/openapi.json` | OpenAPI 3.1 spec |
| `GET /docs` | Interactive API documentation |

### CI / Pipeline Integration

Use in GitHub Actions or any CI system to validate TLS health:

```yaml
# Check TLS health passes
- run: |
    curl -sf 'https://tls.netray.info/api/inspect?h=$DOMAIN' \
      | jq -e '.quality.verdict == "Pass"'

# Check certificate expiry (fail if < 30 days)
- run: |
    curl -sf 'https://tls.netray.info/api/inspect?h=$DOMAIN' \
      | jq -e '[.quality.checks[] | select(.name == "expiry_window" and .status == "pass")] | length > 0'
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
make frontend-dev   # Vite dev server on :5174, proxies /api/* to :8081
make dev            # cargo run with tlsight.dev.toml

# Tests
make test           # Rust + frontend
make ci             # Full CI: lint + test + frontend build

# CA/CAA data (contributors only — commit the result)
make data           # re-fetch SSLMate + CCADB CA lists and regenerate data/caa_domains.tsv
```

The release binary embeds the compiled frontend. No separate static file hosting required.

### CA data

`data/caa_domains.tsv` maps CAA `issue` domain values (e.g. `pki.goog`) to CA display names and is committed to the repository. `build.rs` embeds it as a sorted lookup table at compile time. Run `make data` to refresh it when CAs are added or renamed, then commit the updated TSV.

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
dns_base_url = "https://dns.netray.info"   # cross-links to DNS tool
ip_base_url  = "https://ip.netray.info"    # cross-links to IP tool
# ip_api_url = "https://ip.netray.info"    # enables IP enrichment (geo, ASN, rDNS badges)

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

All `*.pem` files in the directory are loaded at startup and added to the trust store alongside the Mozilla root bundle. The directory is re-scanned on `SIGHUP` — no restart required. The count of loaded custom CAs is exposed via the `/api/meta` endpoint (`custom_ca_count` field).

---

## Health checks

Each port inspection runs a set of health checks, producing a Pass / Warn / Fail / Skip verdict per check:

**Certificate**
- Chain trusted — verifies against Mozilla root bundle (+ custom CAs)
- Not expired — any cert in the chain expired or not-yet-valid
- Hostname match — leaf SAN covers the queried hostname
- Chain complete — correct chain order; leaf → intermediates → root
- Strong signature — no SHA-1 or MD5 in the chain
- Key strength — RSA ≥ 2048 bits, ECDSA ≥ P-256
- Expiry window — Warn ≤ 30 days, Fail ≤ 7 days
- Certificate lifetime — Warn > 398 days, Fail > 825 days (CA/B Forum limits)

**Protocol**
- TLS version — Warn on TLS 1.2, Fail on earlier
- Forward secrecy — ECDHE / DHE key exchange required
- AEAD cipher — GCM, ChaCha20-Poly1305, or CCM required
- OCSP stapled — Warn if absent
- CT logged — 2+ SCTs recommended (skipped when CT checking is disabled)

**Configuration**
- DANE valid — TLSA record match (skipped without DNSSEC)
- CAA compliant — issuing CA authorized by CAA records (matched via a compiled-in table of ~155 CA→domain mappings sourced from SSLMate and CCADB; unknown CAA domains → Fail)
- IP consistency — all IPs serve matching cert, TLS version, cipher, and ALPN
- ALPN consistency — ALPN protocol identical across all IPs
- ECH advertised — Warn if absent on port 443

**Hostname-scoped (once per inspection)**
- HSTS — `Strict-Transport-Security` header present with ≥ 180-day max-age
- HTTPS redirect — plain HTTP redirects to HTTPS

---

## Frontend features

- **Certificate validity timeline bar** — color-coded progress bar per cert (green > 30 days, amber 1–30 days, red expired)
- **OCSP staple freshness badge** — shows staple age and validity window alongside the staple status
- **Copy-to-clipboard** — individual certificate field values (subject, serial, fingerprints, …) have copy buttons
- **Cert change detection** — a banner is shown when the leaf certificate fingerprint differs from the last visit (stored in `localStorage`)
- **Port tab verdict badges** — colored dot per port tab (green / amber / red) derived from the port's health check roll-up

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
