# Software Design Document: `tlsight`

**Feature**: Web-based TLS certificate inspection and diagnostics service
**Status**: Phase 2 Partial (DNS cross-checks deferred)
**Date**: 2026-03-08

---

## 1. Motivation

Debugging TLS issues today means choosing between heavyweight scanners (SSL Labs — slow, third-party, no self-hosting), CLI tools (`openssl s_client` — arcane flags, no visualization), or online checkers that show a subset of the picture (certificate only, no DANE, no CAA cross-check).

Nobody offers the combination of **full chain-of-trust inspection**, **TLS configuration analysis**, **DNS-based certificate validation (DANE/TLSA + CAA)**, and **multi-port scanning** in a fast, self-hosted, single-binary tool with a web interface.

**Scope note**: tlsight covers direct TLS connections (port 443, 465, 993, 995, 8443, etc.). STARTTLS — upgrading a plaintext connection to TLS mid-stream on ports like 25 or 587 — is deferred to Phase 4 (section 14). This means the most common `openssl s_client` use case (SMTP STARTTLS on port 25/587) is not covered initially. tlsight replaces `openssl s_client` for direct TLS inspection; for STARTTLS debugging, users should continue using `openssl s_client -starttls smtp` until Phase 4.

### 1.1 The pdt.sh Ecosystem

tlsight is the third tool in a suite of self-hostable network diagnostic services under `*.pdt.sh`:

| Tool | Repo | Domain | Question it answers |
|------|------|--------|---------------------|
| **ifconfig-rs** | `ifconfig-rs/` | `ip.pdt.sh` | "Who is behind this IP?" — GeoIP, ASN, cloud/VPN/Tor/bot classification, threat intel |
| **prism** | `mhost-prism/` | `dns.pdt.sh` | "What does this domain resolve to?" — multi-server DNS fan-out, DNSSEC, delegation tracing |
| **tlsight** | `tlsight/` | `tls.pdt.sh` | "Is this domain's TLS setup correct?" — certificate chain, DANE, CAA, cipher analysis |

The three form a natural debugging workflow: DNS resolution -> IP identity -> TLS validation. Each tool is a standalone binary with its own repository and release cadence. Cross-links between them make each more useful, but every tool functions independently — an operator can deploy one, two, or all three.

**Why a separate project (not a prism feature):**

- **Different protocol surface.** prism sends DNS queries; tlsight makes outbound TCP connections with TLS handshakes. The security model, threat vectors (port scanning, connection abuse), and rate limiting strategy are fundamentally different.
- **Different response model.** prism streams results via SSE as per-record-type DNS queries complete over seconds. tlsight returns synchronous JSON — a TLS handshake completes in milliseconds.
- **Different dependency profile.** tlsight pulls in `rustls`, `x509-parser`, and TLS-specific crates. prism pulls in `hickory-proto` for raw DNS. Neither set belongs in the other binary.
- **Independent deployment.** An operator may want TLS inspection without DNS debugging, or vice versa. Bundling them forces both on every deployment.
- **Focused scope.** Each tool does one thing well. prism's query language, SSE streaming, and circuit breaker complexity don't apply to TLS inspection. Keeping them separate avoids feature creep and keeps each codebase navigable.

**Shared infrastructure** (axum, SolidJS, config patterns, security middleware) is intentionally duplicated across the three repos. Extraction into a shared crate (`pdt-common`) is deferred until a fourth service justifies it — see section 10.3.

**Target users:**
- DevOps engineers verifying certificate deployments and renewals
- SREs debugging TLS handshake failures during incidents
- Security engineers auditing TLS configurations and DANE compliance
- Developers verifying HTTPS setup without learning `openssl s_client` syntax

## 2. Core Concept

A single Rust binary that serves an embedded SPA and performs TLS handshake inspection against user-specified hostnames. The backend connects to the target, completes a TLS handshake, extracts the full certificate chain and negotiated parameters, cross-references DNS records (CAA, TLSA) for validation, and returns a structured result.

Unlike prism's streaming model, tlsight uses synchronous request/response — a TLS handshake completes in milliseconds, not seconds. No SSE is needed.

```
Browser                     tlsight binary
  |                              |
  |  GET /                       |
  |----------------------------->|  Serve embedded SPA (rust-embed)
  |<-----------------------------|
  |                              |
  |  GET /api/inspect?h=...      |
  |----------------------------->|  Parse hostname, resolve IPs,
  |                              |  connect + TLS handshake,
  |                              |  extract chain + params,
  |                              |  fetch CAA + TLSA via DNS,
  |                              |  cross-validate
  |  JSON response               |
  |<-----------------------------|  Return full inspection result
  |                              |
```

## 3. Placement: Standalone Repository

tlsight is a standalone repository (`tlsight/`), depending on `mhost` as a published crate from crates.io (no `app` feature) for DNS resolution (CAA/TLSA lookups, PTR for IP context). This follows the same pattern as prism and ifconfig-rs. See section 13.4 for the rationale behind using mhost over alternatives like hickory-resolver.

### 3.1 Directory Layout

```
tlsight/
  Cargo.toml                      # depends on mhost (crates.io), rustls, etc.
  build.rs                        # panics in release if frontend/dist missing
  Makefile                        # build/test/docker targets
  src/
    main.rs                       # entry point, axum server, graceful shutdown
    lib.rs                        # build_app(), middleware stack
    config.rs                     # config crate: TOML + env vars (TLSIGHT_ prefix)
    error.rs                      # thiserror AppError enum -> HTTP status + error codes
    state.rs                      # AppState (config, rate limiter, dns resolver)
    routes.rs                     # axum router, endpoint handlers
    tls/
      mod.rs                      # TLS inspection orchestration
      connect.rs                  # TCP connect + TLS handshake execution
      chain.rs                    # Certificate chain parsing and validation
      params.rs                   # TLS version, cipher suite, ALPN extraction
      ocsp.rs                     # OCSP stapling check
    dns/
      mod.rs                      # DNS lookups for CAA, TLSA, A/AAAA
      caa.rs                      # CAA record fetch + issuer cross-check
      tlsa.rs                     # TLSA record fetch + DANE validation
    validate/
      mod.rs                      # Cross-validation orchestration
      chain_trust.rs              # Chain completeness, expiry, signature algo checks
      dane.rs                     # TLSA record vs. presented certificate matching
      caa_compliance.rs           # CAA record vs. issuing CA matching
      ct.rs                       # Certificate Transparency log presence (optional)
    security/
      mod.rs                      # Security headers, CORS
      rate_limit.rs               # GCRA rate limiting (per-IP, per-target)
      ip_extract.rs               # Client IP from proxy headers
      target_policy.rs            # Target validation (no internal IPs, port restrictions)
    format.rs                     # Human-readable formatting for cert fields
    middleware.rs                 # Request ID, metrics, security headers
  frontend/                       # SolidJS + Vite (strict TypeScript)
    src/
      App.tsx                     # Main state, inspection trigger, theme
      components/
        HostInput.tsx             # Hostname input with port selector
        ChainView.tsx             # Certificate chain visualization
        CertDetail.tsx            # Individual certificate details
        TlsParams.tsx             # Negotiated TLS parameters
        ValidationSummary.tsx     # Pass/warn/fail validation results
        CrossLinks.tsx            # Links to dns.pdt.sh and ip.pdt.sh
      lib/
        types.ts                  # TypeScript interfaces matching Rust response
      styles/
        global.css                # Plain CSS with custom properties
    dist/                         # Build output, .gitignored, embedded via rust-embed
  docs/
    sdd.md                        # this document
```

## 4. Input Language

### 4.1 Design Principles

1. **Minimal**: The simplest input is just a hostname. Port defaults to 443.
2. **Explicit ports**: `hostname:port` syntax for non-standard ports.
3. **Multi-port**: `hostname:443,465,993` scans multiple ports in one request.
4. **No query language**: Unlike prism, there is no complex grammar — TLS inspection has fewer degrees of freedom. Input is just `hostname[:port[,port...]]`.

### 4.2 Syntax

```
input       ::= target (":" port_list)?
target      ::= hostname | ip_address
hostname    ::= <valid DNS hostname per RFC 952 / RFC 1123>
ip_address  ::= <IPv4 dotted-decimal> | "[" <IPv6> "]"
port_list   ::= port ("," port)*
port        ::= <1-65535>
```

### 4.3 Hostname Validation

Input is validated according to these rules:

| Rule | Behavior | Rationale |
|------|----------|-----------|
| ASCII labels | Must match `[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?` per label | RFC 952 / RFC 1123 |
| Internationalized names (IDN) | Accepted in Punycode form (`xn--...`) only | Avoids homograph ambiguity; browsers already convert to Punycode |
| Trailing dot | Stripped before use (`example.com.` -> `example.com`) | Common copy-paste artifact from DNS tools |
| Underscores | Rejected | Invalid in hostnames (RFC 952). TLSA's `_port._tcp.` prefix is constructed internally, never user-supplied |
| Wildcard (`*.example.com`) | Rejected | tlsight inspects live servers, not certificate patterns |
| Max length | 253 characters (after trailing dot removal) | DNS protocol limit |
| Empty / whitespace-only | Rejected with 400 | Invalid input |

