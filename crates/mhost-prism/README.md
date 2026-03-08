# prism

**A DNS debugging tool that doesn't make you feel like you're reading assembly.**

prism is a web-based DNS inspector powered by [mhost](https://github.com/lukaspustina/mhost). It fans out queries to multiple resolvers simultaneously, walks delegation chains, and runs health checks — all streaming results to your browser as they arrive.

Think `dig` on steroids, minus the terminal, plus a UI that actually shows you when two resolvers disagree.

---

## What it does

**Query** — Ask multiple DNS resolvers the same question at the same time. See who answers differently, who's faster, and who has DNSSEC configured. Results stream in as they arrive — no waiting for the slowest resolver to finish.

**Check** — Run 15 record types + DMARC lint against a domain in one shot. prism surfaces common misconfigurations: missing SPF, broken DMARC policies, DNSKEY/DS mismatches, misconfigured MX records.

**Trace** — Walk the full delegation chain from root servers to authoritative, hop by hop. Find out where your delegation breaks, where the TTL drops, and which nameserver is actually authoritative for your zone.

**Compare** — Query the same domain over all four transports (UDP, TCP, DNS-over-TLS, DNS-over-HTTPS) in parallel. See which transports return different answers, which are blocked, and where middleboxes are interfering.

**Auth** — Discover the domain's authoritative nameservers, then query both authoritative (RD=0) and recursive resolvers side by side. Instantly reveals caching staleness, NXDOMAIN hijacking, or split-horizon inconsistencies.

---

## Quick start

```sh
git clone https://github.com/lukaspustina/mhost-prism
cd mhost-prism
make          # build frontend + backend
./target/release/prism
# open http://localhost:8080
```

---

## The query language

prism uses a `dig`-inspired single-line syntax that works the same everywhere — the search bar, the API, and URL sharing.

```
domain [TYPE...] [@server...] [+flag...]
```

Type it, get results. Tab-completion fills in record types, servers, and flags.

### Tokens

| Token | What it does | Examples |
|-------|-------------|---------|
| **Domain** | The name (or IP) to query | `example.com`, `192.0.2.1` |
| **Record type** | One or more DNS record types | `A`, `AAAA`, `MX`, `TXT`, `NS`, `SOA`, `CAA`, `CNAME`, `DNSKEY`, `DS`, `HTTPS`, `SVCB`, `SRV`, `SSHFP`, `TLSA`, `NAPTR` |
| **Server** | Which resolver(s) to ask | `@cloudflare`, `@google`, `@quad9`, `@mullvad`, `@wikimedia`, `@dns4eu`, `@system`, `@1.1.1.1`, `@8.8.8.8:53` |
| **Transport** | How to speak DNS | `+udp` (default), `+tcp`, `+tls`, `+https` |
| **DNSSEC** | Request DNSSEC validation | `+dnssec` |
| **Mode** | Switch to a different endpoint | `+check`, `+trace`, `+compare`, `+auth` |

Blocked types: `ANY`, `AXFR`, `IXFR`. Private and loopback IPs are rejected as server targets.

### Real examples

```
# Where does example.com point? Ask three resolvers at once.
example.com A AAAA @cloudflare @google @quad9

# Is my mail configured correctly?
example.com MX TXT @cloudflare

# Check over DNS-over-TLS
example.com A @google +tls

# Full domain health check (routes to /api/check)
example.com +check

# Walk the delegation chain
example.com A +trace

# Compare answers across all transports
example.com A +compare

# Authoritative vs recursive comparison
example.com MX +auth

# DNSSEC validation
example.com DNSKEY DS @cloudflare +dnssec

# Reverse DNS
192.0.2.1 PTR @system

# Custom resolver (when arbitrary servers are enabled)
example.com A @8.8.8.8
```

---

## Modes

### Query mode

Fan-out queries to multiple resolvers. Results stream in per record type — you don't wait for everything to finish before you see the first answer.

The results table shows each resolver's answer side by side. Mismatches stand out.

```sh
curl -sN -X POST http://localhost:8080/api/query \
  -H 'Content-Type: application/json' \
  -d '{"domain": "example.com", "record_types": ["MX", "TXT"], "servers": ["cloudflare", "google"]}'
```

### Check mode

One command, full domain audit. prism queries A, AAAA, MX, TXT, NS, SOA, CAA, CNAME, DNSKEY, DS, HTTPS, SVCB, SRV, SSHFP, TLSA, and synthesizes a DMARC check. Lint results stream alongside the raw DNS data.

Catches things like:
- SPF records that resolve to too many IPs (permerror waiting to happen)
- DMARC `p=none` with no `rua` reporting address
- Missing CAA records on a zone with subdomains
- DNSSEC `DS` record without a matching `DNSKEY`
- Duplicate or conflicting `CNAME` + other-record collisions

```sh
curl -sN -X POST http://localhost:8080/api/check \
  -H 'Content-Type: application/json' \
  -d '{"domain": "example.com", "servers": ["cloudflare"]}'
```

### Trace mode

An iterative delegation walk — no recursion, just raw queries following the referral chain yourself. Starts at a root server, follows NS referrals to the TLD, then to your authoritative server.

Useful for:
- Diagnosing split-brain DNS
- Finding stale glue records
- Seeing exactly where your delegation breaks after a nameserver change
- Understanding why DNSSEC validation fails midchain

```sh
curl -sN -X POST http://localhost:8080/api/trace \
  -H 'Content-Type: application/json' \
  -d '{"domain": "example.com", "record_type": "A"}'
```

### Compare mode

Query the same domain over UDP, TCP, DNS-over-TLS, and DNS-over-HTTPS simultaneously. Results are grouped by transport so you can spot differences at a glance.

Useful for:
- Detecting ISP or corporate firewall DNS manipulation
- Finding middleboxes that intercept or modify DNS responses
- Comparing latency and reliability across transports
- Verifying that encrypted DNS returns the same answers as plain UDP

```sh
curl -sN -X POST http://localhost:8080/api/compare \
  -H 'Content-Type: application/json' \
  -d '{"domain": "example.com", "record_types": ["A", "AAAA"], "servers": ["cloudflare"]}'
```

### Auth mode

Discover the domain's authoritative nameservers via NS lookup, then query them directly (RD=0) alongside recursive resolvers. A two-column comparison shows where authoritative and recursive answers diverge.

Useful for:
- Detecting stale cached records vs fresh authoritative data
- Finding NXDOMAIN hijacking by recursive resolvers
- Identifying split-horizon DNS configurations
- Verifying propagation after DNS changes

```sh
curl -sN -X POST http://localhost:8080/api/authcompare \
  -H 'Content-Type: application/json' \
  -d '{"domain": "example.com", "record_types": ["A", "MX"], "servers": ["cloudflare"]}'
```

---

## API reference

All endpoints accept `application/json` and return `text/event-stream` (Server-Sent Events).

### `POST /api/query`

```json
{
  "domain": "example.com",
  "record_types": ["A", "MX"],
  "servers": ["cloudflare", "google"],
  "transport": "udp",
  "dnssec": false
}
```

Emits `batch` events (one per record type per resolver group) and a final `done` event.

```
event: batch
data: {"request_id":"...","record_type":"MX","lookups":[...],"completed":1,"total":2}

event: done
data: {"request_id":"...","total_queries":4,"duration_ms":312,"warnings":[],"transport":"udp","dnssec":false}
```

### `GET /api/query?q=...`

Same as POST but the query string goes in `?q=example.com+MX+%40cloudflare`. Handy for sharing links.

### `POST /api/check`

```json
{
  "domain": "example.com",
  "servers": ["cloudflare"],
  "timeout_secs": 5
}
```

Emits `batch` events (raw DNS data) and `lint` events (check results), then `done`.

### `POST /api/trace`

```json
{
  "domain": "example.com",
  "record_type": "A",
  "timeout_secs": 10
}
```

Emits `hop` events (one per delegation level), then `done`.

```
event: hop
data: {"request_id":"...","hop":{"level":1,"nameservers":["a.root-servers.net."],"referrals":["com."],"records":[],"latency_ms":18}}
```

### `POST /api/compare`

```json
{
  "domain": "example.com",
  "record_types": ["A", "AAAA"],
  "servers": ["cloudflare"]
}
```

Emits `batch` events with a `transport` field (`"udp"`, `"tcp"`, `"tls"`, `"https"`), then `done` with a `transports` array listing which transports were queried. Transports where the provider has no resolvers are skipped.

### `POST /api/authcompare`

```json
{
  "domain": "example.com",
  "record_types": ["A", "MX"],
  "servers": ["cloudflare"]
}
```

Emits `batch` events with a `source` field (`"authoritative"` or `"recursive"`), then `done` with an `auth_servers` array listing the discovered authoritative nameservers.

### Metadata

| Endpoint | Description |
|----------|-------------|
| `GET /api/health` | Liveness probe |
| `GET /api/servers` | List predefined resolvers and their IPs |
| `GET /api/record-types` | List queryable record types |
| `POST /api/parse` | Tokenize a query string, return completions |

### Interactive API reference

- `GET /docs` — Scalar API reference UI (browsable, try-it-out)
- `GET /api-docs/openapi.json` — OpenAPI 3.1 schema

---

## Configuration

Config is loaded in this order (highest wins): CLI argument / `PRISM_CONFIG` env var → TOML file → built-in defaults.

```sh
cp prism.example.toml prism.toml
./prism prism.toml
# or
PRISM_CONFIG=prism.toml ./prism
```

Environment variables override the TOML file. Use `PRISM_` prefix and `__` as the section separator:

```sh
PRISM_SERVER__BIND=0.0.0.0:8080
PRISM_DNS__ALLOW_ARBITRARY_SERVERS=true
PRISM_LIMITS__PER_IP_PER_MINUTE=200
```

### Key settings

```toml
[server]
bind = "127.0.0.1:8080"          # API + frontend
metrics_bind = "127.0.0.1:9090"  # Prometheus metrics
trusted_proxies = []              # CIDR ranges of trusted reverse proxies

[limits]
per_ip_per_minute = 120   # GCRA tokens per client IP per minute
per_ip_burst = 40         # max burst (must cover max_record_types × max_servers)
global_per_minute = 1000  # total tokens across all clients
max_record_types = 10     # per-query cap
max_servers = 4           # per-query cap
max_timeout_secs = 10     # hard cap; clients can request less

[trace]
max_hops = 10             # delegation levels before giving up (hard cap: 20)
query_timeout_secs = 3    # per-hop DNS timeout

[dns]
default_servers = ["cloudflare"]  # used when no @server in query
allow_system_resolvers = true     # permit @system
allow_arbitrary_servers = false   # permit @1.2.3.4 custom IPs

[circuit_breaker]
failure_threshold = 0.5   # trip when 50% of requests fail
window_secs = 60
cooldown_secs = 30
min_requests = 5
```

---

## Security

prism is designed to be exposed to the internet. Security is load-bearing, not optional.

**4-layer defense-in-depth:**

1. **Query restrictions** — `ANY`, `AXFR`, and `IXFR` are blocked outright. RFC 1918, loopback, link-local, CGNAT, and multicast addresses are rejected as resolver targets. Domains over 253 characters are rejected.

2. **GCRA rate limiting** — Three independent limiters: per client IP (120 tokens/min, burst 40), per DNS target (60 tokens/min, burst 20), and global (1000 tokens/min). Query cost = `record_types × servers`. Compare mode multiplies by 4 (one per transport). Auth mode adds a flat 16 for NS discovery.

3. **IP extraction** — Client IP is extracted from proxy headers only when the request arrives from a configured trusted proxy. Direct connections always use the peer address.

4. **Security headers** — CSP, `X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`, HSTS. CORS is same-origin only.

There is no server-side DNS caching — every request hits the resolver fresh. That's intentional: prism is a debugging tool, not a caching proxy.

---

## Deployment

prism is a single binary with the frontend baked in. Copy it to your server, drop a config file next to it, and run it behind a reverse proxy.

### Behind nginx

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

Configure `trusted_proxies` so prism extracts the real client IP:

```toml
[server]
trusted_proxies = ["127.0.0.1/32"]
```

### Behind Caddy

```
dns.example.com {
    reverse_proxy 127.0.0.1:8080
}
```

Caddy handles TLS automatically. Set `trusted_proxies` the same way.

### Security hardening checklist

- Set `allow_arbitrary_servers = false` (default) to block custom resolver IPs
- Set `allow_system_resolvers = false` if you don't want `/etc/resolv.conf` exposed
- Configure `trusted_proxies` to match your reverse proxy's IP range
- Keep the metrics port (`:9090`) on loopback — never expose it publicly
- Review rate limits for your expected traffic (`per_ip_per_minute`, `global_per_minute`)

### Metrics

prism exposes Prometheus metrics on the metrics bind address (default `127.0.0.1:9090`).

```yaml
# prometheus.yml
scrape_configs:
  - job_name: prism
    static_configs:
      - targets: ['127.0.0.1:9090']
```

Keep the metrics endpoint on loopback or behind a firewall — it is not authenticated.

---

## Development

### Prerequisites

- Rust toolchain (stable)
- Node.js (for the frontend)

### Build targets

```sh
make              # full production build
make dev          # cargo run with prism.dev.toml (server on :8080)
make frontend-dev # Vite dev server on :5173, proxies /api/* to :8080
make test         # cargo test
make clippy       # cargo clippy -- -D warnings
make lint         # clippy + fmt-check
make ci           # lint + test + frontend + build (full pipeline)
make clean        # remove target/, frontend/dist/, node_modules/
```

For active frontend development, run both in separate terminals:

```sh
# Terminal 1
make dev

# Terminal 2
make frontend-dev
# open http://localhost:5173
```

### Architecture

```
src/
  main.rs              # Axum server, graceful shutdown, metrics on :9090
  config.rs            # Layered config (TOML + env vars)
  error.rs             # ApiError → HTTP status + error codes
  parser.rs            # Query language parser (source of truth)
  record_format.rs     # Human-readable formatting (TXT, CAA, MX, SOA)
  dns_trace.rs         # Iterative delegation walker
  circuit_breaker.rs   # Per-provider sliding-window circuit breaker
  api/
    query.rs           # FuturesUnordered fan-out, 30s stream deadline
    check.rs           # 15-type lint sweep
    trace.rs           # Delegation walk
    compare.rs         # Transport comparison (UDP/TCP/TLS/HTTPS)
    authcompare.rs     # Auth vs recursive (NS discovery + RD=0)
    parse.rs           # Completion hints
    meta.rs            # Health, servers, record-types
  security/
    rate_limit.rs      # 3-tier GCRA
    ip_extract.rs      # Proxy-aware client IP
    query_policy.rs    # Target validation, type restrictions, limits

frontend/src/
  App.tsx              # State, history (localStorage), theme, SSE parsing
  components/
    QueryInput.tsx     # CodeMirror 6 editor with autocomplete
    ResultsTable.tsx   # Streaming results
    LintTab.tsx        # Check mode lint results
    TraceView.tsx      # Delegation hop visualization
    TransportComparison.tsx  # Transport comparison view
    AuthComparison.tsx       # Auth vs recursive view
  lib/tokenizer.ts     # Syntax highlighting (cosmetic, not semantic)
  styles/global.css    # Plain CSS, custom properties, no framework
```

The Rust parser is the **single source of truth** for query semantics. The TypeScript tokenizer is cosmetic — it drives syntax highlighting colors, not behavior.

---

## Stack

| Layer | Technology |
|-------|-----------|
| Backend | Rust, [axum](https://github.com/tokio-rs/axum) 0.8 |
| DNS | [mhost](https://github.com/lukaspustina/mhost) (crates.io) |
| Streaming | Server-Sent Events via `tokio::sync::mpsc` + `ReceiverStream` |
| Rate limiting | [tower-governor](https://github.com/benwis/tower-governor) (GCRA) |
| Assets | [rust-embed](https://github.com/pyros2097/rust-embed) (SPA baked into binary) |
| Metrics | `metrics` + `metrics-exporter-prometheus` |
| Frontend | [SolidJS](https://solidjs.com) + TypeScript (strict mode) |
| Editor | [CodeMirror 6](https://codemirror.net) |
| Build | [Vite](https://vitejs.dev) |

The entire frontend is baked into the binary at build time. Deploying prism is a single file copy.

---

## License

MIT OR Apache-2.0
