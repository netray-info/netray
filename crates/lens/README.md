# lens

**Unified domain health check — one request, three signals, one grade.**

lens is a web-based tool that checks a domain's DNS configuration, TLS certificate, and IP reputation in a single operation. It fans out to mhost-prism, tlsight, and ifconfig-rs in parallel, streams results as they arrive, and produces a letter-graded health score. No plugins, no CLI setup — just a domain and a result.

Live at [lens.netray.info](https://lens.netray.info) · Part of the [netray.info](https://netray.info) toolchain alongside [dns.netray.info](https://dns.netray.info), [tls.netray.info](https://tls.netray.info), and [ip.netray.info](https://ip.netray.info).

---

## What it does

Given a domain name, lens:

- **Checks DNS** — queries mhost-prism for MX, SPF, DMARC, DKIM, BIMI, MTA-STS, and TLSRPT records; surfaces lint findings
- **Checks TLS** — queries tlsight for certificate validity, chain trust, protocol version, cipher strength, and OCSP stapling
- **Checks IP reputation** — queries ifconfig-rs for each resolved IP; classifies as residential, datacenter, VPN, Tor, or known botnet C2
- **Runs in parallel** — DNS and TLS checks fire concurrently; IP enrichment runs after DNS resolves addresses
- **Streams results via SSE** — each signal arrives as soon as its backend responds, no waiting for the slowest one
- **Scores and grades** — weighted aggregate of check results produces an A+–F grade with per-section breakdowns
- **Enforces a 20s deadline** — partial results returned if any backend is slow or unreachable

---

## API

### Check endpoint

```
GET /api/check?d=domain
```

Returns a Server-Sent Events stream. Each event carries a JSON payload.

```sh
curl -N 'https://lens.netray.info/api/check?d=example.com'
```

#### SSE event types

| Event | Description |
|---|---|
| `dns` | DNS check result from mhost-prism (lint findings, resolved IPs) |
| `tls` | TLS check result from tlsight (cert chain, quality checks) |
| `ip` | IP reputation result from ifconfig-rs (per-IP network classification) |
| `summary` | Aggregated score, per-section grades, and overall grade |
| `done` | Stream terminator (no data payload) |
| `error` | Structured error if the request is rejected before streaming begins |

#### Event format

```
event: dns
data: {"status":"ok","findings":[...],"resolved_ips":["93.184.216.34"]}

event: tls
data: {"status":"ok","checks":[{"name":"chain_trusted","status":"pass"},...],"grade":"A"}

event: ip
data: {"status":"ok","ips":[{"ip":"93.184.216.34","network_type":"hosting","asn":15133}]}

event: summary
data: {"grade":"A","score":87,"sections":{"dns":{"grade":"B","score":72},"tls":{"grade":"A","score":92},"ip":{"grade":"A+","score":100}}}

event: done
data:
```

#### Error format

If the request is rejected before streaming (invalid domain, rate limited), a standard JSON error is returned:

```json
{"error": {"code": "invalid_domain", "message": "..."}}
```

### Other endpoints

| Endpoint | Description |
|---|---|
| `GET /health` | Liveness probe |
| `GET /ready` | Readiness probe |
| `GET /api/meta` | Server capabilities and configured backends |
| `GET /api-docs/openapi.json` | OpenAPI 3.1 spec |
| `GET /docs` | Interactive API documentation |

### CI / Pipeline integration

Use in GitHub Actions to gate deploys on domain health:

```yaml
# Fail the build if the overall grade is below B
- run: |
    curl -sN 'https://lens.netray.info/api/check?d=$DOMAIN' \
      | grep '^data:' \
      | awk -F'data: ' '/summary/{print $2;exit}' \
      | jq -e '.grade | test("^(A|B)")'
```

---

## Scoring

This section is the authoritative description of the scoring algorithm. Any change to `src/scoring/` must update this section in the same commit.

### Algorithm

Each backend returns a set of named checks. Every check has a status: `pass`, `warn`, `fail`, `not_found`, `skip`, or `error`.

1. **Per-check score**: `pass` = full weight, `warn` = half weight, `fail`/`not_found` = 0 points. `skip` and `error` are excluded from the totals entirely.
2. **Section score**: weighted sum of earned points divided by weighted sum of possible points, expressed as a percentage.
3. **Overall score**: weighted average of section scores using section weights.
4. **Hard-fail overrides**: certain critical failures force the overall grade to F regardless of the numeric score.
5. **Letter grade**: overall score mapped to grade thresholds.

### Weight tiers

Weights express the relative importance of each check within its section:

| Weight | Meaning |
|---|---|
| 10 | Security-critical — failure has immediate, severe consequences |
| 5 | Important — strongly recommended by standards or best practice |
| 3 | Significant — meaningful impact on deliverability or security posture |
| 2 | Advisory — good practice, but failure is not operationally harmful |
| 1 | Informational — present/absent signals low-cost improvement opportunities |

### Section weights

| Section | Weight | Rationale |
|---|---|---|
| TLS | 45% | Certificate validity and transport security are foundational |
| DNS | 35% | Email authentication and DNS health have major deliverability impact |
| IP | 20% | Reputation informs risk but is beyond the domain owner's direct control |

### Grade thresholds

| Grade | Score | Meaning |
|---|---|---|
| A+ | ≥ 97% | Exemplary — all checks pass |
| A | ≥ 90% | Excellent — minor gaps only |
| B | ≥ 75% | Good — some non-critical findings |
| C | ≥ 60% | Fair — notable gaps, action recommended |
| D | ≥ 40% | Poor — significant issues present |
| F | < 40% | Failing — or hard-fail override triggered |

### Hard failures

The following conditions force the overall grade to **F** regardless of the numeric score:

- Untrusted TLS chain (certificate not signed by a trusted CA)
- Expired certificate (any certificate in the chain)
- No SPF record (missing entirely, not a misconfigured one)
- No DMARC record (missing entirely)

These represent baseline requirements. A domain that fails any of them poses immediate risk to recipients or operators.

### `not_found` treatment

A check that returns `not_found` (e.g. a missing SPF record, absent DMARC policy) scores 0 points — the same as an explicit `fail`. The distinction is preserved in the event payload for display purposes but has no effect on scoring.

### Skipped and errored checks

- `skip` — the check was intentionally not run (e.g. DANE requires DNSSEC). Excluded from both earned and possible points.
- `error` — the check could not complete due to a backend error. Also excluded from totals. The response includes a `warnings` array listing which checks were excluded and why.

Excluding errored checks means a backend outage degrades score precision but does not artificially inflate or deflate the result.

If all three backends fail and every section is excluded, no numeric score can be computed. The grade is reported as `error` and the UI shows "Grade unavailable" instead of a letter grade.

### Custom scoring profiles

The scoring profile (weights, thresholds, hard-fail rules) is defined in a TOML file. The default profile is embedded in the binary. You can override it by setting `scoring.profile_path` in `lens.toml`.

Example profile structure (v2 format):

```toml
[meta]
name = "default"
version = 2

[sections.tls]
weight = 45
hard_fail = ["chain_trusted", "not_expired"]

[sections.tls.checks]
chain_trusted   = 10
not_expired     = 10
hostname_match  = 10
chain_complete  = 5
strong_signature = 5
key_strength    = 5
expiry_window   = 5
tls_version     = 5
forward_secrecy = 5
aead_cipher     = 5
ocsp_stapled    = 3
ct_logged       = 3
# ... more checks

[sections.dns]
weight = 35
hard_fail = ["spf", "dmarc"]

[sections.dns.checks]
spf   = 10
dmarc = 10
dnssec = 5
caa   = 5
mx    = 5
# ... more checks

[sections.ip]
weight = 20
hard_fail = []

[sections.ip.checks]
reputation = 5

[thresholds]
"A+" = 97
"A"  = 90
"B"  = 75
"C"  = 60
"D"  = 40
"F"  = 0
```

Each `[sections.<name>]` block defines:
- `weight` — percentage weight of this section in the overall score (all weights must sum to 100)
- `hard_fail` — check names that trigger an automatic F grade if they fail
- `checks` — individual check names and their point weights

Adding a new scoring section requires only a new `[sections.<name>]` block in the profile and a corresponding backend implementation.

---

## Configuration

Copy `lens.example.toml` to `lens.toml` and adjust as needed.

```toml
[server]
bind = "0.0.0.0:8082"
metrics_bind = "127.0.0.1:8090"
# trusted_proxies = ["10.0.0.0/8"]

[backends]
dns_url = "https://dns.netray.info"
tls_url = "https://tls.netray.info"
ip_url  = "https://ip.netray.info"
# backend_timeout_secs = 20

[cache]
enabled = true
ttl_seconds = 300

[rate_limit]
per_ip_per_minute = 10
per_ip_burst = 3
global_per_minute = 100
global_burst = 20

[scoring]
# profile_path = "profiles/default.toml"   # override built-in default profile
```

Configuration is loaded from `lens.toml` by default. Override the path with the `LENS_CONFIG` environment variable. Environment variables take precedence over the file, using the `LENS_` prefix with `__` as the section separator — e.g. `LENS_SERVER__BIND=0.0.0.0:8082`.

---

## Building

Prerequisites: Rust toolchain, Node.js (for the frontend).

```sh
# Full production build (frontend + Rust binary)
make

# Run the built binary
make run

# Development (two terminals)
make frontend-dev   # Vite dev server on :5174, proxies /api/* to :8082
make dev            # cargo run

# Tests
make test           # Rust + frontend
make ci             # Full CI: lint + test + frontend build
```

The release binary embeds the compiled frontend. No separate static file hosting required.

---

## Tech stack

**Backend**: Rust · axum · reqwest · tokio · tower-governor

**Frontend**: SolidJS · Vite · TypeScript (strict)

---

## License

MIT — see [LICENSE](LICENSE).