### 4.4 Examples

```
example.com                          # defaults: port 443
example.com:8443                     # custom port
example.com:443,465,993              # multi-port scan
mail.example.com:25                  # STARTTLS (SMTP) — future
10.0.0.1                             # IP input, no SNI (warning)
[2001:db8::1]:8443                   # IPv6 with port
```

### 4.5 Defaults and Limits

| Input | Behavior | Rationale |
|-------|----------|-----------|
| `example.com` (bare hostname) | Inspect port 443 | HTTPS is the overwhelmingly common case |
| `example.com:443,465,993,995,8443` | Inspect all listed ports | Multi-port scan for common TLS services |
| No port specified | 443 | Sensible default |
| Port 0 or >65535 | Rejected with 400 | Invalid port |
| >5 ports | Rejected with 422 `TOO_MANY_PORTS` | Limit fan-out |
| IP address input | Accepted with warning | SNI not sent; results may differ from hostname-based access. Useful for services with IP SANs. See section 13.7 |

### 4.6 URL Encoding

The GET endpoint uses `?h=` with the input value. Colons and commas are valid in query strings per RFC 3986, but clients may percent-encode them. The parser accepts both literal and percent-encoded forms: `?h=example.com%3A443%2C465` is equivalent to `?h=example.com:443,465`. IDN hostnames in non-Punycode form are rejected (see 4.3), so Unicode encoding in URLs is not a concern.

### 4.7 STARTTLS (Future)

STARTTLS upgrades a plaintext connection to TLS mid-stream. This requires protocol-specific negotiation (SMTP `EHLO`/`STARTTLS`, IMAP `CAPABILITY`/`STARTTLS`). Deferred to a later phase — initial implementation covers direct TLS connections only. When added, a `+starttls` flag or protocol hint (`smtp://`, `imap://`) would indicate the upgrade protocol.

## 5. API Design

### 5.1 Inspect Endpoint

**GET — Hostname in query string** (frontend, shareable URLs, curl):

```
GET /api/inspect?h=example.com:443
Accept: application/json
```

**POST — Structured JSON** (programmatic clients):

```
POST /api/inspect
Content-Type: application/json

{
  "hostname": "example.com",
  "ports": [443],
  "timeout_secs": 5,
  "check_dane": true,
  "check_caa": true,
  "check_ct": false
}
```

POST with `?h=` is rejected with 400 (`AMBIGUOUS_INPUT`).

**POST field defaults and validation:**

| Field | Required | Default | Constraints |
|-------|----------|---------|-------------|
| `hostname` | Yes | — | Must pass hostname validation (section 4.3) |
| `ports` | No | `[443]` | 1-5 elements, each 1-65535. Empty array rejected with 400 |
| `timeout_secs` | No | `5` | 1-5 (clamped to hard cap). Applies per-handshake |
| `check_dane` | No | from config | Boolean |
| `check_caa` | No | from config | Boolean |
| `check_ct` | No | from config | Boolean |

If POST body includes a port in the hostname field (`"hostname": "example.com:443"`) *and* a `ports` array, the request is rejected with 400 (`AMBIGUOUS_INPUT`). If port is only in the hostname field, it is extracted and used (no `ports` array needed).

**Response**:

```json
{
  "request_id": "019...",
  "hostname": "example.com",
  "input_mode": "hostname",
  "summary": {
    "verdict": "pass",
    "checks": {
      "chain_trusted": "pass",
      "not_expired": "pass",
      "hostname_match": "pass",
      "caa_compliant": "pass",
      "dane_valid": "skip",
      "ocsp_stapled": "pass",
      "consistency": "pass"
    }
  },
  "ports": [
    {
      "port": 443,
      "dns": {
        "tlsa": {
          "records": [],
          "dnssec_signed": false,
          "dane_valid": null
        }
      },
      "ips": [
        {
          "ip": "93.184.216.34",
          "ip_version": "v4",
          "tls": {
            "version": "TLSv1.3",
            "cipher_suite": "TLS_AES_256_GCM_SHA384",
            "alpn": "h2",
            "sni": "example.com",
            "ocsp": {
              "stapled": true,
              "status": "good",
              "this_update": "2026-03-07T00:00:00Z",
              "next_update": "2026-03-14T00:00:00Z"
            },
            "handshake_ms": 42
          },
          "chain": [
            {
              "position": "leaf",
              "subject": "CN=example.com",
              "issuer": "CN=R11, O=Let's Encrypt, C=US",
              "sans": ["example.com", "www.example.com"],
              "serial": "04:AB:CD:...",
              "not_before": "2026-02-01T00:00:00Z",
              "not_after": "2026-05-02T00:00:00Z",
              "days_remaining": 55,
              "key_type": "ECDSA",
              "key_size": 256,
              "signature_algorithm": "ecdsa-with-SHA384",
              "fingerprint_sha256": "AB:CD:EF:...",
              "is_expired": false,
              "is_self_signed": false
            },
            {
              "position": "intermediate",
              "subject": "CN=R11, O=Let's Encrypt, C=US",
              "issuer": "CN=ISRG Root X1, O=Internet Security Research Group, C=US",
              "sans": [],
              "serial": "...",
              "not_before": "2024-03-13T00:00:00Z",
              "not_after": "2027-03-12T23:59:59Z",
              "days_remaining": 369,
              "key_type": "RSA",
              "key_size": 2048,
              "signature_algorithm": "sha256WithRSAEncryption",
              "fingerprint_sha256": "...",
              "is_expired": false,
              "is_self_signed": false
            }
          ],
          "validation": {
            "chain_trusted": true,
            "terminates_at_self_signed": false,
            "chain_order_correct": true,
            "leaf_covers_hostname": true,
            "any_expired": false,
            "any_not_yet_valid": false,
            "weakest_signature": "ecdsa-with-SHA384",
            "earliest_expiry": "2026-05-02T00:00:00Z",
            "earliest_expiry_days": 55
          }
        }
      ],
      "consistency": null
    }
  ],
  "dns": {
    "caa": {
      "records": ["0 issue \"letsencrypt.org\""],
      "issuer_allowed": true,
      "issuewild_present": false
    },
    "resolved_ips": ["93.184.216.34", "2606:2800:21f:cb07:6820:80da:af6b:8b2c"]
  },
  "warnings": [],
  "duration_ms": 187
}
```

**Response structure rationale:**

- **`ports[]`**: Top-level grouping by port. Each port contains its own `dns.tlsa` (TLSA is keyed by `_port._tcp.hostname`, so it differs per port) and an `ips[]` array of per-IP results.
- **`dns.caa`**: Remains top-level because CAA records are per-hostname, not per-port.
- **`summary`**: Top-level verdict aggregating all checks across all ports and IPs. Consumers wanting "is everything OK?" check one field. See section 5.6 for verdict semantics.
- **`consistency`**: Per-port (only present when multiple IPs are inspected for that port). See section 5.3.
- **`input_mode`**: `"hostname"` or `"ip"`. When `"ip"`, the `summary.checks.hostname_match` is `"skip"` (no SNI sent) and `dns` fields are null.

**Timing fields:**
- `tls.handshake_ms`: Wall-clock time for TCP connect + TLS handshake for a single IP.
- `duration_ms`: Total wall-clock time for the entire request, including DNS resolution, all handshakes (concurrent), and DNS cross-checks. Does not include rate-limit queue wait time.

### 5.2 Multi-port Response

When multiple ports are requested, the `ports` array contains one entry per port. Each port entry is independent — one port may succeed while another fails:

```json
{
  "ports": [
    {
      "port": 443,
      "dns": {"tlsa": {...}},
      "ips": [{"ip": "93.184.216.34", "tls": {...}, "chain": [...], "validation": {...}}],
      "consistency": null
    },
    {
      "port": 465,
      "error": {"code": "CONNECTION_REFUSED", "message": "Connection refused"},
      "ips": [],
      "consistency": null
    },
    {
      "port": 993,
      "dns": {"tlsa": {...}},
      "ips": [{"ip": "93.184.216.34", "tls": {...}, "chain": [...], "validation": {...}}],
      "consistency": null
    }
  ]
}
```

Per-port errors use the same `error` object format as top-level errors (section 5.5). The HTTP status is still 200 — partial results are not failures.

### 5.3 Multi-IP Inspection

When a hostname resolves to multiple IPs, tlsight connects to **each IP independently** and reports per-IP results within each port entry. This reveals load balancer certificate inconsistencies — a common misconfiguration.

```json
{
  "ports": [
    {
      "port": 443,
      "ips": [
        {"ip": "93.184.216.34", "ip_version": "v4", "tls": {...}, "chain": [...]},
        {"ip": "2606:2800:21f:...", "ip_version": "v6", "tls": {...}, "chain": [...]}
      ],
      "consistency": {
        "certificates_match": true,
        "tls_versions_match": true,
        "cipher_suites_match": false,
        "mismatches": [
          {"field": "cipher_suite", "values": {"93.184.216.34": "TLS_AES_256_GCM_SHA384", "2606:...": "TLS_CHACHA20_POLY1305_SHA256"}}
        ]
      }
    }
  ]
}
```

