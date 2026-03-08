# TLS Quality Assessment — Software Design Document

**Status**: Implemented (Phase Q1 + Q2 complete)
**Author**: Lukas Pustina
**Parent SDD**: `docs/done/sdd-2026-03-08.md` (tlsight core)

---

## 1. Motivation

tlsight currently reports raw inspection data — certificate chains, TLS parameters, validation checks — but leaves interpretation to the user. Users must understand what "TLS 1.2 with TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256" means for their security posture.

### 1.1 Why Not a Letter Grade (Yet)

SSL Labs popularized letter grades (A+ through F) for TLS configuration. These grades depend heavily on **active probing** — enumerating all supported protocol versions and cipher suites by sending dozens of ClientHello variants. tlsight uses rustls, which only supports TLS 1.2 and 1.3 with strong cipher suites. This means:

- We cannot probe for TLS 1.0/1.1/SSLv3 support (rustls doesn't implement them).
- We cannot enumerate weak ciphers (RC4, 3DES, export, NULL) — rustls doesn't offer them.
- The negotiated cipher suite is always strong (AEAD, forward secrecy, 128+ bit keys).

A scoring engine built on this data would give every reachable server an A. That's not useful — it's misleading. It obscures real problems that we *can* detect.

### 1.2 What We Build Instead

A **TLS health check** — a structured checklist of pass/warn/fail findings derived from existing inspection data. Each check is a concrete, actionable finding ("SHA-1 signature in chain", "certificate expires in 12 days", "no HSTS header"). No composite scores, no letter grades, no weights.

This approach:

- Is immediately useful with zero dead code.
- Honestly represents what we can observe.
- Follows the Hardenize model (status-based, no grade) rather than the SSL Labs model.
- Provides the foundation for a future letter-grade system when raw socket probing (§8.1) makes scoring discriminating.

### 1.3 Scope Boundaries

**In scope**:
- Health checks derived from existing inspection data (certificate, chain, TLS params)
- HSTS presence and configuration (new HTTP check)
- HTTP-to-HTTPS redirect detection (new HTTP check)
- OCSP stapling check (warn if not stapled)
- DANE/TLSA validity check (pass-through from validation)
- Always-on when `config.quality.enabled` (no opt-in parameter)
- Per-port health check results

**Out of scope** (future work, §8):
- Numeric sub-scores and composite scoring
- Letter grades (A+ through F, T, M)
- Protocol version probing
- Cipher suite enumeration
- Active vulnerability probing
- Configurable/versioned rulesets
- Web security headers beyond HSTS (CSP, X-Frame-Options)

---

## 2. Health Checks

### 2.1 Check Model

Each health check produces a structured result:

```rust
pub struct HealthCheck {
    pub id: &'static str,       // machine-readable identifier
    pub category: Category,     // certificate, protocol, configuration
    pub status: CheckStatus,    // pass, warn, fail, skip
    pub label: &'static str,    // human-readable short label
    pub detail: String,         // specific finding (e.g., "expires in 12 days")
}

pub enum Category {
    Certificate,
    Protocol,
    Configuration,
}
```

`CheckStatus` reuses the existing enum from `validate/mod.rs` (pass, warn, fail, skip).

### 2.2 Relationship to Existing `summary`

The existing `summary` field (`Summary` struct with `verdict` and `SummaryChecks`) overlaps significantly with the quality health checks — both evaluate chain trust, expiry, hostname match, etc. The key differences:

- `summary` is a flat struct with one `CheckStatus` per concern — no detail text, no categories.
- `quality.checks` is a richer list with human-readable labels, detail strings, and categories.

**Design decision**: Health checks **read from the same `ValidationResult`** that `summary` reads from. They are a reformatting with richer output, not a recomputation through a different code path. Both consume the same validated data.

**Coexistence**: The `summary` field is **frozen** — it continues to appear in all responses (with and without `grade=true`) for backwards compatibility. The `quality` field is the superset. Clients should prefer `quality` when present. `summary` will be deprecated in a future API version, announced via changelog. No removal in this phase.

### 2.3 Check Definitions

Every check must be **actionable** — a warn or fail status should correspond to something the operator can fix or investigate. Informational observations that require no action are reported as pass with descriptive detail text, not as warnings.

#### Certificate Checks

| ID | Label | Pass | Warn | Fail | Skip | Source |
|----|-------|------|------|------|------|--------|
| `chain_trusted` | Chain trusted | webpki verification passes | — | Verification fails (self-signed, unknown CA) | No chain available | `ValidationResult.chain_trusted` |
| `not_expired` | Certificate valid | All certs in validity period | — | Any cert expired or not yet valid | No chain available | `ValidationResult.any_expired`, `any_not_yet_valid` |
| `hostname_match` | Hostname match | Leaf SAN covers requested hostname | — | No matching SAN | IP-mode input (no hostname to match) | `ValidationResult.leaf_covers_hostname` |
| `chain_complete` | Chain complete | Chain order correct, intermediates present | `chain_order_correct` is false but `chain_trusted` is pass (webpki was lenient despite misordering) | — | No chain available | `ValidationResult.chain_order_correct`, `chain_trusted` |
| `strong_signature` | Signature algorithm | SHA-256 or stronger across entire chain | — | SHA-1 or MD5 in any chain certificate | No chain available | `ValidationResult.weakest_signature` |
| `key_strength` | Key strength | RSA >= 2048 bits, or ECDSA P-256+ / Ed25519+ | — | RSA < 2048 bits | No chain available | `CertInfo.key_type`, `key_size` |
| `expiry_window` | Expiry window | > 30 days remaining | 8–30 days remaining | <= 7 days remaining | No chain available | `CertInfo.days_remaining` (see note) |

**`expiry_window`**: Uses the **earliest expiry across the entire chain**, not just the leaf. An intermediate expiring in 5 days is as operationally dangerous as a leaf expiring in 5 days — both cause outages. The detail text identifies which certificate is expiring soonest (e.g., "Intermediate cert expires in 12 days (2026-03-20)").

**`key_strength`**: RSA 2048 is pass, not warn. It is the industry standard minimum for all major CAs and the vast majority of the internet. Warning on RSA 2048 would flag nearly every RSA-based site with no actionable remediation (most CAs don't offer 4096-bit leaf certs, and switching is not practical advice). Larger key sizes are noted in the detail text as informational (e.g., "RSA 4096-bit").

#### Protocol Checks

| ID | Label | Pass | Warn | Fail | Skip | Source |
|----|-------|------|------|------|------|--------|
| `tls_version` | TLS version | TLS 1.2 or TLS 1.3 negotiated | — | — | No handshake | `TlsParams.version` |
| `forward_secrecy` | Forward secrecy | Cipher suite uses ECDHE / DHE / TLS 1.3 | — | Static RSA key exchange | No handshake | Cipher suite name parsing |
| `aead_cipher` | AEAD cipher | GCM, ChaCha20-Poly1305, CCM | — | CBC or other non-AEAD mode | No handshake | Cipher suite name parsing |
| `ct_logged` | CT logged | >= 2 embedded SCTs | < 2 SCTs or none found | — | CT checking disabled | Existing `CtInfo` |

**`tls_version`**: Both TLS 1.2 and TLS 1.3 are pass. TLS 1.2 with AEAD ciphers and forward secrecy is secure — warning on it would flag the majority of the internet for something that is not a problem and may not be under the operator's control (the version negotiated depends on both client and server). When TLS 1.3 is negotiated, the detail text says "TLS 1.3". When TLS 1.2, it says "TLS 1.2; older version support cannot be determined without protocol probing." The detection gap is communicated as information, not inflated to a warning.

| `ocsp_stapled` | OCSP stapled | OCSP response stapled | No OCSP staple (clients must contact CA) | — | No handshake | `TlsParams.ocsp.stapled` |

**`ocsp_stapled`**: Absence of OCSP stapling is a warn (not fail) because it degrades client privacy and performance rather than being a security vulnerability. Servers that staple OCSP responses allow clients to verify revocation without contacting the CA directly.

#### Configuration Checks — Hostname-Scoped

These checks run once per request, regardless of port count:

| ID | Label | Pass | Warn | Fail | Skip | Source |
|----|-------|------|------|------|------|--------|
| `hsts` | HSTS | Present, `max-age >= 15768000` (6 months) | Present but `max-age` < 6 months | Not present | HTTP check skipped or failed; IP-mode input | New HTTP check (§3) |
| `https_redirect` | HTTPS redirect | Port 80 redirects to `https://` same host | Redirects to `https://` different host | No redirect / non-redirect response | Port 80 not open (connection refused), timeout, or HTTP check skipped | New HTTP check (§3) |

**`hsts`**: Fail when absent assumes the target is a browser-facing HTTPS service. Operators can disable quality assessment via config (`quality.enabled = false`) or skip HTTP checks specifically (`quality.skip_http_checks = true`) for API-only endpoints and internal services that don't serve browsers.

#### Configuration Checks — Per-Port

These checks are per-port because they depend on port-specific data:

| ID | Label | Pass | Warn | Fail | Skip | Source |
|----|-------|------|------|------|------|--------|
| `caa_compliant` | CAA compliance | Issuer matches CAA `issue` record, or no CAA records | — | CAA records exist and issuer doesn't match | CAA checking disabled or no cert to check against | Existing `caa_compliance` |
| `dane_valid` | DANE valid | TLSA records match presented certificate | — | TLSA records do not match | DANE check skipped (requires DNSSEC) | Existing `dane_status` from validation |
| `consistency` | Multi-IP consistency | All IPs return same leaf cert fingerprint, same TLS version, and same cipher suite | — | Any of the three fields differs across IPs | Single IP or fewer than 2 successful handshakes | Existing `ConsistencyResult` |

**`caa_compliant`**: Per-port because CAA compliance depends on the leaf certificate's issuer, which may differ per port. CAA records are per-domain (fetched once), but if port 443 has a Let's Encrypt cert and port 8443 has a DigiCert cert, each needs a separate issuer check.

**`consistency`**: Per-port because it compares IPs within a single port's handshake results. Uses category `configuration` despite being port-scoped — it is the one configuration check that lives in `ports[].quality.checks` rather than the top-level `quality.checks`. Pass requires all three fields to match across all IPs: leaf certificate SHA-256 fingerprint, negotiated TLS version string, and negotiated cipher suite name. A mismatch on any one field is fail. The detail text lists which fields differ and which IPs returned which values (using existing `ConsistencyMismatch` data).

### 2.4 Verdicts

Each scope (per-port, hostname-scoped) has its own verdict computed from its checks:

- If any check is `fail` → verdict is `fail`
- Else if any check is `warn` → verdict is `warn`
- Else → verdict is `pass`

`skip` checks do not affect the verdict.

**There is no cross-port top-level verdict.** A top-level worst-of rollup across ports is lossy — if port 443 passes and port 8443 has a self-signed cert, a top-level "fail" hides which port is the problem and causes unnecessary alarm. The per-port verdicts and the hostname-scoped verdict are the user-facing signals. The frontend displays each port's verdict independently.

---

## 3. HTTP-Layer Checks

### 3.1 HSTS Check

After TLS handshakes complete, send a single HTTP/1.1 `HEAD` request over a fresh TLS connection. The target port is selected as: **port 443 if it's in the inspected port list, otherwise the first inspected port.** Rationale: HSTS is almost always configured on 443; if someone inspects `example.com:8443,443`, they want the HSTS check on 443, not 8443.

```
HEAD / HTTP/1.1
Host: <hostname>
Connection: close
```

Parse the `Strict-Transport-Security` response header:

| Field | Extraction |
|-------|-----------|
| `max-age` | Integer seconds; absent = 0 |
| `includeSubDomains` | Boolean directive present/absent |
| `preload` | Boolean directive present/absent |

**Target IP**: For multi-IP targets, the HTTP checks connect to the **first resolved IP** only. HSTS is a per-hostname property — checking all IPs is wasteful and would return the same header.

**No redirect following.** The HSTS check reads the first response only. If the server returns a redirect, the `Strict-Transport-Security` header is extracted from that response (HSTS applies regardless of HTTP status code per RFC 6797 §8.1). The redirect target is not followed.

**HEAD vs GET limitation.** Some servers only return `Strict-Transport-Security` on GET responses or specific paths. A `HEAD /` may miss the header. This is a known limitation, documented in the check detail when HSTS is not found: "HSTS header not found in HEAD / response; some servers may only return it on GET requests."

**Security model exception.** The core SDD (§8) states "no application data sent after TLS handshake" for inspection connections. The HSTS check uses a **separate** connection specifically for HTTP data, clearly delineated from inspection handshakes. The parent SDD must be updated to document this exception (see §7).

**Timeout**: 5-second hard cap, independent of the handshake timeout. HSTS check failure is non-fatal — the health check status is `skip`, and a warning explains why.

### 3.2 HTTPS Redirect Check

Send a single HTTP/1.1 `HEAD` request to port 80 (plaintext):

```
HEAD / HTTP/1.1
Host: <hostname>
Connection: close
```

**No redirect following.** Read the first response only. Check whether it's a 301/302/307/308 with a `Location` header starting with `https://`.

| Result | Status | Detail |
|--------|--------|--------|
| 3xx redirect to `https://<same-host>` | pass | "Redirects to https://example.com/" |
| 3xx redirect to `https://<different-host>` | warn | "Redirects to https://other.example.com/ (different host)" |
| Non-redirect response (200, 404, etc.) | fail | "Port 80 responds but does not redirect to HTTPS" |
| Connection refused | skip | "Port 80 not open" |
| Timeout | skip | "Port 80 connection timed out" |

### 3.3 Security Considerations

- **Target policy applies**: HTTP connections go to the same validated, blocklist-checked IPs. No additional DNS resolution.
- **No redirect following**: Prevents SSRF via server-controlled redirect to internal IPs.
- **Response size limit**: Read at most 8KB of response headers. No response body is read.
- **Timeout enforced**: 5-second hard cap per connection.
- **No cookies, no auth**: Requests are minimal `HEAD` with `Connection: close`.
- **Port 80 only for redirect**: The plaintext connection targets only port 80.
- **IP-mode input**: HTTP checks are skipped entirely (no meaningful HSTS or redirect for raw IPs).

### 3.4 Multi-Port Behavior

HSTS is per-hostname, not per-port. When multiple ports are inspected (e.g., `example.com:443,8443`):

- The HSTS check runs once, targeting port 443 if present, otherwise the first port.
- The redirect check runs once (always port 80).
- Both results are reported at the top level (hostname-scoped), not per-port.

---

## 4. API Design

### 4.1 Request

Quality assessment runs automatically when `config.quality.enabled` (default: true). No query parameter needed:

```
GET /api/inspect?h=example.com
```

**Design change**: The original design used an opt-in `grade=true` query parameter. This was removed — quality is always computed when enabled in config. The rationale: quality checks are lightweight (pure logic over existing inspection data + two HTTP requests) and the merged Validation card always shows the results. An opt-in toggle added UI complexity without meaningful benefit.

### 4.2 Response

When quality is enabled, the `InspectResponse` includes a top-level `quality` field for hostname-scoped checks, and each `PortResult` includes a `quality` field for port-scoped checks:

```json
{
  "request_id": "019503ab-...",
  "hostname": "example.com",
  "input_mode": "hostname",
  "summary": {
    "verdict": "pass",
    "checks": { "..." }
  },
  "quality": {
    "verdict": "pass",
    "checks": [
      {
        "id": "hsts",
        "category": "configuration",
        "status": "pass",
        "label": "HSTS",
        "detail": "max-age=31536000, includeSubDomains, preload"
      },
      {
        "id": "https_redirect",
        "category": "configuration",
        "status": "pass",
        "label": "HTTPS redirect",
        "detail": "Port 80 redirects to https://example.com/"
      }
    ],
    "hsts": {
      "present": true,
      "max_age": 31536000,
      "include_sub_domains": true,
      "preload": true
    },
    "https_redirect": {
      "status": "pass",
      "redirect_url": "https://example.com/"
    }
  },
  "ports": [
    {
      "port": 443,
      "quality": {
        "verdict": "warn",
        "checks": [
          {
            "id": "chain_trusted",
            "category": "certificate",
            "status": "pass",
            "label": "Chain trusted",
            "detail": "Certificate chain verified against Mozilla trust store"
          },
          {
            "id": "not_expired",
            "category": "certificate",
            "status": "pass",
            "label": "Certificate valid",
            "detail": "All certificates within validity period"
          },
          {
            "id": "hostname_match",
            "category": "certificate",
            "status": "pass",
            "label": "Hostname match",
            "detail": "Leaf certificate SAN covers example.com"
          },
          {
            "id": "chain_complete",
            "category": "certificate",
            "status": "pass",
            "label": "Chain complete",
            "detail": "Chain order correct, all intermediates present"
          },
          {
            "id": "strong_signature",
            "category": "certificate",
            "status": "pass",
            "label": "Signature algorithm",
            "detail": "Weakest algorithm in chain: sha256WithRSAEncryption"
          },
          {
            "id": "key_strength",
            "category": "certificate",
            "status": "pass",
            "label": "Key strength",
            "detail": "ECDSA P-256 (256-bit)"
          },
          {
            "id": "expiry_window",
            "category": "certificate",
            "status": "warn",
            "label": "Expiry window",
            "detail": "Certificate expires in 22 days (2026-03-30)"
          },
          {
            "id": "tls_version",
            "category": "protocol",
            "status": "pass",
            "label": "TLS version",
            "detail": "TLS 1.3"
          },
          {
            "id": "forward_secrecy",
            "category": "protocol",
            "status": "pass",
            "label": "Forward secrecy",
            "detail": "TLS 1.3 (implicit ECDHE key exchange)"
          },
          {
            "id": "aead_cipher",
            "category": "protocol",
            "status": "pass",
            "label": "AEAD cipher",
            "detail": "AES-256-GCM"
          },
          {
            "id": "ct_logged",
            "category": "protocol",
            "status": "pass",
            "label": "CT logged",
            "detail": "3 embedded SCTs"
          },
          {
            "id": "caa_compliant",
            "category": "configuration",
            "status": "pass",
            "label": "CAA compliance",
            "detail": "Issuer matches CAA issue record"
          },
          {
            "id": "consistency",
            "category": "configuration",
            "status": "skip",
            "label": "Multi-IP consistency",
            "detail": "Single IP resolved; nothing to compare"
          }
        ]
      },
      "ips": [ "..." ],
      "consistency": null,
      "validation": { "..." },
      "tlsa": null,
      "error": null
    }
  ],
  "dns": { "..." },
  "warnings": [],
  "skipped_ips": [],
  "duration_ms": 284
}
```

### 4.3 Field Semantics

| Field | Type | Description |
|-------|------|-------------|
| `quality.verdict` | `string` | Hostname-scoped verdict: `"pass"`, `"warn"`, `"fail"` |
| `quality.checks` | `HealthCheck[]` | Hostname-scoped checks only (hsts, https_redirect) |
| `quality.hsts` | `object?` | Parsed HSTS header; `null` if check skipped/failed |
| `quality.hsts.present` | `bool` | Whether the header was found |
| `quality.hsts.max_age` | `u64` | `max-age` value in seconds |
| `quality.hsts.include_sub_domains` | `bool` | `includeSubDomains` directive present |
| `quality.hsts.preload` | `bool` | `preload` directive present |
| `quality.https_redirect` | `object?` | Redirect check result; `null` if skipped |
| `quality.https_redirect.status` | `string` | `"pass"`, `"warn"`, `"fail"`, `"skip"` |
| `quality.https_redirect.redirect_url` | `string?` | Target URL if redirect found |
| `ports[].quality.verdict` | `string` | Per-port verdict: `"pass"`, `"warn"`, `"fail"` |
| `ports[].quality.checks` | `HealthCheck[]` | Port-scoped checks (certificate, protocol, caa_compliant, consistency) |

The existing `summary` field is unchanged and always present (§2.2). Note: `summary.verdict` may differ from `ports[].quality.verdict` because they evaluate different check sets — for example, `summary` has `not_expired` (pass/fail only, no expiry window warning) while `quality` has `expiry_window` (which warns at 8–30 days). A cert expiring in 22 days produces `summary.verdict: "pass"` but `quality.verdict: "warn"`.

### 4.4 Meta Endpoint Update

The `quality_assessment` field was removed from `/api/meta` features — quality is no longer opt-in, so clients don't need to feature-detect it.

### 4.5 OpenAPI

All new response types (`HealthCheck`, `HstsInfo`, `RedirectInfo`, `QualityResult`, `PortQualityResult`) require `#[derive(ToSchema)]` and utoipa annotations.

---

## 5. Implementation Architecture

### 5.1 Module Structure

```
src/
  quality/
    mod.rs              # Public API: assess(inspection_data, http_data) -> QualityResult
    checks.rs           # Individual health check functions
    http.rs             # HSTS + redirect checks (HTTP HEAD requests)
    types.rs            # HealthCheck, HstsInfo, RedirectInfo, QualityResult
```

Four files. No scoring engine, no ruleset system, no probe module. Those arrive with the letter-grade system in §8.

### 5.2 Pipeline Integration

```
Parse input
  → Resolve IPs → Filter blocked → Cap-and-warn
  → Concurrent: TLS handshakes + DNS lookups
  → Chain validation, CT extraction
  → [if quality enabled] HTTP checks (HSTS + redirect), concurrent with each other
  → [if quality enabled] Compute health checks from inspection data + HTTP results
  → Build response
```

The health check computation is pure logic — it reads existing `IpInspectionResult`, `ValidationResult`, `TlsParams`, `CtInfo`, `CaaInfo`, and `ConsistencyResult` structs and produces `HealthCheck` items. No additional TLS connections. The only new I/O is the HTTP checks.

Each check function in `checks.rs` takes the relevant validated struct and returns a `HealthCheck`. Example:

```rust
pub fn check_chain_trusted(validation: Option<&ValidationResult>) -> HealthCheck { ... }
pub fn check_key_strength(chain: Option<&[CertInfo]>) -> HealthCheck { ... }
pub fn check_forward_secrecy(tls: Option<&TlsParams>) -> HealthCheck { ... }
```

These functions read from `ValidationResult` — the same source that `summarize()` in `validate/mod.rs` reads from. No recomputation.

### 5.3 Cipher Suite Name Parsing

Derive cipher properties from the negotiated suite name. A `classify_cipher_suite(name: &str)` function extracts:

- **Key exchange**: `ECDHE`, `DHE`, `RSA`, or `TLS1.3` (implicit ECDHE)
- **AEAD**: `true` if `GCM`, `CCM`, or `CHACHA20_POLY1305` in name
- **Symmetric bits**: parsed from `AES_128`, `AES_256`, `CHACHA20` (always 256)

This is string matching on known rustls suite name patterns, not cryptographic analysis. Unknown suite names produce a `skip` status with a warning, not a crash.

### 5.4 Error Handling

Quality assessment failures are non-fatal:

| Failure | Behavior |
|---------|----------|
| HSTS check times out | `hsts` check status is `skip`; `quality.hsts` is `null`; warning in detail |
| Redirect check times out | `https_redirect` check status is `skip`; `quality.https_redirect` is `null` |
| Cipher suite name unparseable | `forward_secrecy` and `aead_cipher` checks are `skip` |
| Quality computation panics | `quality` field omitted from response; inspection data returned; error logged |

Principle: **never let health checks degrade the core inspection**.

### 5.5 Rate Limiting

The HTTP checks add to the per-request rate limit cost:

```
cost = ports * ips  (HTTP checks are not rate-limited separately)
```

**Note**: The original design added +2 for HTTP check connections. In the current implementation, HTTP checks are not separately rate-limited — they are lightweight (two HEAD requests to the first resolved IP) and always run when quality is enabled. If rate limiting needs to account for HTTP checks in the future, the cost model can be revisited.

---

## 6. Frontend Integration

### 6.1 Unified Validation Card

Validation and health check are merged into a single always-visible "Validation" card (`ValidationSummary` component). The card has two layers:

**Always visible (header)**:
- Collapsible toggle with "Validation" label
- Verdict badge (pass/warn/fail) — derived from quality checks when available, else from summary
- Count pills (N passed, N warnings, N failed)
- Summary pills row showing the 8 summary checks (chain trusted, not expired, etc.)

**Expanded body** (collapsed by default):
- Hostname-scoped checks (HSTS, HTTPS redirect) at the top
- Per-port detailed checks grouped by category (Certificate, Protocol, Configuration)
- Each check shows status icon, label, and detail text

```
┌─────────────────────────────────────────────────────────┐
│  Validation  [PASS]  [15 passed]                    ▸   │
│  [✓ Chain trusted] [✓ Not expired] [✓ Hostname match]   │
│  [✓ CAA compliant] [— DANE valid] [— CT logged]        │
│  [✓ OCSP stapled] [✓ Consistency]                       │
│─────────────────────────────────────────────────────────│
│  ✓ HSTS — max-age=31536000 (1 year)                    │
│  ✓ HTTPS redirect — HTTP 301 → https://...             │
│                                                         │
│  CERTIFICATE                                            │
│  ✓ Chain trusted — chain verifies to a trusted root     │
│  ✓ Not expired — all certificates are within validity   │
│  ...                                                    │
│  PROTOCOL                                               │
│  ✓ TLS version — TLS 1.3                               │
│  ...                                                    │
│  CONFIGURATION                                          │
│  ✓ CAA compliant — issuing CA authorized by CAA records │
│  — DANE valid — DANE check skipped (requires DNSSEC)    │
│  ✓ IP consistency — all IPs serve matching config       │
└─────────────────────────────────────────────────────────┘
```

### 6.2 Placement

The Validation card appears below the results toolbar/actions and above port tabs and per-IP cards. There is no separate health check card.

### 6.3 No Grade Toggle

**Design change**: The `grade` toggle was removed. Quality assessment runs automatically when enabled in config. The `g` keyboard shortcut and localStorage persistence for `tlsight_grade` were removed.

### 6.4 Exports

The `ExportButtons` component includes quality data when present. The JSON export includes the full `quality` objects. The markdown export adds a "Detailed Checks" section listing each check with its status and detail.

---

## 7. Parent SDD Update

The core SDD (`docs/sdd.md`) must be updated alongside the implementation (not as a separate phase):

1. **§8 Security**: Add an exception to the "no application data after TLS handshake" rule: "Exception: when quality assessment is enabled, separate HTTP connections are made for HSTS and redirect checking. These are distinct from inspection handshakes and governed by the quality SDD (§3.3) security constraints."

2. **§7 Backend**: Reference the `quality/` module in the inspection pipeline.

3. **§14 Phased delivery**: Add quality assessment phases.

---

## 8. Future Work

### 8.1 Letter Grade System

When raw socket probing is available, build the full SSL Labs-style grading engine on top of the health check foundation:

- **Numeric sub-scores**: Protocol support (30%), key exchange (30%), cipher strength (40%) — weighted composite 0–100.
- **Letter grades**: A+ through F derived from composite score with cap rules and hard failures.
- **Special grades**: T (trust failure), M (hostname mismatch) — override the letter grade but show what it would be if resolved.
- **Versioned rulesets**: Named, versioned rule collections (e.g., `v2025.1`) with operator TOML overrides. Build the abstraction when the second ruleset materializes, not before.

The health checks become inputs to the scoring engine — each check maps to a score contribution or cap rule.

### 8.2 Raw Socket Protocol Probing

Build a minimal ClientHello crafter to probe TLS 1.0, TLS 1.1, and SSLv3 support without depending on rustls:

- Send a handshake-only ClientHello with `max_version` set to the target protocol.
- Parse only the ServerHello to determine the negotiated version (or detect rejection).
- No full handshake — no certificate processing needed.

This is the single highest-impact enhancement: it unlocks the most important cap rules (TLS 1.0/1.1 → cap at B, SSLv3 → cap at C).

### 8.3 Cipher Suite Enumeration

Extend the raw socket crafter to enumerate supported cipher suites by sending ClientHello variants with single cipher suites. Unlocks: accurate cipher strength scoring, weak suite detection (RC4, 3DES, export, NULL), forward secrecy assessment across all suites.

### 8.4 Active Vulnerability Probing

Per-vulnerability probes: Heartbleed, ROBOT, Ticketbleed, POODLE (TLS), renegotiation. Each is independent and can be implemented incrementally.

### 8.5 Extended Certificate Parsing

Extract from x509-parser: Extended Key Usage, Basic Constraints (leaf should not be CA), Certificate Policies (DV/OV/EV), Authority Information Access (OCSP responder URL for live queries), CRL Distribution Points.

### 8.6 HSTS Preload Check

Query the Chrome HSTS preload list to determine if the domain is preloaded.

### 8.7 Compliance Framework Mapping

Tag findings against PCI DSS 4.0, NIST SP 800-52r2, BSI TR-02102-2.

---

## 9. Configuration

```toml
[quality]
# Enable quality assessment (always-on when true, no query parameter needed)
# Default: true
enabled = true

# Timeout for HTTP checks (HSTS, redirect) in seconds
# Hard cap: 5
# Default: 5
http_check_timeout_secs = 5

# Skip HTTP checks entirely (HSTS, redirect)
# Useful for environments where outbound HTTP is blocked
# Default: false
skip_http_checks = false
```

Three fields. No ruleset selection, no weight overrides, no threshold tuning — those arrive with the letter-grade system.

Environment variable override follows the existing convention:

```
TLSIGHT_QUALITY__ENABLED=true
TLSIGHT_QUALITY__HTTP_CHECK_TIMEOUT_SECS=5
TLSIGHT_QUALITY__SKIP_HTTP_CHECKS=false
```

Hard caps:

| Parameter | Cap | Rationale |
|-----------|-----|-----------|
| `http_check_timeout_secs` | 5s | Prevent slow servers from stalling the response |

---

## 10. Security Considerations

### 10.1 HTTP Requests

The HSTS and redirect checks introduce outbound HTTP connections — a new attack surface:

- **Target policy applies**: HTTP connections use the same resolved, blocklist-checked IPs. No additional DNS resolution. No redirect following (prevents SSRF via server-controlled Location header).
- **Response size limit**: Read at most 8KB of response headers. No response body is read.
- **Timeout enforced**: 5-second hard cap per connection.
- **No cookies, no auth, no state**: Requests are minimal `HEAD` with `Connection: close`.
- **Port 80 only for redirect**: Plaintext connection targets only port 80.
- **IP-mode input**: HTTP checks skipped entirely.
- **HSTS port**: Port 443 if in the inspected port list, otherwise the first inspected port.

### 10.2 Rate Limiting

Quality assessment is accounted for in rate limiting (§5.5). If the budget is insufficient, quality is skipped before any HTTP connections are made.

---

## 11. Testing Strategy

### 11.1 Unit Tests

| File | Tests |
|------|-------|
| `checks.rs` | Each check function with passing, warning, failing, and skip inputs. Cipher suite name parsing for all known rustls suites. Key strength thresholds at boundaries (RSA 1024 → fail, 2048 → pass). Signature algorithm detection (SHA-1 → fail, SHA-256 → pass). Expiry window boundaries (7, 8, 30, 31 days). Consistency check: all-match → pass, fingerprint-mismatch → fail, version-mismatch → fail, single IP → skip. |
| `http.rs` | HSTS header parsing: valid with all directives, missing header, malformed `max-age`, multiple HSTS headers (first wins per RFC 6797). Redirect detection: 301/302/307/308 with https Location, 301 with http Location (fail), 200 (fail), connection refused (skip). Port selection: prefers 443 when present. |
| `types.rs` | Verdict computation from check lists. Serialization matches expected JSON shape. |

### 11.2 Integration Tests

- `do_inspect` with `grade=true` against rcgen test server — `quality` field present on both top-level and port, all certificate and protocol checks populated.
- `grade=false` — `quality` field absent on both levels, no HTTP connections attempted.
- `grade=true` with IP-mode input — HTTP checks are `skip`, certificate checks still run.
- Quality computation failure (e.g., mock a panic) — inspection data still returned without `quality`.
- Rate limiting with grade cost — verify quality skipped when budget insufficient.
- Multi-port — verify hostname-scoped checks appear once at top level, port-scoped checks appear per-port.

### 11.3 Frontend Tests

- Health check section renders hostname-scoped and per-port groups.
- Correct icons for each status (pass/warn/fail/skip).
- Section hidden when `quality` field absent.
- Grade toggle persists in localStorage.
- Toggle triggers re-query.
- Export includes quality data when present.

---

## 12. Design Decisions

**Decision 1: Health checks, not letter grades.**
A letter grade system without cipher enumeration and protocol probing gives every server an A. A checklist of concrete findings is immediately useful and honest about its coverage. The grade system is built later on top of this foundation when probing makes scoring discriminating.

**Decision 2: Always-on via config, not opt-in query parameter.**
Originally designed as opt-in via `grade=true`. Changed to always-on when `config.quality.enabled` — quality checks are lightweight and the merged Validation card always shows results. An opt-in toggle added UI complexity without meaningful benefit. Operators can disable quality entirely via config if needed.

**Decision 3: HSTS and redirect only, no other HTTP headers.**
HSTS prevents TLS downgrade attacks — directly TLS-relevant. Other security headers (CSP, X-Frame-Options) are web security, not TLS quality. Scope discipline.

**Decision 4: No protocol version probing in this phase.**
The only rustls-possible probe (TLS 1.2) is useless — the primary handshake already proves TLS 1.2 support. Probing is deferred entirely until raw sockets enable TLS 1.0/1.1/SSLv3 detection.

**Decision 5: No ruleset system in this phase.**
One set of check definitions, hardcoded. The ruleset abstraction arrives when the letter-grade system needs configurable weights and thresholds. YAGNI.

**Decision 6: Per-port quality, hostname-scoped HTTP checks, no cross-port verdict.**
Different ports may serve different certificates and TLS configs — checks must be per-port. HSTS and redirect are per-hostname properties — reported once. A cross-port worst-of verdict is lossy and misleading; per-port verdicts are sufficient.

**Decision 7: Non-fatal quality failures.**
The core value is inspection data. If health checks fail (HTTP timeout, parse error, bug), inspection data is still returned. Quality is an enhancement, not a gate.

**Decision 8: Rate limit cost charged upfront, not refunded.**
Simpler accounting. If port 80 refuses the connection, the cost is already paid. This prevents gaming and keeps the rate limiter stateless with respect to quality outcomes.

**Decision 9: `summary` frozen, `quality` is the superset.**
The existing `summary` continues to appear for backwards compatibility. `quality` reads from the same `ValidationResult` — it's a richer reformatting, not a recomputation. Deprecation of `summary` is a future API version concern.

**Decision 10: Only actionable findings produce warnings.**
RSA 2048 is pass (industry standard, not actionable). TLS 1.2 is pass (secure, detection gap is informational). OCSP stapling absence is warn (degrades client privacy/performance, server can fix). Warnings must correspond to something the operator can fix.

**Decision 11: Merged Validation + Health Check card.**
Originally two separate cards (ValidationSummary for the 8-pill summary, QualityView for detailed checks). Merged into a single "Validation" card — the pills are always visible as a quick glance, the detailed checks are in the collapsible body. Reduces visual clutter and eliminates the overlapping information between the two views.

---

## 13. Phased Delivery

### Phase Q1: Health Check Engine + HTTP Checks — DONE
- [x] `quality/` module with `types.rs`, `checks.rs`, `http.rs`
- [x] Health check functions for all check IDs defined in §2.3 (including `ocsp_stapled` and `dane_valid`)
- [x] Cipher suite name parser (key exchange, AEAD, symmetric bits)
- [x] HSTS HTTP check (HEAD, port 443 preferred, no redirect following)
- [x] HTTPS redirect check (HEAD to port 80, no redirect following)
- [x] Always-on quality when `config.quality.enabled` (no `grade` parameter)
- [x] `QualityResult` and `PortQualityResult` response types
- [x] Hostname-scoped quality at top level, port-scoped quality per `PortResult`
- [x] Config: `[quality]` section in TOML
- [x] Unit tests for all checks, cipher suite parsing, HTTP parsing, verdict computation
- [x] Removed `quality_assessment` from `/api/meta` features (no longer opt-in)

### Phase Q2: Frontend — DONE
- [x] Unified Validation card (merged ValidationSummary + QualityView)
- [x] Summary pills always visible, detailed checks in collapsible body
- [x] Status icons (pass/warn/fail/skip)
- [x] No grade toggle (always-on)
- [x] Quality data in JSON/markdown exports
- [x] Deleted QualityView.tsx (logic merged into ValidationSummary)

### Phase Q3: Letter Grade System (§8)
- [ ] Raw socket ClientHello crafter
- [ ] Protocol version probing (TLS 1.0, 1.1, SSLv3)
- [ ] Numeric sub-scores and composite scoring
- [ ] Letter grade derivation with cap rules
- [ ] Versioned rulesets with TOML overrides
- [ ] Cipher suite enumeration

---

## 14. Risks & Open Questions

1. **Health checks are less impressive than letter grades.** Users expect an "A" badge. A checklist of pass/warn/fail is less immediately satisfying. Mitigation: the checklist is *accurate* and *useful*; a misleading A is worse than an honest checklist. The letter grade comes when we can compute it correctly.

2. **HSTS HEAD limitation.** Some servers don't return HSTS on HEAD requests. We'll miss HSTS on those servers and incorrectly report fail. Mitigation: document the limitation in the check detail. If this proves problematic in practice, switch to GET with no body read.

3. **CDN behavior.** CDNs may return different TLS configurations based on client geography or ClientHello. Our health check reflects what our server sees. This is inherent to any remote testing tool and not specific to our approach.

4. **Port 80 availability.** Many modern deployments don't expose port 80. The redirect check handles this gracefully (skip), but users may wonder why it's skipped. Clear detail text.

5. **Detection gaps are invisible.** We can't tell users "your server also accepts TLS 1.0" because we can't detect it. The TLS version check says "TLS 1.2" with a note about the detection limitation, but users may assume a clean bill of health. Mitigation: prominent "assessment coverage" indicator in the frontend showing which checks could not be fully evaluated.
