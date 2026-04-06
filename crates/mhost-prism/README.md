<p align="center">
  <strong><code>prism</code></strong><br>
  <em>DNS, refracted.</em>
</p>

<p align="center">
  Multi-resolver fan-out. Streaming results. Health checks. Delegation traces.<br>
  Transport comparison. Auth vs recursive. All in your browser.
</p>

<p align="center">
  <a href="#quick-start">Quick Start</a> &middot;
  <a href="#six-modes">Six Modes</a> &middot;
  <a href="#the-query-language">Query Language</a> &middot;
  <a href="#api">API</a> &middot;
  <a href="#deployment">Deployment</a>
</p>

Part of the [netray.info](https://netray.info) network intelligence suite.

---

**prism** is a web-based DNS inspector powered by [mhost](https://github.com/lukaspustina/mhost). It asks the questions `dig` can't ask — like "do all my resolvers agree?" and "is my ISP lying to me?" — then streams the answers to your browser as they arrive.

One binary. One config file. Zero dependencies at runtime.

## Quick start

```sh
git clone https://github.com/lukaspustina/mhost-prism
cd mhost-prism
make
./target/release/prism
# http://localhost:8080
```

## Six modes

### Query

Ask multiple resolvers the same question at the same time. See who disagrees, who's slow, and who drops records. Results stream in per record type — no staring at a spinner.

```
example.com A AAAA MX @cloudflare @google @quad9
```

### Check

Full domain audit in one shot: 15 record types plus DMARC lint. Catches broken SPF, missing CAA, DNSSEC mismatches, stale MX, and more — the kind of things that bite you at 2am.

Additional checks included:

- **NS lame delegation**: validates the AA bit per nameserver to detect lame delegations
- **Delegation-consistent NS**: diffs parent vs. child NS sets and flags discrepancies
- **DNSSEC rollover detection**: identifies multiple KSKs, orphaned DS records, and missing DS for a new KSK
- **DNSKEY algorithm security**: flags deprecated algorithms (RSA/MD5, RSA/SHA-1)

```
example.com +check
```

### Trace

Walk the delegation chain from root servers to authoritative, hop by hop. See exactly where the referral chain breaks, where TTLs drop, and which glue records are stale.

```
example.com A +trace
```

### Compare

Query over UDP, TCP, DNS-over-TLS, and DNS-over-HTTPS simultaneously. Four columns, one truth. Instantly reveals middlebox interference, transport-specific filtering, or that one protocol your corporate firewall silently rewrites.

```
example.com A +compare
```

### Auth

Discover the authoritative nameservers, query them directly (RD=0), and compare against your recursive resolver. Two columns: what the authority says vs what your resolver cached. Reveals stale caches, NXDOMAIN hijacking, and split-horizon setups.

```
example.com MX +auth
```

### DNSSEC

Fetch DNSKEY and DS records alongside your query. Inspect the chain of trust without memorizing RFC 4035.

```
example.com +dnssec
```

## Frontend features

- **TTL countdown**: displayed TTLs decrement live in the browser after records arrive
- **SOA serial age**: badge next to SOA serial shows how long ago it was set (e.g. "45 days ago")
- **Trace latency heatmap**: per-hop coloured latency bars in Trace view
- **Server latency summary**: median latency per resolver shown in the Servers tab
- **TTL-only divergence**: shown in amber; data divergence remains red

## The query language

A `dig`-inspired single-line syntax. Type a domain, add what you want, hit Enter.

```
domain [TYPE...] [@server...] [+flag...]
```

Tab-complete everything. Works in the search bar, the API, and shareable URLs.

| Token | What it does | Examples |
|-------|-------------|---------|
| Domain | Name or IP to query | `example.com`, `192.0.2.1` |
| Record type | DNS record types | `A` `AAAA` `MX` `TXT` `NS` `SOA` `CAA` `CNAME` `SRV` `PTR` `HTTPS` `SVCB` `SSHFP` `TLSA` `NAPTR` `DNSKEY` `DS` |
| Server | Individual resolver | `@cloudflare` `@google` `@quad9` `@mullvad` `@wikimedia` `@dns4eu` `@system` `@1.1.1.1` |
| Server group | Alias expanding to multiple resolvers | `@public` (Google + Cloudflare + Quad9), `@cloudflare` (1.1.1.1 + 1.0.0.1), `@google` (8.8.8.8 + 8.8.4.4), `@quad9` (9.9.9.9 + 149.112.112.112), `@all` (all public resolvers, capped to 4) |
| Transport | Protocol | `+udp` `+tcp` `+tls` `+https` |
| Mode | Switch endpoint | `+check` `+trace` `+compare` `+auth` `+dnssec` |
| Flag | Query behaviour | `+norecurse` (set RD=0, non-recursive query), `+short` (suppress TTLs in output) |

```sh
# Compare three resolvers over TLS with DNSSEC
example.com A AAAA @cloudflare @google @quad9 +tls +dnssec

# Reverse DNS via system resolver
192.0.2.1 PTR @system
```

## API

Every mode is also an API endpoint. All return Server-Sent Events — connect with `curl`, `EventSource`, or any SSE client.

| Endpoint | Mode | Trigger |
|----------|------|---------|
| `GET /api/query?q=...` | Query | Shareable URLs, `curl`, `EventSource` |
| `POST /api/query` | Query | Structured JSON body |
| `POST /api/check` | Check | Health audit |
| `POST /api/trace` | Trace | Delegation walk |
| `POST /api/compare` | Compare | Transport comparison |
| `POST /api/authcompare` | Auth | Auth vs recursive |
| `POST /api/parse` | -- | Tokenization + completions |

```sh
# Stream results with curl
curl -sN 'http://localhost:8080/api/query?q=example.com+MX+%40cloudflare'

# Structured POST
curl -sN -X POST http://localhost:8080/api/query \
  -H 'Content-Type: application/json' \
  -d '{"domain":"example.com","record_types":["A","MX"],"servers":["cloudflare","google"]}'
```

**Non-streaming mode:** Any endpoint accepts `?stream=false` (or `Accept: application/json`) to collect the full SSE stream server-side and return a single JSON response:

```json
{ "events": [...], "truncated": true }
```

`truncated` is `true` if the stream hit the 30s deadline before completing.

```sh
# Collect full response as JSON
curl -s 'http://localhost:8080/api/query?q=example.com+A&stream=false'
```

**SSE events:** `batch` (per record type), `lint` (check mode), `hop` (trace mode), `enrichment` (IP metadata), `done` (always last).

Interactive docs at `GET /docs` (Scalar UI). OpenAPI spec at `GET /api-docs/openapi.json`.

| Utility endpoint | Description |
|-----------------|-------------|
| `GET /api/health` | Liveness probe (always 200 while running) |
| `GET /api/ready` | Readiness probe (503 when a circuit breaker is open) |
| `GET /api/servers` | Predefined resolvers and IPs |
| `GET /api/record-types` | Queryable record types |
| `GET /api/results/{key}` | Retrieve a cached result by permalink key |

## Deployment

Single binary, frontend baked in. Copy, configure, run.

```sh
cp prism.example.toml prism.toml
./prism prism.toml
```

### Configuration

Layered: CLI arg / `PRISM_CONFIG` env var > TOML file > defaults. Env vars use `PRISM_` prefix with `__` as section separator.

```sh
PRISM_SERVER__BIND=0.0.0.0:8080
PRISM_DNS__ALLOW_ARBITRARY_SERVERS=true
PRISM_LIMITS__PER_IP_PER_MINUTE=200
```

<details>
<summary><strong>Full config reference</strong></summary>

```toml
[server]
bind = "127.0.0.1:8080"          # API + frontend
metrics_bind = "127.0.0.1:9090"  # Prometheus metrics (keep on loopback — do not expose publicly)
trusted_proxies = []              # Individual IP addresses of trusted reverse proxies (not CIDR)

[limits]
per_ip_per_minute = 120     # GCRA tokens per client IP per minute
per_ip_burst = 64           # burst allowance (must cover combined mode costs)
per_target_per_minute = 60  # tokens per DNS target per minute
per_target_burst = 20       # per-target burst size
global_per_minute = 1000    # total across all clients
global_burst = 50           # global burst size
max_concurrent_connections = 256  # maximum concurrent TCP connections
per_ip_max_streams = 10     # maximum concurrent SSE streams per client IP
max_record_types = 10       # per-query cap (hard cap: 10)
max_servers = 4             # per-query cap (hard cap: 4)
max_timeout_secs = 10       # hard cap

[trace]
max_hops = 10             # delegation depth (hard cap: 20)
query_timeout_secs = 3    # per-hop timeout

[dns]
default_servers = ["cloudflare"]
allow_system_resolvers = true
allow_arbitrary_servers = false   # set true only in trusted environments

[circuit_breaker]
failure_threshold = 0.5
window_secs = 60
cooldown_secs = 30
min_requests = 5

[ecosystem]
# ifconfig_url = "https://ip.example.com"      # enables clickable IPs + enrichment badges
# ifconfig_api_url = "https://ip.example.com"  # backend API URL (defaults to ifconfig_url)
# enrichment_timeout_ms = 500                  # hard cap: 2000

[performance]
resolver_pool_ttl_secs = 300          # TTL for cached resolver instances
resolver_pool_max_size = 32           # maximum resolver instances in pool
resolver_pool_cleanup_interval_secs = 60  # pool eviction task interval

[telemetry]
log_format = "text"          # "text" (default) or "json" (for log aggregators)
enabled = false              # set true to export OpenTelemetry spans
# otlp_endpoint = "http://localhost:4318"
# service_name = "prism"
# sample_rate = 1.0
```

</details>

### Reverse proxy

<details>
<summary><strong>nginx</strong></summary>

```nginx
server {
    listen 443 ssl http2;
    server_name dns.example.com;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # SSE support
        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 60s;
    }
}
```

```toml
[server]
trusted_proxies = ["127.0.0.1", "10.0.0.0/8"]   # individual IPs or CIDR ranges
```

</details>

<details>
<summary><strong>Caddy</strong></summary>

```
dns.example.com {
    reverse_proxy 127.0.0.1:8080
}
```

</details>

### Hardening checklist

- `allow_arbitrary_servers = false` (default) -- no custom resolver IPs
- `allow_system_resolvers = false` if you don't want `/etc/resolv.conf` exposed
- `trusted_proxies` lists your reverse proxy IP addresses or CIDR ranges (e.g. `["10.0.0.1", "172.16.0.0/12"]`)
- Metrics port (`:9090`) stays on loopback
- Rate limits tuned for your traffic

## Security

prism is designed for the public internet. Security is structural, not bolted on.

| Layer | What it does |
|-------|-------------|
| **Query restrictions** | Blocks `ANY`/`AXFR`/`IXFR`, rejects private IPs as targets, enforces domain length limits |
| **GCRA rate limiting** | Per-IP, per-target, and global token buckets. Cost scales with query complexity |
| **IP extraction** | Proxy-aware client IP from `CF-Connecting-IP` / `X-Real-IP` / `X-Forwarded-For` |
| **Security headers** | CSP, HSTS, `X-Frame-Options`, `Referrer-Policy`, `nosniff`. Same-origin CORS |

No server-side DNS caching. Every request hits resolvers fresh. That's the point -- it's a debugging tool.

## Development

```sh
# Two terminals:
make dev            # Rust server on :8080
make frontend-dev   # Vite on :5173, proxies /api/* to :8080

# CI pipeline (run before pushing):
make ci             # lint + test + frontend + build
```

<details>
<summary><strong>Architecture</strong></summary>

```
src/
  main.rs              # Axum server, graceful shutdown
  parser.rs            # Query language parser (single source of truth)
  config.rs            # Layered config (TOML + env vars)
  error.rs             # ApiError -> HTTP status + error codes
  dns_trace.rs         # Iterative delegation walker
  circuit_breaker.rs   # Per-provider sliding-window breaker
  record_format.rs     # Human-readable TXT, CAA, MX, SOA
  api/
    query.rs           # FuturesUnordered fan-out, 30s deadline
    check.rs           # 15-type lint sweep
    trace.rs           # Delegation walk
    compare.rs         # Transport comparison
    authcompare.rs     # Auth vs recursive
    parse.rs           # Completion hints
    meta.rs            # Health, servers, record-types
  security/
    rate_limit.rs      # 3-tier GCRA
    ip_extract.rs      # Proxy-aware client IP
    query_policy.rs    # Target validation, limits

frontend/src/
  App.tsx              # State, SSE, theme, history, keyboard nav
  components/
    QueryInput.tsx     # CodeMirror 6 with autocomplete
    ResultsTable.tsx   # Streaming results table
    ServerComparison.tsx
    TransportComparison.tsx
    AuthComparison.tsx
    LintTab.tsx
    TraceView.tsx
  lib/tokenizer.ts     # Syntax highlighting (cosmetic only)
  styles/global.css    # Plain CSS, custom properties
```

The Rust parser is the **single source of truth**. The TypeScript tokenizer is cosmetic -- wrong colors, never wrong queries.

</details>

## Stack

| | |
|---|---|
| **Backend** | Rust, [axum](https://github.com/tokio-rs/axum) 0.8, [mhost](https://github.com/lukaspustina/mhost) |
| **Streaming** | Server-Sent Events (`tokio::sync::mpsc` + `ReceiverStream`) |
| **Security** | [tower-governor](https://github.com/benwis/tower-governor) (GCRA), 4-layer defense-in-depth |
| **Frontend** | [SolidJS](https://solidjs.com) + TypeScript (strict), [CodeMirror 6](https://codemirror.net), [Vite](https://vitejs.dev) |
| **Packaging** | [rust-embed](https://github.com/pyros2097/rust-embed) -- entire SPA baked into the binary |
| **Observability** | Prometheus metrics, optional OpenTelemetry tracing |

## License

MIT OR Apache-2.0