The `consistency` object is non-null only when multiple IPs are inspected for the same port.

**Partial IP failures**: If some IPs succeed and others fail, the HTTP status is still 200. Failed IPs appear in the `ips` array with an `error` field instead of `tls`/`chain`/`validation`:

```json
{
  "ip": "93.184.216.34",
  "ip_version": "v4",
  "error": {"code": "HANDSHAKE_FAILED", "message": "Connection timed out"}
}
```

The `consistency` comparison only considers successful IPs.

### 5.4 Metadata Endpoints

```
GET /api/health            # liveness probe (bypasses rate limiting)
GET /api/ready             # readiness probe
GET /api/meta              # site metadata (version, features, links to ecosystem tools)
GET /docs                  # Scalar API reference UI
GET /api-docs/openapi.json # OpenAPI 3.1 spec
```

**`/api/meta` response:**

```json
{
  "name": "tlsight",
  "version": "0.1.0",
  "features": {
    "dane": true,
    "caa": true,
    "ct": false,
    "multi_port": true,
    "ip_input": true
  },
  "ecosystem": {
    "dns_url": "https://dns.pdt.sh",
    "ip_url": "https://ip.pdt.sh"
  },
  "limits": {
    "max_ports": 5,
    "max_ips_per_hostname": 10,
    "handshake_timeout_secs": 5,
    "request_timeout_secs": 15
  }
}
```

`ecosystem` URLs are only present when configured — absent keys mean the feature is disabled. The frontend uses this to conditionally render cross-links. `limits` lets programmatic clients discover constraints without trial-and-error.

### 5.5 Error Responses

Consistent JSON format matching the pdt.sh ecosystem convention:

```json
{
  "error": {
    "code": "INVALID_HOSTNAME",
    "message": "Hostname exceeds 253 characters"
  }
}
```

| HTTP Status | Code | When |
|-------------|------|------|
| 400 | `INVALID_HOSTNAME` | Malformed, empty, or unparseable hostname (includes IDN in non-Punycode, underscores, wildcards) |
| 400 | `INVALID_PORT` | Port 0 or >65535 |
| 400 | `PARSE_ERROR` | Unparseable input |
| 400 | `AMBIGUOUS_INPUT` | POST with `?h=` query param, or POST with port in both hostname and ports array |
| 422 | `TOO_MANY_PORTS` | >5 ports requested |
| 403 | `BLOCKED_TARGET` | Hostname resolves to private/internal IP (or all resolved IPs are blocked). The client cannot fix this — it is a server-side policy refusal |
| 429 | `RATE_LIMITED` | Rate limit exceeded (includes `Retry-After` header) |
| 502 | `DNS_RESOLUTION_FAILED` | DNS lookup failed (NXDOMAIN, SERVFAIL, timeout). Includes the resolver error in `message` |
| 502 | `CONNECTION_FAILED` | TCP connect failed (timeout, refused, unreachable) |
| 502 | `HANDSHAKE_FAILED` | TLS handshake failed (protocol error, no common cipher) |
| 502 | `CERTIFICATE_ERROR` | Certificate could not be parsed |
| 504 | `REQUEST_TIMEOUT` | 15-second request timeout exceeded (multi-port/multi-IP). Retry with fewer ports |

### 5.6 Summary Verdicts

The top-level `summary` provides a single-pass "is everything OK?" answer. Each check has a tri-state value:

| Value | Meaning |
|-------|---------|
| `"pass"` | Check passed for all ports/IPs |
| `"warn"` | Non-critical issue (e.g., expiring soon, OCSP not stapled, cipher mismatch across IPs) |
| `"fail"` | Critical issue (expired cert, chain incomplete, hostname mismatch) |
| `"skip"` | Check not applicable (e.g., DANE skipped when no TLSA records, hostname_match skipped for IP input) |

The top-level `verdict` is the worst status across all checks: any `"fail"` -> `"fail"`, else any `"warn"` -> `"warn"`, else `"pass"`.

**Multi-IP roll-up**: Each summary check aggregates across all ports and all IPs. If port 443 resolves to 3 IPs and 2 have trusted chains but 1 does not, `chain_trusted` is `"fail"`. The summary does not identify *which* IP caused the failure — that detail is in `ports[].ips[].validation`, which is authoritative. The summary is a triage signal ("something is wrong"), not a diagnostic ("here's what's wrong where"). The frontend uses the summary for the at-a-glance badge display and links to the per-IP details for investigation.

### 5.7 OCSP Field Semantics

The `tls.ocsp` object is always present. When the server does not staple an OCSP response:

```json
{
  "ocsp": {
    "stapled": false,
    "status": null,
    "this_update": null,
    "next_update": null
  }
}
```

Possible `status` values when `stapled: true`:

| Value | Meaning |
|-------|---------|
| `"good"` | Certificate is valid per the OCSP responder |
| `"revoked"` | Certificate has been revoked |
| `"unknown"` | OCSP responder does not know this certificate |
| `"malformed"` | Stapled OCSP response could not be parsed |

A `"revoked"` status sets `summary.verdict` to `"fail"`.

## 6. Frontend

### 6.1 Technology Choice: SolidJS + Vite

Same stack as prism and ifconfig-rs for ecosystem consistency. The frontend is lighter than prism — no CodeMirror (simple text input suffices), no streaming.

Estimated frontend bundle: ~25-35KB gzipped (Solid ~7KB + application code ~18-28KB). The application code includes chain visualization, expandable panels, multi-port tabs, multi-IP comparison, keyboard shortcuts, theme toggle, history management, and responsive layout — more than a simple form, but still well under prism's bundle size.

### 6.2 Input

A standard text input with:
- **Placeholder**: `example.com` or `example.com:443,8443`
- **Port presets**: Quick buttons for common port combinations (HTTPS: 443, Email: 465,993,995, All common: 443,465,993,995,8443)
- **Submit**: Enter key or button
- **URL sync**: `?h=example.com:443` for shareability
- **History**: Recent hostnames in localStorage

No CodeMirror — the input is a single hostname with optional ports. A styled `<input>` is sufficient.

### 6.3 Results Display

```
+--------------------------------------------------------------+
|  [ example.com:443 ................... ]           [Inspect]  |
|  Presets: [HTTPS] [Email] [All common]                        |
+--------------------------------------------------------------+
|                                                               |
|  Validation Summary                                           |
|  +--------------------------------------------------------+  |
|  |  * Chain valid    * Not expired    * CAA OK             |  |
|  |  - No DANE/TLSA   * OCSP stapled  * Hostname match     |  |
|  +--------------------------------------------------------+  |
|                                                               |
|  Certificate Chain                                            |
|  +-------------+    +-------------+    +-------------+       |
|  |    Leaf      |--->| Intermediate|--->|    Root     |       |
|  | example.com  |    | R11 (LE)    |    | ISRG Root   |       |
|  | 55 days left |    | 369 days    |    | (trusted)   |       |
|  | ECDSA P-256  |    | RSA 2048    |    | RSA 4096    |       |
|  +-------------+    +-------------+    +-------------+       |
|                                                               |
|  > TLS Parameters                                             |
|    Version: TLSv1.3 | Cipher: TLS_AES_256_GCM_SHA384        |
|    ALPN: h2 | OCSP: stapled (good, expires Mar 14)           |
|    Handshake: 42ms                                            |
|                                                               |
|  > DNS Cross-Check                                            |
|    CAA: 0 issue "letsencrypt.org" -- matches issuer           |
|    TLSA: no records found                                     |
|    -> View full DNS at dns.pdt.sh                             |
|    -> View IP info at ip.pdt.sh                               |
|                                                               |
+--------------------------------------------------------------+
|  Inspected 1 IP on port 443 in 187ms                         |
+--------------------------------------------------------------+
```

Key elements:

- **Validation summary**: At-a-glance pass/warn/fail badges for each check category, driven by `summary.checks`
- **Chain visualization**: Horizontal chain diagram showing leaf -> intermediate(s) -> root, with key metadata per certificate (subject, days remaining, key type/size)
- **Certificate detail**: Expandable per-cert panels showing full fields (SANs, serial, fingerprint, validity dates, signature algorithm)
- **TLS parameters**: Negotiated version, cipher suite, ALPN, OCSP status
- **DNS cross-check**: CAA and TLSA results with cross-validation status
- **Ecosystem links**: Direct links to dns.pdt.sh and ip.pdt.sh for the inspected hostname/IPs
- **Multi-IP consistency**: When multiple IPs are inspected, highlight mismatches between them
- **Multi-port tabs**: When multiple ports are scanned, a tab bar selects which port's results to display

### 6.4 Visual Design

- **System theme by default** with manual toggle (same pattern as prism/ifconfig-rs)
- **Monospaced font** for certificate fields, fingerprints, serial numbers
- **Color coding**: Green = valid/pass, yellow = warning (expiring soon, weak cipher), red = fail/expired
- **Expiry timeline**: Color gradient based on days remaining — green >60 days, yellow 30-60, orange 14-30, red <14
- **Mobile**: Responsive card layout, chain visualization stacks vertically below 768px

### 6.5 Keyboard Shortcuts

| Key | Context | Action |
|-----|---------|--------|
| `/` | Global (not in input) | Focus hostname input |
| `Enter` | Input focused | Submit inspection |
| `j` / `k` | Results area focused | Navigate expandable sections |
| `Enter` | Results area focused | Expand/collapse focused section |
| `?` | Global (not in input) | Toggle keyboard shortcuts help |

Shortcut keys (`/`, `j`, `k`, `?`) are only active when the hostname input does not have focus. This prevents conflicts with typing.

## 7. Backend Architecture

### 7.1 Server Setup

Same axum pattern as prism and ifconfig-rs:

```rust
// main.rs (simplified)
#[tokio::main]
async fn main() {
    let config = Config::load(config_path.as_deref())
        .expect("failed to load configuration");

    let state = AppState::new(&config).await;

    let health = Router::new()
        .route("/api/health", get(routes::health));

    let api = Router::new()
        .route("/api/inspect", get(routes::get_inspect).post(routes::post_inspect))
        .route("/api/meta", get(routes::meta))
        .route("/api/ready", get(routes::ready))
        .layer(rate_limit_layer(&config));

    let app = Router::new()
        .merge(health)
        .merge(api)
        .layer(RequestBodyLimitLayer::new(4 * 1024))  // 4KB (simpler payloads than prism)
        .layer(cors_layer(&config))
        .layer(security_headers_layer())
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(ConcurrencyLimitLayer::new(config.limits.max_concurrent_connections))
        .fallback(static_handler)
        .with_state(state);

    // Metrics on separate port
    let metrics_app = Router::new().route("/metrics", get(metrics_handler));
    tokio::spawn(axum::serve(metrics_listener, metrics_app).into_future());

    // Main server with graceful shutdown
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}
```

### 7.2 Inspection Pipeline

```rust
// Simplified inspection flow
async fn inspect(
    hostname: &str,
    ports: &[u16],
    config: &Config,
    rate_budget: &RateBudget,
) -> Result<InspectResponse, AppError> {
    // 1. Resolve hostname to IPs via mhost (uses cached resolver).
    //    Fails with DNS_RESOLUTION_FAILED on NXDOMAIN, SERVFAIL, or timeout.
    let ips = resolve_ips(hostname).await
        .map_err(|e| AppError::DnsResolutionFailed(e))?;

    if ips.is_empty() {
        return Err(AppError::DnsResolutionFailed("no A/AAAA records".into()));
    }

    // 2. Filter blocked IPs (RFC 1918, loopback, etc.)
    let (allowed, blocked): (Vec<_>, Vec<_>) = ips.into_iter()
        .partition(|ip| target_policy::is_allowed(ip).is_ok());

    if allowed.is_empty() {
        return Err(AppError::BlockedTarget);
    }

    let mut warnings = Vec::new();
    if !blocked.is_empty() {
        warnings.push(format!("Blocked IPs not inspected: {}", blocked.iter().join(", ")));
    }

    // 3. Cap IPs to rate budget (cap-and-warn)
    let max_ips = rate_budget.affordable_ips(ports.len());
    let (inspected_ips, skipped_ips) = if allowed.len() > max_ips {
        // Prefer one IPv4 + one IPv6, then fill remaining budget
        let selected = select_representative_ips(&allowed, max_ips);
        let skipped: Vec<_> = allowed.iter()
            .filter(|ip| !selected.contains(ip))
            .collect();
        warnings.push(format!(
            "{} additional IPs not inspected (rate limit budget)",
            skipped.len()
        ));
        (selected, skipped)
    } else {
        (allowed, vec![])
    };

    // 4. All ports + CAA concurrently, bounded by shared semaphore.
    //
    // Ports are NOT iterated sequentially — all port work is spawned at once so
    // the semaphore distributes handshakes across ports fairly. Without this,
    // 5 ports x 10 IPs x 5s timeout = 25s sequential worst case, exceeding
    // the 15s request timeout.
    //
    // The semaphore bounds total concurrent handshakes across all ports in this
    // request. Each spawned task acquires its own permit internally — acquiring
    // before spawning would deadlock (JoinSet isn't polled until after push).
    let semaphore = Arc::new(Semaphore::new(config.limits.max_concurrent_handshakes));

    // 4a. Spawn per-port work
    let hostname: Arc<str> = hostname.into();  // Arc'd for 'static JoinSet tasks
    let mut port_join_set = JoinSet::new();
    for &port in ports {
        let sem = semaphore.clone();
        let ips = inspected_ips.clone();
        let hostname = hostname.clone();
        let hs_timeout = config.limits.handshake_timeout;
        port_join_set.spawn(async move {
            // TLSA lookup for this port (concurrent with handshakes)
            let tlsa_fut = dns::fetch_tlsa(hostname, port);

            // Handshake per IP
            let mut ip_join_set = JoinSet::new();
            for ip in &ips {
                let sem = sem.clone();
                let hostname = hostname.clone();
                ip_join_set.spawn(async move {
                    let _permit = sem.acquire_owned().await
                        .expect("semaphore closed unexpectedly");
                    let result = tls_handshake(&ip, port, &hostname, hs_timeout).await;
                    (*ip, result)
                });
            }

            let (tlsa, ip_results) = tokio::join!(
                tlsa_fut,
                collect_join_set(&mut ip_join_set),  // see note below
            );

            let consistency = if ip_results.len() > 1 {
                Some(compute_consistency(&ip_results))
            } else {
                None
            };

            PortResult { port, ips: ip_results, tlsa, consistency }
        });
    }

    // 4b. CAA lookup (hostname-level, concurrent with all port work)
    let caa_fut = dns::fetch_caa(hostname);

    let (port_results, caa) = tokio::join!(
        collect_join_set(&mut port_join_set),
        caa_fut,
    );

    // 5. Cross-validate and compute summary
    let summary = validate::summarize(&port_results, &caa);

    Ok(InspectResponse {
        hostname, ports: port_results, dns: DnsContext { caa, resolved_ips: inspected_ips },
        summary, warnings, skipped_ips,
    })

    // Note: collect_join_set drains a JoinSet via join_next() in a loop,
    // converting task panics into per-IP/per-port errors rather than
    // propagating them. See section 7.8.
}
```

### 7.3 TLS Handshake

The handshake uses `rustls` configured to **accept any certificate** (custom `ServerCertVerifier` that always returns `Ok`). This is intentional — tlsight is an inspection tool, not a TLS client. It must be able to inspect expired certificates, self-signed certificates, and misconfigured chains without rejecting them. The validation is performed after the handshake, in application code, where it can be reported to the user rather than silently failing.

```rust
// tls/connect.rs (simplified)
async fn tls_handshake(
    ip: &IpAddr,
    port: u16,
    sni: Option<&str>,  // None for IP-address input
    timeout: Duration,
) -> Result<HandshakeResult, TlsError> {
    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyCert))
        .with_no_client_auth();

    // For hostname input: SNI is set to the hostname.
    // For IP input: ServerName::IpAddress. Note that rustls's behavior with
    // IpAddress varies by version — some versions include the IP in the SNI
    // extension (discouraged by RFC 6066), others omit it. Since we use
    // AcceptAnyCert and don't rely on SNI-based verification, this is
    // acceptable either way. The server will present whatever certificate it
    // maps to the SNI (or its default cert if SNI is absent/unrecognized).
    let server_name = match sni {
        Some(name) => ServerName::try_from(name.to_owned())?,
        None => ServerName::IpAddress((*ip).into()),
    };

    let connector = TlsConnector::from(Arc::new(config));
    let start = Instant::now();
    let stream = tokio::time::timeout(timeout, async {
        let tcp = TcpStream::connect((*ip, port)).await?;
        tcp.set_nodelay(true)?;  // disable Nagle for handshake latency
        let tls = connector.connect(server_name, tcp).await?;
        Ok::<_, TlsError>(tls)
    }).await??;
    let handshake_ms = start.elapsed().as_millis() as u32;

    // Extract negotiated parameters.
    // Note: by this point the full handshake (including Finished messages) has
    // completed — rustls does not expose parameters mid-handshake. The
    // connection is closed after extraction, not "immediately after" the
    // ClientHello.
    let conn = stream.get_ref().1;
    let version = conn.protocol_version();
    let cipher = conn.negotiated_cipher_suite();
    let alpn = conn.alpn_protocol();
    let certs = conn.peer_certificates();
    let ocsp = conn.received_ocsp_staple();

    Ok(HandshakeResult { version, cipher, alpn, certs, ocsp, handshake_ms })
}
```

### 7.4 Certificate Chain Parsing

Certificates are parsed using `x509-parser` (or `rustls-pki-types` + `webpki`):

- **Subject, Issuer**: Extracted from X.500 distinguished names
- **SANs**: From the Subject Alternative Name extension
- **Validity**: `notBefore` / `notAfter` dates, computed `days_remaining`
- **Key info**: Algorithm (RSA/ECDSA/Ed25519), key size
- **Signature algorithm**: OID mapped to human-readable name
- **Fingerprint**: SHA-256 hash of DER-encoded certificate
- **Chain position**: Determined by the following algorithm:
  1. **Index 0** in the server-presented chain is always `"leaf"`.
  2. A certificate where `subject == issuer` is `"self_signed"`. If it is also at index 0, position is `"leaf_self_signed"`.
  3. The last certificate in the chain, if not self-signed, is `"intermediate"` (the root was likely omitted by the server, which is common and valid per RFC 8446 section 4.4.2).
  4. The last certificate in the chain, if self-signed, is `"root"`.
  5. All other certificates are `"intermediate"`.

This classification is based solely on the presented chain order, not a trust store lookup. The `validation.terminates_at_self_signed` field indicates whether the chain ends at a self-signed certificate. The separate `validation.chain_trusted` field performs actual trust store validation via `webpki` + `webpki-roots` (see section 13.9).

### 7.5 DNS Resolution and Caching

DNS lookups (A/AAAA, CAA, TLSA) use a shared `ResolverGroup` instance built once at startup from the configured resolver (see section 9).

**Caching strategy**: mhost's internal resolver handles DNS caching according to record TTLs. tlsight does not add an application-level cache on top — each request triggers a fresh mhost lookup, which may be served from mhost's TTL-based cache. This means:

- Repeated inspections of the same hostname within the TTL window are fast (cached DNS).
- There is no stale-data risk beyond standard DNS TTL semantics.
- No additional cache invalidation logic is needed.
- DNSSEC validation state is preserved per-lookup by mhost.

tlsight does **not** cache TLS handshake results. Each inspection performs a fresh connection — the point is to see the current live state.

### 7.6 Static File Serving

Identical pattern to prism and ifconfig-rs: `rust-embed` in release, filesystem reads in debug. Vite-hashed assets get `immutable` cache headers; `index.html` gets `no-cache`.

### 7.7 build.rs

The `build.rs` script serves two purposes:

1. **Release guard**: Panics if `frontend/dist/` is missing or empty in release builds. This prevents shipping a binary without the embedded frontend. In debug builds, the check is skipped — the server falls back to filesystem reads (section 7.6).
2. **Version embedding**: Sets `TLSIGHT_VERSION` from `Cargo.toml` and `TLSIGHT_GIT_SHA` from `git rev-parse --short HEAD` (if available) as compile-time environment variables for the `/api/meta` endpoint.

This matches the prism/ifconfig-rs pattern. The Makefile's `make` target builds frontend before backend, so the panic only fires when running `cargo build --release` directly without the frontend — which is a developer error.

### 7.8 Request Timeout and Panic Handling

**Request timeout placement**: The 15-second request timeout is enforced as a `tokio::time::timeout` wrapping the `inspect()` function call inside the route handler — **after** the request has been admitted by the `ConcurrencyLimitLayer`. This means:

- Queue wait time in the concurrency layer is **not** counted toward the 15s timeout. A request that waits 10s in the queue still gets a full 15s for inspection.
- The `ConcurrencyLimitLayer` itself does not have a timeout. Under sustained overload, queued requests will eventually be dropped by the client (browser timeout) or by an upstream reverse proxy timeout. This is acceptable — the concurrency layer protects the server from overcommitting, not the client from waiting.
- If the 15s timeout fires mid-inspection, completed port results are **discarded** and the request returns 504 `REQUEST_TIMEOUT`. Partial port results are not returned — this avoids the complexity of a partially-filled response where some ports have data and others are missing with no error. The client can retry with fewer ports if needed.

**JoinSet panic handling**: The pipeline uses `collect_join_set()`, a helper that drains a `JoinSet` via `join_next()` in a loop. If a spawned task panics (e.g., due to an unexpected invariant violation), the panic is caught by `JoinSet` and converted into a `JoinError`. The helper converts this into a per-IP or per-port error (`"code": "INTERNAL_ERROR"`) rather than propagating the panic to the request handler. This prevents one bad handshake from crashing the entire request. The `sem.acquire_owned().await.expect(...)` in the spawned task is safe because the semaphore is `Arc`-held by the parent scope and cannot be closed while tasks are running.

## 8. Security Architecture

tlsight makes outbound TCP connections to user-specified hostnames — a different threat model than prism (which makes DNS queries) but with similar mitigations.

### 8.1 Layer 1 — Target Restrictions (hardcoded)

| Restriction | Value | Rationale |
|-------------|-------|-----------|
| Blocked target IPs | RFC 1918, localhost, link-local, CGNAT, multicast, documentation ranges | Internal network probing prevention |
| Allowed ports | 1-65535, max 5 per request | Limit fan-out |
| Well-known port bias | None (all ports treated equally) | Legitimate TLS services run on any port |
| Max IPs per hostname | 10 | Prevent fan-out to many-IP CDN deployments |
| Per-handshake timeout | 5 seconds (hard cap) | Prevent hung connections |
| Per-request timeout | 15 seconds (hard cap) | Bound total request time for multi-port/multi-IP |
| Request body size | 4 KB | Simpler payloads than prism |
| Max concurrent connections | 256 (global) | Connection exhaustion prevention |
| Max concurrent handshakes per request | 10 | Prevent a single request from consuming all connections |
| IP address input | Allowed with warning | SNI not sent; `input_mode: "ip"` in response |
| Domain length | 253 characters | DNS protocol limit |

**Concurrency relationship**: With 256 global connections and 10 per-request, at most ~25 max-fan-out requests can execute concurrently. Ports within a request run concurrently (not sequentially), sharing the per-request semaphore. Additional requests queue behind the `ConcurrencyLimitLayer` — see section 7.8 for timeout interaction.

**IP validation timing**: The hostname is resolved to IPs via DNS, and each resolved IP is checked against the blocklist **before** initiating a TCP connection. If all resolved IPs are blocked, the request fails with 403 `BLOCKED_TARGET`. If some IPs are blocked and others are not, only the allowed IPs are inspected (with a warning listing the blocked IPs).

**DNS rebinding mitigation**: An attacker could point `evil.example.com` at `127.0.0.1` or an internal IP. The IP blocklist check after DNS resolution mitigates this — the connection is never initiated to a blocked IP. To minimize TOCTOU risk, the resolved IP is used directly in `TcpStream::connect()` (not re-resolved via the hostname). The sequence is: DNS resolve -> check IP -> connect to that exact IP. There is no second DNS lookup between check and connect.

### 8.2 Layer 2 — Rate Limiting

GCRA via `tower-governor`, **two tiers** — per-source-IP and per-target-hostname:

| Scope | Limit | Burst | Key |
|-------|-------|-------|-----|
| Per source IP | 30 inspections/minute | 10 | Real client IP |
| Per target hostname | 60 inspections/minute | 20 | Normalized hostname |

The per-target limit prevents a single target from being hammered by distributed clients. It is intentionally higher than per-source because legitimate use involves many users inspecting the same popular domain.

**Cost model with cap-and-warn**: Each request's cost is `ports * inspected_ips`. The cost is computed after DNS resolution. If the full cost exceeds the source IP's remaining budget, the request is **not rejected** — instead, the number of inspected IPs is reduced to fit the budget:

1. Select one IPv4 and one IPv6 address (if available) — these cover the most common consistency check. Selection within each address family is **first resolved** (i.e., the order returned by DNS), for reproducibility. This is best-effort representativeness, not a guarantee — if a hostname has 8 IPv6 and 2 IPv4 addresses with a budget of 3, you get 1v4 + 2v6, which may not be representative if the IPv4 backends serve different certs. The per-IP results and `skipped_ips` field make the selection transparent.
2. Fill remaining budget with additional IPs in DNS resolution order.
3. If even one handshake exceeds the budget, reject with 429.
4. Include a warning listing skipped IPs and a `skipped_ips` field in the response.

This degrades gracefully: users always get results (unless fully exhausted), and the response tells them exactly what was skipped.

**429 Retry-After**: The `Retry-After` header value is an integer (seconds), derived from `tower-governor`'s `wait_time_from()` on the rejected check. Both per-source and per-target limiters are checked; if both reject, `Retry-After` is the **maximum** of the two wait times (the client must wait for whichever bucket recovers last). The limiters are checked in order: per-source first, then per-target. If per-source rejects, per-target is not checked (no point deducting from the target budget when the source is exhausted).

**Atomicity**: The rate limiter uses `tower-governor`'s atomic check-and-decrement. The sequence is: (1) resolve DNS, (2) compute cost, (3) atomically check remaining budget and deduct cost in one operation. If two concurrent requests for the same hostname race, one will see the full budget and the other will see the reduced budget — the total deducted never exceeds the actual budget. The cap-and-warn reduction (step 3 in the list above) is applied before the atomic deduction, so the deducted amount matches the actual work performed.

### 8.3 Layer 3 — Client IP Extraction

Same priority chain as prism:

1. `CF-Connecting-IP` (behind Cloudflare)
2. `X-Real-IP` (behind nginx)
3. Rightmost untrusted IP in `X-Forwarded-For`
4. Direct peer address (fallback)

Proxy header extraction only activates when `trusted_proxies` is configured (empty = use peer address).

### 8.4 Layer 4 — HTTP Security Headers

Same headers as prism:

```
Content-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; frame-ancestors 'none'
X-Content-Type-Options: nosniff
X-Frame-Options: DENY
Referrer-Policy: strict-origin-when-cross-origin
Strict-Transport-Security: max-age=31536000; includeSubDomains
```

Relaxed `script-src` on `/docs` for Scalar CDN.

### 8.5 Outbound Connection Security

TLS inspection involves outbound TCP connections — additional considerations beyond prism:

- **No data exfiltration**: tlsight completes a full TLS handshake (ClientHello through Finished) and then closes the connection. It does not send any application data after the handshake completes. No HTTP requests, no payload — only the handshake itself.
- **No follow-up requests**: The service does not follow HTTP redirects, load page content, or issue any requests beyond the initial TLS handshake.
- **SNI privacy**: The SNI value is the user-provided hostname (sent in cleartext in TLS 1.2 ClientHello, encrypted in TLS 1.3 with ECH). This is inherent to TLS inspection.
- **Outbound firewall**: In production, the server should have outbound firewall rules allowing only TCP on common TLS ports, not arbitrary UDP or other protocols.

### 8.6 Logging and Monitoring

**Request IDs**: UUID v7 per request, returned in `X-Request-Id`. UUID v7 embeds a millisecond timestamp, which is useful for log correlation and ordering. The timestamp component leaks approximate server clock information — this is a negligible risk for a public-facing inspection tool.

**Logged per request:**
- Request ID, timestamp (UTC)
- Client IP (hashed after retention period)
- Inspected hostname, ports
- Per-IP handshake outcome (success/failure, TLS version)
- Duration

**Not logged:**
- Full certificate content (may contain organization PII)
- Private key material (never available — only public certs)

**Metrics** (Prometheus, separate port):
- `tlsight_inspections_total` (counter, labels: status, tls_version)
- `tlsight_inspection_duration_seconds` (histogram)
- `tlsight_handshake_duration_seconds` (histogram, labels: tls_version)
- `tlsight_rate_limit_hits_total` (counter, labels: tier [source_ip, target_hostname])
- `tlsight_active_inspections` (gauge)
- `tlsight_dns_lookup_duration_seconds` (histogram)
- `tlsight_certificates_expired_total` (counter)
- `tlsight_certificates_expiring_soon_total` (counter, labels: days_bucket)
- `tlsight_ips_skipped_total` (counter) — rate-limit cap-and-warn skips

## 9. Configuration

TOML file + `TLSIGHT_` prefixed env vars, `__` section separator. Same `config` crate pattern as prism and ifconfig-rs.

```toml
# tlsight.toml

[server]
bind = "127.0.0.1:8080"
metrics_bind = "127.0.0.1:9090"
trusted_proxies = []

[limits]
per_ip_per_minute = 30
per_ip_burst = 10
per_target_per_minute = 60           # per-target-hostname rate limit
per_target_burst = 20
max_concurrent_connections = 256
max_concurrent_handshakes = 10       # per-request concurrency cap
max_ports = 5
max_ips_per_hostname = 10
handshake_timeout_secs = 5           # hard cap per handshake
request_timeout_secs = 15            # hard cap per full request
max_domain_length = 253

[dns]
# mhost resolver config for CAA/TLSA/A/AAAA lookups
resolver = "cloudflare"              # predefined: "cloudflare", "google", "quad9", "system"
timeout_secs = 3

[validation]
expiry_warning_days = 30             # warn if cert expires within N days
expiry_critical_days = 14            # critical if cert expires within N days
check_dane = true                    # fetch TLSA records and validate
check_caa = true                     # fetch CAA records and cross-check
check_ct = false                     # CT log checking (requires external API, disabled by default)
# custom_ca_dir = "/etc/tlsight/ca.d/"  # directory of *.pem files for private CAs (optional)

[ecosystem]
# Links to sibling pdt.sh services. All optional — omit to disable cross-links.
# Operators running their own instances can point to different URLs.

# Base URL for DNS cross-links in the frontend (omit to disable DNS links)
dns_url = "https://dns.pdt.sh"

# Base URL for IP cross-links in the frontend (omit to disable IP links)
ip_url = "https://ip.pdt.sh"

# Backend-to-backend IP enrichment (ip_api_url) is deferred to Phase 4+.
# When added, it will support an internal address for server-side calls
# that bypass the public instance's rate limits.
```

**Ecosystem URL design**: Every `[ecosystem]` URL is optional. When omitted, the corresponding cross-links and enrichment features are disabled — tlsight functions fully standalone. This supports three deployment scenarios:

1. **Full pdt.sh suite**: All three services deployed together. Configure `dns_url` and `ip_url` to point at the shared instances.
2. **Partial deployment**: Only tlsight + ifconfig-rs deployed. Set `ip_url` to the local ifconfig instance, omit `dns_url`. DNS cross-links disappear from the UI.
3. **Standalone**: No sibling services. Omit `[ecosystem]` entirely. tlsight works as a self-contained TLS inspector.

Operators running private instances of ifconfig-rs (e.g., `https://ip.internal.example.com`) configure `ip_url` to point there instead of the public `ip.pdt.sh`.

**Hard caps** (cannot exceed via config): `handshake_timeout_secs=5`, `request_timeout_secs=15`, `max_ports=5`, `max_ips_per_hostname=10`. Values above caps are clamped with a startup warning. Zero values rejected.

**DNS resolver options**: The `resolver` field accepts predefined provider names (`"cloudflare"`, `"google"`, `"quad9"`, `"system"`) or a custom IP address (`"1.1.1.1"`, `"[2606:4700:4700::1111]"`). The `"system"` option uses `/etc/resolv.conf`. For DANE validation, the resolver must support DNSSEC — `"cloudflare"` and `"google"` do; `"system"` depends on the host configuration.

## 10. pdt.sh Ecosystem Integration

### 10.1 Cross-Links

All cross-links are driven by the `[ecosystem]` config section (see section 9). Links are only rendered when the corresponding URL is configured. The frontend receives the ecosystem URLs from the `/api/meta` endpoint, so no environment-specific values are hardcoded in the JavaScript bundle.

**Outbound links from tlsight:**

- **DNS** (`dns_url`): "View full DNS records" link for the inspected hostname. Pre-filled query: `{dns_url}/?q={hostname}+CAA+TLSA+A+AAAA`. Shown in the DNS Cross-Check section.
- **IP** (`ip_url`): "View IP info" link for each resolved IP. Link: `{ip_url}/?ip={ip}`. Shown next to each IP in multi-IP results.
- **IP enrichment** (deferred to Phase 4+): Backend-to-backend calls to ifconfig-rs for inline badges (cloud provider, network type) are explicitly deferred. The frontend cross-link to ip.pdt.sh already lets users click through for IP details. Adding a backend HTTP client, timeout/retry logic, and a new failure mode for a cosmetic feature is not justified until the core feature set is stable.

**Inbound links from sibling services:**

- **prism** (dns.pdt.sh): TLSA record results link to `{tls_url}/?h={hostname}` for certificate validation. Configured in prism's `[ecosystem]` section.
- **ifconfig-rs** (ip.pdt.sh): PTR hostname results link to `{tls_url}/?h={hostname}` for TLS inspection. Configured in ifconfig-rs's equivalent config.

Each service owns its own `[ecosystem]` config — there is no shared configuration file or service discovery. This keeps deployments independent: adding or removing a service only requires updating the config of the services that link to it.

### 10.2 Shared Patterns

All three services share:
- Axum 0.8 + tower-http middleware stack
- SolidJS + Vite + TypeScript strict + `rust-embed` frontend embedding
- `config` crate with service-specific prefix and `__` separator
- `thiserror` error enums -> HTTP status + JSON error codes
- `utoipa` for OpenAPI spec + Scalar docs UI
- Prometheus metrics on separate port
- UUID v7 request IDs
- 4-layer security (target restrictions, rate limiting, IP extraction, security headers)
- Same visual design language (CSS custom properties, dark/light theme, monospace for technical data)
- `build.rs` that panics in release if `frontend/dist/` is missing
- Makefile-driven builds (`make`, `make dev`, `make frontend-dev`, `make ci`)

### 10.3 Future: Shared Crate

If the ecosystem grows beyond three services, extracting shared infrastructure into a `pdt-common` crate may be warranted:
- Security middleware (IP extraction, rate limiting, security headers)
- Config loading pattern with prefix + `__` separator + hard cap clamping
- Static file serving with `rust-embed` + cache headers
- Request ID middleware
- Error response format
- Prometheus metrics setup + admin port

This is explicitly deferred — three similar implementations are tolerable. Extract on the fourth.

## 11. Dependencies

### 11.1 Rust

| Crate | Purpose | Notes |
|-------|---------|-------|
| `mhost` (crates.io, no `app`) | DNS resolution (CAA, TLSA, A/AAAA lookups) | Same as prism/ifconfig-rs. See section 13.4 |
| `rustls` | TLS handshake execution | Pure-Rust, no OpenSSL dependency |
| `tokio-rustls` | Async TLS connector | Bridges rustls + tokio |
| `webpki` | Trust store chain validation | Used for `chain_trusted` check (section 13.9) |
| `webpki-roots` | Mozilla root certificate bundle | Baseline trust store for chain validation |
| `x509-parser` | Certificate chain parsing | DER/PEM parsing, extension extraction |
| `x509-ocsp` | OCSP response parsing | Parses DER-encoded OCSP staple from `rustls` raw bytes |
| `axum` 0.8 | Web framework | Same as prism/ifconfig-rs |
| `tower-http` 0.6 | CORS, compression, tracing, headers | Same as prism/ifconfig-rs |
| `tower-governor` | GCRA rate limiting | Same as prism |
| `rust-embed` 8 | Frontend asset embedding | Same as prism/ifconfig-rs |
| `config` | TOML + env var config | Same as prism/ifconfig-rs |
| `thiserror` | Error enums | Same as prism/ifconfig-rs |
| `utoipa` | OpenAPI spec | Same as prism/ifconfig-rs |
| `tokio` | Async runtime | Same as prism/ifconfig-rs |
| `uuid` (v7) | Request IDs | Same as prism |
| `serde` + `serde_json` | Serialization | Same as prism/ifconfig-rs |
| `metrics` + `metrics-exporter-prometheus` | Prometheus metrics | Same as prism/ifconfig-rs |

**Notable: no `openssl` dependency.** `rustls` provides a pure-Rust TLS implementation, keeping the dependency tree clean and enabling fully static musl builds (same Docker pattern as prism).

### 11.2 Frontend (npm)

| Package | Purpose |
|---------|---------|
| `solid-js` | Reactive UI (~7KB) |
| `vite` | Build tool (dev dependency) |
| `vite-plugin-solid` | Solid JSX transform (dev dependency) |

No CodeMirror — simple text input suffices. Estimated bundle: ~25-35KB gzipped (see section 6.1).

## 12. Build Process

Same Makefile pattern as prism and ifconfig-rs:

```sh
# Development (two terminals)
make frontend-dev                   # Vite dev server :5173 (proxies /api/* to :8080)
make dev                            # cargo run (axum server :8080)

# Production build
make                                # frontend + backend (in order)
make frontend                       # cd frontend && npm ci && npm run build
make clean                          # remove target/ + frontend/dist/ + node_modules/

# CI pipeline
make ci                             # clippy -> test -> frontend build -> release build
```

**CI ordering**: `make ci` runs steps sequentially — `clippy` and `test` first (fast feedback, no frontend needed in debug), then `frontend` build, then `cargo build --release` (which embeds the frontend via `rust-embed`). This means clippy and tests pass without a built frontend, which is important since `build.rs` only panics in release mode.

### 12.1 Docker

Same multi-stage pattern:

```dockerfile
FROM node:22-alpine AS frontend
WORKDIR /app/frontend
COPY frontend/package*.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

FROM rust:1-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY build.rs ./
COPY --from=frontend /app/frontend/dist frontend/dist
RUN cargo build --release

FROM alpine:3
RUN apk add --no-cache ca-certificates
COPY --from=builder /app/target/release/tlsight /usr/local/bin/
EXPOSE 8080
ENTRYPOINT ["tlsight"]
```

Expected image size: ~15MB (lighter than prism — no `hickory-proto` dependency chain).

Note: The Dockerfile references `Cargo.lock`, which will exist once the first `cargo build` generates it. The Dockerfile is aspirational until the project has its first commit with source code.

## 13. Design Decisions

### 13.1 Request/Response over SSE

A TLS handshake completes in tens of milliseconds. Even a multi-port, multi-IP scan finishes in under a second. SSE streaming is unnecessary overhead — standard JSON responses are simpler, more cacheable, and work with standard HTTP clients without EventSource.

### 13.2 rustls over openssl

- **Pure Rust**: No system dependency on libssl, no version mismatch issues
- **Static linking**: Fully static musl builds for Docker
- **Security**: Memory-safe TLS implementation
- **Trade-off**: rustls does not support every legacy cipher suite (SSLv3, RC4, etc.). This means tlsight cannot inspect servers that only support legacy TLS. This is acceptable — such servers have larger problems than what an inspection tool can surface.

### 13.3 Accept-Any-Certificate Verifier

The custom `ServerCertVerifier` that accepts all certificates is the most security-sensitive design decision. Justification:
- tlsight's purpose is to **report** certificate status, not to **enforce** it
- Rejecting expired/self-signed/misconfigured certificates would prevent inspecting the very configurations users need to debug
- The verifier is used only for the inspection connection, never for internal HTTPS calls or DNS-over-HTTPS
- Application-level validation runs after the handshake and produces user-visible warnings/errors

### 13.4 DNS via mhost (not hickory-resolver)

tlsight uses `mhost` as a library for all DNS lookups (A/AAAA, CAA, TLSA). An obvious alternative is `hickory-resolver` (the standard Rust DNS resolver library), which would be a lighter dependency for simple record-type queries.

**Why mhost:**
- **Ecosystem consistency**: All three pdt.sh tools (prism, ifconfig-rs, tlsight) use mhost. A single DNS library across the suite means consistent resolver behavior, shared config patterns, and one set of DNS bugs to track.
- **DNSSEC awareness**: mhost provides DNSSEC validation state per-lookup, which is required for DANE (TLSA records without DNSSEC are meaningless). hickory-resolver supports DNSSEC too, but the integration is already proven in mhost.
- **Same team maintains mhost**: Breaking changes are coordinated, not discovered. The library API is stable because prism and ifconfig-rs depend on it.

**Trade-off acknowledged**: mhost is a DNS diagnostic tool, not a minimal resolver library. Using it as a library pulls in machinery that tlsight doesn't need (multi-server fan-out, circuit breakers, detailed per-query diagnostics). The `no app` feature flag excludes the CLI and presentation layer, but the resolver internals are heavier than `hickory-resolver` for the simple "query one server for a few record types" use case. This is accepted as the cost of ecosystem consistency. If mhost's library footprint becomes a problem (compile time, binary size), extracting a thin `mhost-resolver` crate is an option.

A shared `ResolverGroup` instance (built once at startup) is reused across all requests. Unlike prism, where each request may target different servers, tlsight always queries the same configured resolver.

### 13.5 Multi-IP by Default

Inspecting all resolved IPs (not just the first one) is a deliberate choice. Load balancer certificate mismatches are one of the most common TLS issues in production — different backend servers serving different certificates. This is invisible to tools that only connect to one IP.

### 13.6 No STARTTLS in v1

STARTTLS requires protocol-specific negotiation:
- SMTP: `EHLO` -> `STARTTLS` -> TLS handshake
- IMAP: `CAPABILITY` -> `STARTTLS` -> TLS handshake
- POP3: `CAPA` -> `STLS` -> TLS handshake
- XMPP: XML stream negotiation -> `<starttls/>` -> TLS handshake

Each protocol is a separate state machine. The value/complexity ratio doesn't justify v1 inclusion. Direct TLS (port 465 for SMTP submission, 993 for IMAPS, 995 for POP3S) covers most modern email setups.

**Honest limitation**: STARTTLS on port 25/587 is one of the most common reasons people reach for `openssl s_client`. Deferring it to Phase 4 means tlsight does not fully replace `openssl s_client` for email TLS debugging until then. This is acknowledged in section 1.

### 13.7 IP Address Input with Warning

IP address input is accepted but treated differently from hostname input:

- **SNI behavior**: rustls is given `ServerName::IpAddress`. Depending on the rustls version, this may include the IP literal in the SNI extension or omit SNI entirely (RFC 6066 says clients SHOULD NOT send IP literals as SNI). Either way, the server will present its default certificate or an IP-matched certificate. The response includes a warning explaining that results may differ from hostname-based access.
- **No DNS cross-checks**: CAA and TLSA lookups require a hostname. When input is an IP, `dns` fields are null and DNS-based checks are `"skip"` in the summary.
- **No multi-IP**: An IP input inspects exactly that IP — no DNS resolution step.
- **Response flag**: `input_mode: "ip"` signals to the frontend and API consumers that results are IP-mode.

This is useful for: inspecting internal services with IP-based SANs, verifying a specific backend behind a load balancer, and debugging default certificate configuration.

### 13.8 Nested Response Structure

The response groups results as `ports[] -> ips[]` rather than a flat `inspections[]` array. This is deliberate:

- **TLSA is per-port**: TLSA records are keyed by `_port._tcp.hostname`. A flat array would either duplicate TLSA data per entry or force it to the top level where it doesn't belong.
- **Consistency is per-port**: Comparing certificates across IPs only makes sense within the same port. A nested structure makes the grouping explicit.
- **Frontend mapping**: The UI naturally shows per-port tabs with per-IP comparison within each tab. The nested structure matches the rendering model.
- **CAA is per-hostname**: CAA remains top-level because it is independent of port. This is the correct placement — CAA restricts which CAs may issue for a hostname, regardless of which port the certificate is served on.

### 13.9 Trust Store for Chain Validation

The `validation` object includes two distinct chain checks:

- **`terminates_at_self_signed`**: Structural check — does the presented chain end with a self-signed certificate? This requires no trust store and no network access. A value of `false` is not necessarily a problem — servers commonly omit the root per RFC 8446 section 4.4.2.
- **`chain_trusted`**: Trust validation — does the presented chain build to a root in the trust store? This uses `webpki` with a `RootCertStore` built at startup from two sources: (1) `webpki-roots` (Mozilla's root CA bundle, compiled into the binary) and (2) all `*.pem` files from the optional `custom_ca_dir` config directory. This supports private CAs (e.g., Step-CA, internal PKI) — operators drop PEM files into the directory and tlsight trusts them alongside the Mozilla roots. **Important limitation**: `webpki` validates only the chain as presented by the server. If the server sends only a leaf cert (no intermediates) that is directly signed by a root CA, webpki can validate it. But if the server omits a required intermediate, `chain_trusted` will be `false` even if the chain *could* be completed by fetching the missing intermediate via AIA. tlsight does not perform AIA chasing — `chain_trusted: false` means "could not be verified from the presented chain alone."

The original design had only `chain_complete`, which was misleading — without a trust store, "complete" could only mean "ends at a self-signed cert," which is both narrower and different from what users expect. Splitting into two fields makes the semantics honest:

- A chain that omits the root (common, correct) shows `terminates_at_self_signed: false, chain_trusted: true` — no false alarm. webpki validates against roots without requiring them in the presented chain.
- A chain with an untrusted root shows `terminates_at_self_signed: true, chain_trusted: false` — clearly a problem.
- A chain with a missing intermediate shows `terminates_at_self_signed: false, chain_trusted: false` — the summary verdict is `"fail"`.

**Why webpki-roots + custom CA directory**: The compiled-in Mozilla bundle provides a consistent baseline that doesn't depend on the host's certificate store (which varies across distros and Docker base images). The optional `custom_ca_dir` extends this with operator-provided CAs for private PKI (e.g., Step-CA, corporate CAs). At startup, all `*.pem` files in the directory are loaded into the `RootCertStore` alongside the Mozilla roots. Missing or empty directory is fine (Mozilla-only mode). Malformed PEM files fail fast with a descriptive error. The trade-off for the compiled-in portion is that Mozilla root CA updates require a rebuild — acceptable for an inspection tool (the trust store changes slowly). The `ca-certificates` package in the Docker image is still installed for any future outbound HTTPS needs (e.g., CT log checking), not for chain validation.

### 13.10 Connection Tuning

Each TLS inspection creates fresh TCP + TLS connections (no pooling, no reuse). This is intentional — connection reuse would defeat the purpose of inspecting the current live state.

Socket options:
- **`TCP_NODELAY`**: Enabled. Disables Nagle's algorithm for lower handshake latency (the handshake is a small number of large-ish packets, not a stream of tiny writes).
- **`SO_REUSEADDR`**: Not explicitly set (OS default). On Linux/macOS, the kernel handles ephemeral port reuse via TIME_WAIT recycling.

**Ephemeral port exhaustion**: Under maximum load (256 concurrent connections, each lasting up to 5 seconds), the server uses at most 256 ephemeral ports simultaneously. The default ephemeral range on Linux (32768-60999) provides ~28K ports with a 60-second TIME_WAIT, supporting ~470 connections/second sustained. This exceeds tlsight's maximum throughput by a wide margin. No special tuning is needed.

## 14. Phased Delivery

### Phase 1 — MVP ✓ Complete

- [x] axum server with embedded SPA
- [x] `GET /api/inspect?h=` with JSON response
- [x] Hostname and IP-address parsing with port support
- [x] Single-IP TLS handshake (connect to first resolved IP)
- [x] Certificate chain extraction and parsing
- [x] TLS parameter extraction (version, cipher, ALPN)
- [x] Basic chain validation (expiry, not-yet-valid, trust via webpki, hostname match)
- [x] Top-level summary verdict
- [x] Rate limiting (per-source-IP and per-target-hostname GCRA)
- [x] Target IP validation (no internal IPs)
- [x] Security headers, CORS
- [x] Config validation at startup
- [x] Frontend: hostname input, chain visualization, cert details, TLS params
- [x] Dark/light theme
- [x] URL shareability via `?h=`
- [x] Health/ready/meta endpoints
- [x] Prometheus metrics on separate port
- [x] Scalar docs UI + OpenAPI spec

### Phase 2 — Multi-IP + DNS Cross-Checks (partial)

- [x] Multi-IP consistency comparison (leaf cert, TLS version, cipher suite)
- [x] Cap-and-warn rate limiting for multi-IP fan-out (select_representative_ips)
- [x] Frontend: multi-IP rendering per port, port tabs, consistency badges
- [x] Ecosystem cross-links (dns.pdt.sh, ip.pdt.sh) via `/api/meta`
- [ ] CAA record fetch and issuer cross-check (deferred: requires mhost integration)
- [ ] TLSA record fetch and DANE validation (deferred: requires mhost integration)

### Phase 3 — Polish

- Multi-port scanning (up to 5 ports per request)
- Port presets in frontend (HTTPS, Email, All common)
- POST endpoint for programmatic clients
- Keyboard shortcuts
- Query history (localStorage)
- Mobile-responsive layout
- Certificate Transparency log checking (optional, requires external API)
- Export results (JSON download, copy as markdown)

### Phase 4 — Advanced

- STARTTLS support (SMTP, IMAP, POP3)
- IP enrichment via backend-to-backend calls to ifconfig-rs (`ip_api_url` config)
- Certificate expiry monitoring / scheduling (webhook on approaching expiry)
- TLS version/cipher trend tracking (compare scans over time)
- Batch inspection (`POST /api/inspect/batch` with multiple hostnames)
- Certificate diff between two hostnames

### Phase 5 — Future (not committed)

- ECH (Encrypted Client Hello) detection and testing
- Client certificate authentication testing
- mTLS inspection
- Certificate chain download (PEM/DER)
- Integration with CT monitor APIs (crt.sh, Certstream)

## 15. Testing Strategy

### 15.1 Rust

- **Input parsing**: Unit tests for hostname:port parsing, edge cases (IPv6 brackets, port ranges, empty input, trailing dots, Punycode, underscores, wildcards, IP addresses)
- **Target policy**: Unit tests for IP blocklist (RFC 1918, loopback, link-local, CGNAT)
- **Chain validation**: Unit tests with canned certificate chains (valid, expired, self-signed, missing intermediate, wrong hostname, weak signature)
- **Chain position**: Unit tests for position classification algorithm (section 7.4)
- **DANE matching**: Unit tests for TLSA certificate usage types (0-3), selector types (0-1), matching types (0-2)
- **CAA compliance**: Unit tests for issuer matching against CAA records
- **Summary computation**: Unit tests verifying verdict roll-up from individual checks
- **Rate limiting**: Unit tests for cap-and-warn IP selection (prefers one v4 + one v6)
- **Integration tests**: `axum::test` with mocked TLS connections (no real network). Custom `TlsConnector` that returns canned handshake results.
- **Config validation**: Startup validation tests (clamped values, rejected zeros)

### 15.2 Frontend

- **Component tests**: vitest for chain rendering, validation summary, cross-link generation
- **No E2E in Phase 1**: API integration tests cover the surface; frontend is simple enough for component tests

## 16. Risks and Open Questions

1. **rustls cipher coverage.** rustls does not support legacy cipher suites (RC4, 3DES, export ciphers). Servers that only offer these will fail the handshake. This is acceptable — we document the limitation and users can fall back to `openssl s_client` for truly legacy servers.

2. **OCSP responder availability.** OCSP stapling is checked from the TLS handshake (no outbound OCSP query). If the server doesn't staple, we report "not stapled" — we do not make outbound OCSP queries to the CA's responder (that would add latency, privacy concerns, and a new outbound dependency).

3. **CT log checking.** Checking Certificate Transparency logs requires querying external APIs (crt.sh, Google CT). This adds latency and an external dependency. Disabled by default; when enabled, failures are non-fatal warnings.

4. **DANE/DNSSEC dependency.** DANE validation requires DNSSEC-signed TLSA records. If the zone is not signed, DANE validation is skipped (not failed). The response indicates "TLSA records found but zone not DNSSEC-signed — DANE validation not applicable."

5. **Frontend complexity.** The chain visualization (horizontal chain diagram with connectors) is the most complex frontend component. If it proves too brittle on mobile, a fallback to a simple vertical list is acceptable.

6. **mhost API stability.** mhost is pulled from crates.io with a semver-compatible version constraint in `Cargo.toml`. Breaking changes in mhost would require a coordinated update across tlsight, prism, and ifconfig-rs. This is acceptable given that all three repos are maintained by the same team.

7. **webpki-roots freshness.** The Mozilla root CA bundle is compiled into the binary via `webpki-roots`. Root CA additions/removals require a rebuild with an updated crate version. For an inspection tool (not a security-enforcing client), a slightly stale trust store is low-risk — the `chain_trusted` check may show `false` for a very new CA, which is a conservative failure mode. Operators on a strict update cadence can pin `webpki-roots` and rebuild periodically. For private CAs, the `custom_ca_dir` config avoids this issue entirely — new CA PEM files take effect on restart without a rebuild.

8. **rustls ServerName::IpAddress SNI behavior.** The exact SNI wire behavior for IP address input depends on the rustls version (see section 13.7). This is documented as a known variance, not a bug. Since we use AcceptAnyCert and don't rely on server-side SNI routing for correctness, both behaviors (SNI present with IP, SNI absent) produce valid inspection results.
