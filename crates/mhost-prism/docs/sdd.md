# Software Design Document: `prism`

**Feature**: Web-based DNS debugging service powered by mhost-lib
**Status**: Proposal
**Date**: 2026-02-17
**Roadmap**: To be added to `ROADMAP.md` upon acceptance of this SDD.

---

## 1. Motivation

DNS debugging today means choosing between two extremes: CLI tools (`dig`, `mhost`) that require terminal access and syntax knowledge, or web tools (`dns.google`, `mxtoolbox.com`) that are limited to one server and one record type at a time. Nobody offers the combination of **multi-server fan-out**, **streaming results**, and **an accessible web interface** in a single tool.

mhost already solves the hard problems — concurrent multi-resolver lookups, typed record parsing, DNSSEC chain validation, health-check lints, delegation tracing. It exposes all of this through a library with full `Serialize` support. A web frontend is the natural third surface (after CLI and TUI) that makes these capabilities accessible to anyone with a browser.

**Target users:**
- DevOps engineers debugging DNS propagation from machines without mhost installed
- SREs sharing DNS diagnostic results via URL during incidents
- Developers verifying DNS configuration without learning `dig` syntax
- Teams collaborating on DNS troubleshooting through shareable query links

## 2. Core Concept

A single Rust binary that serves an embedded SPA and exposes mhost-lib capabilities through an HTTP API. Queries are submitted via a `dig`-inspired mini-language with autocomplete. Results stream back progressively via Server-Sent Events as each per-record-type query completes.

**Streaming granularity**: mhost-lib's `ResolverGroup::lookup()` collects all results for a given `MultiQuery` before returning — there is no per-lookup streaming API. To achieve progressive delivery, prism follows the same pattern as mdive (the TUI): it issues **separate queries per record type** via `FuturesUnordered` and streams each completed batch as an SSE event. Streaming granularity is per-query-completion, not per-individual-lookup. For a query with 4 record types across 2 servers, this means ~4 SSE batches (one per record type, each containing results from all servers for that type), not 8 individual events.

```
Browser                     prism binary
  |                              |
  |  GET /                       |
  |----------------------------->|  Serve embedded SPA (rust-embed)
  |<-----------------------------|
  |                              |
  |  GET /api/query?q=...        |
  |  (or POST with JSON body)   |
  |----------------------------->|  Parse, validate, build ResolverGroup
  |                              |  Spawn per-record-type queries
  |  SSE: event: batch           |  (FuturesUnordered)
  |<-----------------------------|  Stream each completed query batch
  |  SSE: event: batch           |
  |<-----------------------------|
  |  SSE: event: done            |
  |<-----------------------------|  Signal completion with stats
  |                              |
```

## 3. Placement: Separate Crate

`prism` is a new workspace member, not a feature flag in the existing mhost crate. Rationale:

- **Different dependency profile.** prism pulls in `axum`, `tower-http`, `tower-governor`, `rust-embed`, and frontend build artifacts. These have no business in the mhost CLI or library.
- **Independent release cadence.** The web service can iterate on UI and API changes without cutting a new mhost release.
- **Clean library dependency.** `prism` depends on `mhost` as a library (no `app` feature) plus a thin layer of shared types. It does not inherit CLI parsing, terminal formatting, or TUI code. **Note**: The lint functions (`check_spf`, `check_mx_sync`, `check_caa`, etc.) and trace/delegation logic currently live in `src/app/modules/check/lints/` and `src/app/modules/trace/`, gated behind the `app` feature. Phase 2 (check + trace endpoints) is blocked on extracting this logic into the library layer — see section 14 for scoping.
- **Precedent.** This mirrors how many Rust projects separate their library from their server binary (e.g., `tantivy` vs. `quickwit`).

### 3.1 Workspace Conversion

mhost is currently a single crate, not a Cargo workspace. Converting to a workspace is a prerequisite for adding prism. This involves:

1. **Root `Cargo.toml` restructure**: Add `[workspace]` section with `members = [".", "prism"]`. The existing `[package]` stays in the same file (Cargo supports `[workspace]` + `[package]` in the root when the root is itself a member via `"."`).
2. **Shared workspace settings**: Extract common `[workspace.dependencies]` for crates used by both members (tokio, serde, tracing). This avoids version drift.
3. **CI updates**: `cargo test` at the workspace root runs tests for all members. The release workflow must build `--bin mhost` and `--bin mdive` explicitly (or via `-p mhost`), since `cargo build --release` in a workspace builds all members.
4. **Feature flag verification**: Ensure `feature = "app"` and `feature = "tui"` still gate correctly when mhost is a workspace member. The `build.rs` (shell completions) runs only for the mhost package.

This is a low-risk migration — the mhost crate's public API and binary outputs are unchanged. But it must be completed before any prism development begins. See §16 risk #1.

### 3.2 Directory Layout

```
mhost/                            # workspace root (existing repo)
  Cargo.toml                      # [workspace] + [package] for mhost
  src/lib.rs                      # library: resolver, nameserver, resources, ...
  prism/                      # new workspace member
    Cargo.toml                    # depends on mhost = { path = ".." }
    src/
      main.rs                     # entry point, axum server setup
      api/
        mod.rs                    # route definitions
        query.rs                  # GET/POST /api/query -> SSE stream
        check.rs                  # POST /api/check -> SSE stream
        trace.rs                  # POST /api/trace -> SSE stream
        parse.rs                  # POST /api/parse -> completion hints
        meta.rs                   # GET /api/servers, GET /api/record-types
      security/
        mod.rs                    # middleware composition
        rate_limit.rs             # tower-governor layers
        ip_extract.rs             # real client IP from proxy headers
        query_policy.rs           # target validation, type restrictions
      config.rs                   # server configuration (bind addr, limits, etc.)
    frontend/                     # SolidJS + Vite project
      src/
        App.tsx
        components/
          QueryInput.tsx          # CodeMirror 6 single-line input
          ResultsTable.tsx        # streaming results table
          ServerComparison.tsx    # multi-server diff view
        lib/
          tokenizer.ts            # syntax highlighting tokenizer (cosmetic only, see §13.7)
      dist/                       # build output, .gitignored, embedded via rust-embed
```

## 4. Query Language

### 4.1 Design Principles

1. **Progressive disclosure**: The simplest query is just a domain name. Everything else is optional.
2. **Familiar to `dig` users**: `@` for servers, `+` for flags — the de facto standard.
3. **Aligned with mhost**: Predefined provider names, record types, and protocol names match the existing CLI.
4. **Position-independent**: After the domain, tokens can appear in any order.
5. **Error-tolerant**: Unknown tokens produce warnings, not hard errors. Valid parts of a query always execute. **Exception**: An invalid domain name (the first token) is a hard error — there is nothing to execute without a valid target. Error tolerance applies to optional tokens (record types, servers, flags), not the required domain.
6. **Normalized**: Domain names are lowercased by the parser (DNS is case-insensitive per RFC 4343). This ensures `Example.COM` and `example.com` are the same query, the same rate-limit key, and the same cache key. Provider names and record type names are also case-insensitive.

### 4.2 Syntax

```
query       ::= domain (WS token)*
token       ::= record_type | server | flag
domain      ::= <valid DNS name or IP address>
                -- The first token is ALWAYS parsed as a domain, even if it matches
                -- a record type name (e.g., "MX" is a valid domain name).
                -- Record type tokens are only recognized in non-first position.
record_type ::= "A" | "AAAA" | "MX" | "TXT" | "NS" | "SOA" | "CNAME" | "CAA"
              | "SRV" | "PTR" | "HTTPS" | "SVCB" | "SSHFP" | "TLSA" | "NAPTR"
              | "HINFO" | "OPENPGPKEY" | "DNSKEY" | "DS" | "ALL"
server      ::= "@" (provider_name | "system" | ip_addr | ip_addr ":" port)
provider    ::= <any name in PredefinedProvider::all()>
                -- Currently: "cloudflare", "google", "quad9", "mullvad",
                -- "wikimedia", "dns4eu". This list is derived from the library
                -- at startup, not hardcoded in the parser — adding a provider
                -- to PredefinedProvider in mhost-lib automatically makes it
                -- available in the web query language.
                -- Provider names are case-insensitive ("Cloudflare" = "cloudflare").
                -- Maps to PredefinedProvider variants via case-insensitive FromStr.
                -- "system" is a special case (host's /etc/resolv.conf),
                -- not a PredefinedProvider. See §9 for allow_system_resolvers.
flag        ::= "+" flag_name
flag_name   ::= "udp" | "tcp" | "tls" | "https"           -- transport
              | "dnssec" | "trace" | "check"               -- mode/options
```

### 4.3 Examples

```
example.com                          # defaults: A AAAA CNAME MX with default servers
example.com MX TXT                   # multi-type
example.com A @8.8.8.8               # specific server
example.com AAAA @cloudflare         # predefined provider
example.com A @google @quad9         # multi-server comparison
example.com MX +tls                  # DNS-over-TLS transport
example.com A +dnssec               # DNSSEC chain verification mode
example.com +check                   # health check mode
example.com +trace                   # delegation trace mode
example.com A AAAA @cloudflare @google +tls +dnssec
```

### 4.4 Defaults and Special Cases

| Input | Expansion | Rationale |
|-------|-----------|-----------|
| `example.com` (bare domain) | `A AAAA CNAME MX` with configured default servers | Matches mhost CLI defaults |
| `example.com ALL` | Rejected with 422 `TOO_MANY_RECORD_TYPES` | `ALL` expands to ~27 types (all standard types minus blocked), which exceeds `max_record_types` (10). The error message explains the limit and suggests specifying types explicitly. `ALL` is not a `RecordType` variant — the parser expands it before validation. |
| `192.0.2.1` (IP address) | `PTR` query with in-addr.arpa | Auto-detect reverse lookup |
| `+trace` | Overrides to trace mode (Phase 2) | Switches backend to trace subcommand. Returns 422 `FEATURE_NOT_AVAILABLE` until implemented. |
| `+check` | Overrides to check mode (Phase 2) | Switches backend to check subcommand. Returns 422 `FEATURE_NOT_AVAILABLE` until implemented. |
| No `@server` specified | Configured `default_servers` (see §9) | Configurable per deployment |

### 4.5 Blocked Query Types

These are blocked unconditionally for security (see section 8):

| Type | Reason |
|------|--------|
| ANY | DNS amplification vector (RFC 8482) |
| AXFR / IXFR | Zone transfer abuse |

## 5. API Design

### 5.1 Query Endpoint

Two methods serve different use cases:

**GET — Raw query string** (used by the frontend, shareable URLs, and curl):

```
GET /api/query?q=example.com+MX+AAAA+@cloudflare+%2Btls
Accept: text/event-stream
```

The Rust query language parser is the single source of truth. The `q` parameter is parsed server-side into execution parameters. GET enables the browser's native `EventSource` API (no custom SSE client library needed), HTTP caching, direct bookmarking, and simple `curl` usage.

**POST — Structured JSON** (for programmatic clients):

```
POST /api/query
Content-Type: application/json
Accept: text/event-stream

{
  "domain": "example.com",
  "record_types": ["A", "AAAA", "MX"],
  "servers": [
    {"predefined": "cloudflare"},
    {"ip": "8.8.8.8", "port": 53}
  ],
  "transport": "tls",
  "timeout_secs": 5,
  "dnssec": false
}
```

POST with `?q=` is rejected with 400 (`AMBIGUOUS_INPUT`) to prevent silent precedence confusion.

**Response**: SSE stream with typed events. Each `batch` event contains the complete results for one record type across all requested servers (a `Lookups` from mhost-lib, serialized via its existing `Serialize` impl). Batches arrive progressively as each per-record-type query completes:

```
event: batch
data: {"request_id":"019...","lookups":[{"query":{"name":"example.com","type":"A"},"server":"1.1.1.1:853","result":{"Response":{"records":[...],"response_time_ms":12}}},{"query":{"name":"example.com","type":"A"},"server":"8.8.8.8:53","result":{"Response":{"records":[...],"response_time_ms":18}}}],"completed":1,"total":3}

event: batch
data: {"request_id":"019...","lookups":[{"query":{"name":"example.com","type":"MX"},"server":"1.1.1.1:853","result":{"Response":{"records":[...],"response_time_ms":15}}},{"query":{"name":"example.com","type":"MX"},"server":"8.8.8.8:53","result":{"Response":{"records":[...],"response_time_ms":20}}}],"completed":2,"total":3}

event: done
data: {"request_id":"019...","total_queries":6,"responses":3,"errors":0,"duration_ms":234}
```

`total_queries` is the total DNS lookups issued (3 record types x 2 servers = 6). `responses` is the number of completed batches (3, one per record type). `errors` counts batch-level failures.

The frontend accumulates batches progressively, populating each record-type section as its batch arrives.

### 5.2 Check Endpoint

```
POST /api/check
Content-Type: application/json
Accept: text/event-stream

{
  "domain": "example.com",
  "servers": [{"predefined": "system"}],
  "timeout_secs": 5
}
```

**Response**: SSE stream. First emits `batch` events (the underlying DNS query results), then emits `lint` events with check results:

```
event: batch
data: { ... }

event: lint
data: {"category":"SPF","status":"warning","message":"Multiple SPF records found","records":["v=spf1 ...","v=spf1 ..."]}

event: done
data: {"checks":9,"passed":7,"warnings":1,"failed":1}
```

### 5.3 Trace Endpoint

```
POST /api/trace
Content-Type: application/json
Accept: text/event-stream

{
  "domain": "example.com",
  "timeout_secs": 10
}
```

**Response**: SSE stream of delegation hops from root to authoritative:

```
event: hop
data: {"depth":0,"server":"198.41.0.4","server_name":"a.root-servers.net","referral":"com.","response_time_ms":8}

event: hop
data: {"depth":1,"server":"192.5.6.30","server_name":"a.gtld-servers.net","referral":"example.com.","response_time_ms":12}

event: hop
data: {"depth":2,"server":"93.184.216.34","server_name":"ns1.example.com","answer":["93.184.216.34"],"response_time_ms":5}

event: done
data: {"hops":3,"total_ms":25}
```

### 5.4 Parse Endpoint (for autocomplete)

```
POST /api/parse
Content-Type: application/json

{
  "input": "example.com MX @cl",
  "cursor_pos": 18
}
```

**Response**: Parsed tokens and context-aware completions:

```json
{
  "tokens": [
    {"kind": "domain", "value": "example.com", "from": 0, "to": 11},
    {"kind": "record_type", "value": "MX", "from": 12, "to": 14},
    {"kind": "server_partial", "value": "@cl", "from": 15, "to": 18}
  ],
  "completions": [
    {"label": "@cloudflare", "detail": "1.1.1.1 / 1.0.0.1", "category": "server"}
  ]
}
```

This endpoint powers server-side autocomplete. The parser runs in Rust, ensuring the frontend and backend agree on syntax. The frontend can also run a local TypeScript tokenizer for instant feedback, with server-side parse as the source of truth.

### 5.5 Metadata Endpoints

```
GET /api/servers          # list predefined server providers
GET /api/record-types     # list supported record types with descriptions
GET /api/health           # service health check
```

These are static data derived from mhost-lib's `PredefinedProvider::all()` and `RecordType::all()`. Served with aggressive caching headers.

### 5.6 Error Responses

All non-SSE error responses use a consistent JSON format:

```json
{
  "error": {
    "code": "INVALID_DOMAIN",
    "message": "Domain name exceeds 253 characters",
    "details": null
  }
}
```

| HTTP Status | Code | When |
|-------------|------|------|
| 400 | `INVALID_DOMAIN`, `INVALID_RECORD_TYPE`, `INVALID_SERVER` | Malformed input |
| 400 | `PARSE_ERROR` | Raw query string (`?q=`) could not be parsed |
| 400 | `AMBIGUOUS_INPUT` | POST with both `?q=` and JSON body |
| 422 | `BLOCKED_QUERY_TYPE`, `BLOCKED_TARGET_IP`, `SYSTEM_RESOLVERS_DISABLED` | Valid input but policy-rejected |
| 422 | `FEATURE_NOT_AVAILABLE` | Recognized but unimplemented flag (e.g., `+trace` in Phase 1) |
| 422 | `TOO_MANY_RECORD_TYPES`, `TOO_MANY_SERVERS` | Exceeds query limits |
| 429 | `RATE_LIMITED` | Rate limit exceeded (includes `Retry-After` header) |
| 500 | `RESOLVER_ERROR` | Failed to build resolver group |

For SSE streams, mid-stream errors are sent as `event: error` events:

```
event: error
data: {"code":"QUERY_TIMEOUT","message":"Query for AAAA timed out after 10s","query_type":"AAAA"}
```

The `done` event is always sent after the last batch or error, so the frontend can reliably detect stream completion.

## 6. Frontend

### 6.1 Technology Choice: SolidJS + Vite

| Criterion | SolidJS | React | Vanilla JS |
|-----------|---------|-------|------------|
| Runtime size | ~7KB gzipped | ~40KB gzipped | 0KB |
| Reactive SSE streams | First-class signals | Requires useEffect + state | Manual DOM updates |
| CodeMirror 6 integration | Works (DOM-level) | Works (react-codemirror) | Works (native API) |
| Build tooling | Vite (fast, simple) | Vite or webpack | None (but no TS, no modules) |
| Component model | Yes (JSX) | Yes (JSX) | No (template strings) |
| Learning curve for contributors | Moderate (React-like JSX) | Low (widely known) | Low (but scales poorly) |

**Recommendation: SolidJS.** It combines the component model and JSX familiarity of React with fine-grained reactivity that is ideal for streaming data. When an SSE event arrives, only the affected table row re-renders — no diffing, no virtual DOM. The 7KB runtime keeps the total frontend bundle small (estimated ~60KB gzipped with CodeMirror 6).

Vanilla JS was considered but rejected: the autocomplete input with token highlighting, progressive table population, and multi-tab results layout would require reimplementing a component system. The build step cost is minimal with Vite.

### 6.2 Query Input

A single-line CodeMirror 6 editor configured as a search bar:

- **Syntax highlighting**: Tokens colored by type — domain (neutral), record types (blue), servers (green), flags (orange), errors (red underline)
- **Autocomplete**: Context-aware dropdown triggered by `@`, `+`, or after whitespace following the domain. Grouped into categories: Record Types, Servers, Transport, Options. Each item includes a short description ("MX — Mail exchange record"). Phase 1 uses a static completion list embedded in the frontend. Phase 3 adds server-side completions via `/api/parse` — the frontend shows static completions immediately and replaces them with server completions when they arrive, with graceful fallback to the static list if `/api/parse` is slow or fails.
- **Placeholder**: `example.com A AAAA @google +tls`
- **Submit**: Enter key or button. Submitting a new query while results are still streaming closes the previous `EventSource` connection before opening a new one (analogous to mdive's `JoinHandle::abort()` pattern).
- **History**: Up/Down arrows cycle previous queries (localStorage)
- **URL sync**: Query state reflected in URL query params for shareability (`?q=example.com+MX+@google`)

CodeMirror 6 is chosen over Monaco (too heavy at ~1MB for a single-line input) and over a custom `<div contenteditable>` with `<span>` coloring. The contenteditable approach would save ~38KB gzipped but loses IME handling, accessibility (ARIA), structured cursor movement, and undo/redo integration — edge cases that CodeMirror handles correctly and that would need to be reimplemented. The 40KB cost is acceptable given that CodeMirror is the only substantial frontend dependency.

### 6.3 Results Display

**Layout:**

```
┌──────────────────────────────────────────────────────────────┐
│  [ Query input ............... ]                    [Query]  │
│  Parsed: domain:example.com  types:A,AAAA  server:Cloudflare│
├──────────────────────────────────────────────────────────────┤
│  [Results]  [Servers]  [JSON]                                │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  ▼ A Records (2)                          4/6 servers done   │
│  ┌──────────┬────────┬─────────────┬──────────────────────┐  │
│  │ Name     │ TTL    │ Value       │ Servers              │  │
│  ├──────────┼────────┼─────────────┼──────────────────────┤  │
│  │ example… │ 300    │ 93.184.216… │ ● 3/3 agree          │  │
│  └──────────┴────────┴─────────────┴──────────────────────┘  │
│                                                              │
│  ▼ MX Records (1)                                            │
│  ┌──────────┬────────┬─────────────────────┬──────────────┐  │
│  │ Name     │ TTL    │ Value               │ Servers      │  │
│  ├──────────┼────────┼─────────────────────┼──────────────┤  │
│  │ example… │ 300    │ 10 mail.example.com │ ● 3/3 agree  │  │
│  └──────────┴────────┴─────────────────────┴──────────────┘  │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│  ✓ 6 queries  ● 5 OK  ○ 1 timeout  ⏱ 234ms                │
└──────────────────────────────────────────────────────────────┘
```

Key elements:

- **Grouped by record type** with collapsible sections, color-coded type badges (reusing mhost's `record_type_color` palette)
- **Progressive population**: Rows appear as SSE events arrive. Skeleton placeholders for pending types.
- **Server agreement column**: Shows "N/M agree" for each unique record value. Divergence highlighted in yellow/red.
- **Expandable row detail**: Click a row to see full rdata fields, responding servers with latency, DNSSEC status, and human-readable interpretation (SPF mechanism breakdown, DMARC policy, etc.)
- **Tabs**: Results (default), Servers (per-server comparison view), JSON (raw serialized output)
- **Status bar**: Live query progress, completion stats
- **Connection error banner**: If the SSE connection fails (CORS preflight rejection, network error, non-2xx response), a prominent error banner replaces the results area with the error details and a retry button. SSE connections that fail CORS preflight produce no browser error message — the frontend must detect the failed `fetch()` and render the error explicitly.

### 6.4 Visual Design

- **Dark mode by default** (with system-preference detection and manual toggle). Developer debugging tool aesthetic.
- **Monospaced font** for all DNS data (domain names, IPs, record values). Proportional font for UI chrome.
- **Color palette**: Inherit from mhost's existing `record_type_color` (`app/common/styles.rs`) constants, adapted to CSS custom properties.
- **Mobile**: Responsive layout — stacked cards instead of wide tables on narrow screens. Query input full-width. Not optimized for mobile but not broken.

### 6.5 Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `/` | Focus query input |
| `Enter` | Submit query (when input focused) |
| `Tab` | Accept top autocomplete suggestion |
| `Escape` | Dismiss autocomplete / clear focus |
| `j` / `k` | Navigate result rows (when input not focused) |
| `Enter` | Expand/collapse selected row |
| `?` | Show keyboard shortcut help |

## 7. Backend Architecture

### 7.1 Server Setup

```rust
// main.rs (simplified)
#[tokio::main]
async fn main() {
    let config = Config::from_env_and_args();

    // Health endpoint is outside rate limiting (used by load balancers)
    let health = Router::new()
        .route("/api/health", get(api::meta::health));

    // API routes with rate limiting
    let api = Router::new()
        .route("/api/query", get(api::query::get_handler).post(api::query::post_handler))
        .route("/api/check", post(api::check::handler))
        .route("/api/trace", post(api::trace::handler))
        .route("/api/parse", post(api::parse::handler))
        .route("/api/servers", get(api::meta::servers))
        .route("/api/record-types", get(api::meta::record_types))
        .layer(security::rate_limit_layer(&config));

    let app = Router::new()
        .merge(health)
        .merge(api)
        // Global middleware (applied to all routes)
        .layer(tower_http::limit::RequestBodyLimitLayer::new(8 * 1024)) // 8KB
        .layer(security::cors_layer(&config))
        .layer(security::security_headers_layer())
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(tower::limit::ConcurrencyLimitLayer::new(
            config.max_concurrent_connections, // default: 256
        ))
        // SPA fallback (must be last)
        .fallback(static_handler);

    // Metrics on separate port (localhost-only by default)
    let metrics_listener = tokio::net::TcpListener::bind(&config.metrics_bind).await.unwrap();
    let metrics_app = Router::new().route("/metrics", get(metrics_handler));
    tokio::spawn(axum::serve(metrics_listener, metrics_app).into_future());

    // Main server
    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await.unwrap();
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(SignalKind::TERMINATE).unwrap();
        tokio::select! {
            _ = ctrl_c => {},
            _ = sigterm.recv() => {},
        }
    }
    #[cfg(not(unix))]
    ctrl_c.await.unwrap();
    tracing::info!("shutdown signal received, draining connections (10s timeout)");
    // axum's graceful shutdown waits for in-flight connections to complete.
    // SSE streams are technically always "in-flight" — without a drain timeout,
    // shutdown hangs until all SSE clients disconnect. The 30-second stream
    // timeout (§8.1) bounds the worst case, but we also set a 10-second drain
    // timeout via tokio::time::timeout around the serve future (not shown here
    // for brevity). After the drain timeout, remaining connections are dropped.
}
```

### 7.2 Query Handler Pattern

Each API endpoint follows the same pattern: validate input, build mhost-lib objects, spawn **per-record-type queries** into a `FuturesUnordered`, and stream each completed batch through an mpsc channel returned as an SSE stream. This mirrors the mdive TUI's proven approach (`src/bin/mdive/dns.rs`).

**Why per-record-type queries**: `ResolverGroup::lookup(query).await` collects all results for a `MultiQuery` before returning. There is no `Stream`-returning API in the library. To achieve progressive delivery, we issue one `MultiQuery::single()` per record type and await them concurrently via `FuturesUnordered`, streaming each batch as it completes.

```rust
// api/query.rs (simplified)
async fn handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<QueryRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    // 1. Validate
    let policy = QueryPolicy::default();
    policy.validate(&req)?;  // checks target IPs, record types, limits

    // 2. Build resolver (shared across all per-type queries)
    let configs = resolve_server_specs(&req.servers)?;
    let group = Arc::new(ResolverGroupBuilder::new()
        .nameservers(configs)
        .timeout(Duration::from_secs(req.timeout_secs.min(10)))
        .build()
        .await
        .map_err(ApiError::resolver)?);

    // 3. Build per-record-type queries
    let queries: Vec<MultiQuery> = req.record_types.iter()
        .map(|rt| MultiQuery::single(&req.domain, *rt))
        .collect::<Result<_, _>>()?;
    let total = queries.len();

    // 4. Stream results as each per-type query completes
    let (tx, rx) = mpsc::channel(64);
    let server_count = req.servers.len().max(1); // default server = 1
    tokio::spawn(async move {
        let start = tokio::time::Instant::now();
        let mut futs: FuturesUnordered<_> = queries.into_iter()
            .map(|q| {
                let g = Arc::clone(&group);
                async move { g.lookup(q).await }
            })
            .collect();

        let mut completed = 0u32;
        let mut errors = 0u32;
        let stream_deadline = start + Duration::from_secs(30);
        loop {
            tokio::select! {
                // Detect client disconnect. Note: in-flight DNS queries
                // (inside ResolverGroup::lookup) are not individually cancellable —
                // they run to completion. This select cancels *pending* queries
                // (not yet started) and prevents new batches from being sent.
                // For a 10-type query, at most 1 query's worth of work is wasted.
                _ = tx.closed() => break,
                // Enforce overall stream timeout (prevents unbounded SSE connections)
                _ = tokio::time::sleep_until(stream_deadline) => {
                    let _ = tx.send(Ok(Event::default()
                        .event("error")
                        .json_data(&ErrorEvent::stream_timeout()).unwrap())).await;
                    break;
                }
                result = futs.next() => {
                    match result {
                        None => break, // all queries completed
                        Some(Ok(lookups)) => {
                            completed += 1;
                            let event = Event::default()
                                .event("batch")
                                .json_data(&BatchEvent { lookups, completed, total })
                                .unwrap();
                            if tx.send(Ok(event)).await.is_err() { break; }
                        }
                        Some(Err(e)) => {
                            errors += 1;
                            let _ = tx.send(Ok(Event::default()
                                .event("error")
                                .json_data(&ErrorEvent::from(e)).unwrap())).await;
                        }
                    }
                }
            }
        }
        let stats = StreamStats {
            total_queries: (total * server_count) as u32,
            responses: completed,
            errors,
            duration_ms: start.elapsed().as_millis() as u64,
        };
        let _ = tx.send(Ok(Event::default().event("done").json_data(&stats).unwrap())).await;
    });

    Ok(Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default()))
}
```

### 7.3 Static File Serving

```rust
#[derive(RustEmbed)]
#[folder = "frontend/dist"]
struct Assets;

async fn static_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    match Assets::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            // Vite-built assets have content hashes in filenames (e.g., app-Ab1Cd2.js)
            // and can be cached immutably. index.html must not be cached (it references
            // hashed asset URLs that change on each build).
            let cache = if path == "index.html" || path.is_empty() {
                "no-cache"
            } else {
                "public, max-age=31536000, immutable"
            };
            ([
                (header::CONTENT_TYPE, mime.as_ref()),
                (header::CACHE_CONTROL, cache),
            ], file.data).into_response()
        }
        None => {
            // SPA fallback: serve index.html for client-side routing
            match Assets::get("index.html") {
                Some(index) => {
                    ([(header::CONTENT_TYPE, "text/html")], index.data).into_response()
                }
                None => {
                    (StatusCode::INTERNAL_SERVER_ERROR,
                     "Frontend assets not found. Run 'npm run build' in frontend/ first.")
                        .into_response()
                }
            }
        }
    }
}
```

In debug builds, `rust-embed` reads from the filesystem, enabling `vite dev` hot-reload during development.

## 8. Security Architecture

This service is an open DNS proxy with a web frontend. Security is not optional — it is load-bearing structure. The design follows a defense-in-depth model with four layers.

### 8.1 Layer 1 — Query Restrictions (hardcoded)

These restrictions are unconditional and cannot be overridden by configuration:

| Restriction | Value | Rationale |
|-------------|-------|-----------|
| Blocked query types | ANY, AXFR, IXFR | Amplification and zone transfer abuse |
| Blocked target IPs | RFC 1918, localhost, link-local, CGNAT, multicast | Internal network probing prevention |
| Max record types per query | 10 | Limit fan-out amplification |
| Max servers per query | 4 | Limit amplification |
| Per-query timeout | 10 seconds (hard cap) | Prevent individual DNS query exhaustion |
| Stream timeout | 30 seconds (hard cap) | Prevent unbounded SSE connections |
| Response size cap | 64 KB per lookup (serialized JSON) | Prevent memory abuse. If a single `Lookup` serializes to >64KB, it is replaced with an error event (`RESPONSE_TOO_LARGE`) containing the query details but not the oversized data. Other lookups in the batch are unaffected. |
| Max domain length | 253 characters | DNS protocol limit enforcement |
| Trace max depth | 10 hops | Prevent unbounded recursion |
| Check max lints | All enabled (no arbitrary depth) | Bounded by lint count |
| Request body size | 8 KB | Prevent oversized payloads (via `RequestBodyLimitLayer`) |
| Max concurrent connections | 256 | Prevent connection exhaustion (via `ConcurrencyLimitLayer`) |

Target IP validation applies to **explicitly user-provided server IPs** (i.e., `@192.168.1.1` in the query). When a user specifies a predefined provider name (`@cloudflare`), the IP is resolved from `PredefinedProvider` — these are known-safe public DNS servers and bypass this check. When no `@server` is specified, the configured `default_servers` are used (see §9).

```rust
fn is_allowed_target(ip: IpAddr) -> Result<(), RejectReason> {
    if ip.is_loopback() { return Err(RejectReason::Loopback); }
    if ip.is_unspecified() { return Err(RejectReason::Unspecified); }
    if ip.is_multicast() { return Err(RejectReason::Multicast); }
    if is_rfc1918(ip) { return Err(RejectReason::PrivateNetwork); }
    if is_link_local(ip) { return Err(RejectReason::LinkLocal); }
    if is_cgnat(ip) { return Err(RejectReason::CGNAT); }        // 100.64.0.0/10
    if is_documentation(ip) { return Err(RejectReason::Documentation); }
    Ok(())
}
```

**DNS rebinding is not a concern**: prism only sends DNS protocol traffic to nameservers — it never connects to resolved IP addresses via HTTP or any other protocol. An attacker setting up `evil.example.com` with an A record pointing to `127.0.0.1` would only receive that record in the query results; the service does not follow up by connecting to the resolved address.

### 8.2 Layer 2 — Rate Limiting (tower-governor)

Uses the GCRA (Generic Cell Rate Algorithm) via `tower-governor`, which provides smooth, fair rate limiting with configurable burst tolerance.

| Scope | Limit | Burst | Key |
|-------|-------|-------|-----|
| Per source IP | 30 tokens/minute | 10 | Real client IP (see 8.3) |
| Per source IP (connections) | 10 concurrent SSE streams | — | Real client IP |
| Per target DNS server | 30 tokens/minute | 10 | Target server IP (across all users) |
| Global | 500 tokens/minute | 50 | None (shared) |

**Query cost model**: Each request consumes tokens proportional to its fan-out: `cost = record_types * servers`. A simple `example.com A @cloudflare` costs 1 token. A `example.com A AAAA MX TXT @cloudflare @google` costs 8 tokens (4 types x 2 servers). This prevents a single high-cardinality request from consuming the same rate-limit budget as a simple one-shot query.

**Pre-check enforcement**: The cost is computed after parsing, **before execution**. If the cost would exceed the remaining token budget, the request is rejected with 429 and a `Retry-After` header — no DNS queries are issued. This prevents a 40-token request from overdriving the bucket into deficit. The response includes the computed cost and remaining budget so clients can adjust:

```json
{"error": {"code": "RATE_LIMITED", "message": "Query cost 40 exceeds remaining budget 12", "retry_after_secs": 48}}
```

The per-target-server limit helps prevent the service from being weaponized against specific DNS servers. When `allow_arbitrary_servers = false` (default), the key is the **provider name** (e.g., `"cloudflare"`), not individual server IPs. This prevents gaming the limit by alternating between a provider's multiple IPs (e.g., 1.1.1.1 and 1.0.0.1). The `@system` special case uses the fixed key `"system"` — all system resolver traffic shares one bucket regardless of how many IPs `/etc/resolv.conf` contains. When `allow_arbitrary_servers = true`, the key falls back to per-IP since there is no provider name for arbitrary addresses — operators accepting this mode should be aware of the per-IP gaming vector.

**Limitation**: This limit only protects the explicitly specified `@server` targets, not the chain of authoritative nameservers that handle recursive resolution. A query for `evil.example.com @cloudflare` rate-limits traffic to Cloudflare, but Cloudflare's recursors will query the authoritative servers for `evil.example.com` — those authoritatives are not rate-limited by this layer. This is an inherent limitation of any DNS proxy: the operator controls which recursors are queried, not the full resolution chain. The primary mitigation is `allow_arbitrary_servers = false` (the default), which restricts targets to the six predefined providers (Cloudflare, Google, Quad9, Mullvad, Wikimedia, DNS4EU) whose infrastructure can absorb the load.

### 8.2.1 Circuit Breaker

If a target DNS provider is degraded (e.g., Cloudflare returns timeouts), every request targeting that provider will wait for the 10-second per-query timeout. Under load, this exhausts the connection pool with stuck requests. A per-provider circuit breaker mitigates this:

- **Closed** (normal): Queries proceed. Track error rate per provider over a sliding 60-second window.
- **Open** (tripped): If >50% of queries to a provider failed in the window, short-circuit new queries to that provider with an immediate error event (`event: error`, code `PROVIDER_DEGRADED`). No DNS query is issued.
- **Half-open** (probe): After a 30-second cooldown, allow one probe query. If it succeeds, close the breaker. If it fails, reopen.

The breaker registry is global shared state (`Arc<CircuitBreakerRegistry>` in axum's app state), keyed by provider name. Each request consults the registry before building a `ResolverGroup` — if the target provider's breaker is open, the request fails fast without constructing a resolver or issuing DNS queries. This is the one exception to the per-request `ResolverGroup` pattern (§13.4): the breaker state is shared, but the resolvers are still per-request.

This is a Phase 1 item — without it, a single degraded upstream can cause cascading timeouts across all connections.

### 8.3 Layer 3 — Client IP Extraction

Behind a reverse proxy (Cloudflare, nginx), the direct peer IP is the proxy, not the client. IP extraction priority:

1. `CF-Connecting-IP` (when behind Cloudflare)
2. `X-Real-IP` (when behind nginx with `proxy_set_header`)
3. Rightmost untrusted IP in `X-Forwarded-For` (walk right-to-left, skip configured trusted proxy ranges)
4. Direct peer address (fallback)

The set of trusted proxy CIDRs is a configuration parameter. **When `trusted_proxies` is empty (default), all proxy headers (`CF-Connecting-IP`, `X-Real-IP`, `X-Forwarded-For`) are ignored entirely**, and the direct peer address is used. This is the safe default — trusting proxy headers without configured trusted proxies allows client IP spoofing. Proxy header extraction only activates when at least one trusted proxy CIDR is configured.

### 8.4 Layer 4 — HTTP Security Headers

Applied via `tower-http` layers on every response:

```
Content-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; frame-ancestors 'none'
X-Content-Type-Options: nosniff
X-Frame-Options: DENY
Referrer-Policy: strict-origin-when-cross-origin
Strict-Transport-Security: max-age=31536000; includeSubDomains
```

**TLS termination**: prism does not perform TLS termination itself — it binds plain HTTP. TLS is expected to be handled by a reverse proxy (Cloudflare, nginx, Caddy) in front of the service. The HSTS header is only meaningful when the reverse proxy terminates TLS and forwards the header to the client. When running directly without a reverse proxy (local development), the HSTS header is ignored by browsers since the connection is not HTTPS. The header is included unconditionally for simplicity — the reverse proxy may strip or replace it as needed.

CORS is configured to allow the service's own origin only (no wildcard). In production, the SPA is served from the same origin (embedded by rust-embed), so **CORS preflight never triggers for same-origin requests** — the CORS layer is a defense-in-depth measure against third-party sites trying to use the API. The POST endpoint (structured JSON) would trigger preflight from a different origin due to `Content-Type: application/json`; the GET endpoint (native `EventSource`) is a simple request and would not.

**Development mode**: The Vite dev server (`:5173`) and axum (`:8080`) are different origins. The Vite config should use `server.proxy` to forward `/api/*` requests to axum, avoiding CORS entirely during development. The frontend's connection error banner (§6.3) exists primarily for network errors and non-2xx responses, not CORS — though it would also catch CORS failures in a misconfigured deployment.

### 8.5 Logging and Monitoring

**Request IDs**: Every request is assigned a UUID v7 (time-ordered) request ID. It is returned in the `X-Request-Id` response header, included in every SSE event (`request_id` field), and logged. This enables end-to-end correlation: a user reporting "my query is slow" can share the request ID (visible in the JSON tab or browser dev tools), and the operator can find it in the logs.

**Logged per request:**
- Request ID (UUID v7)
- Timestamp (UTC, ms precision)
- Client IP (hashed after 30-day retention for GDPR)
- Query parameters: domain, record types, target servers
- Response metadata: status code, response time, error type
- Rate limit events: which limit was hit

**Not logged:**
- Full DNS response content (may contain PII in TXT records)
- API keys or session tokens
- User-Agent strings

**Metrics** (Prometheus format, served on a **separate port** — default `:9090` — not exposed on the public-facing port):
- `prism_queries_total` (counter, labels: status, record_type)
- `prism_query_duration_seconds` (histogram)
- `prism_rate_limit_hits_total` (counter, labels: scope)
- `prism_active_queries` (gauge)
- `prism_circuit_breaker_state` (gauge, labels: provider, state=closed|open|half_open)
- `prism_circuit_breaker_trips_total` (counter, labels: provider)

Circuit breaker state transitions are logged at `warn` level (`provider=cloudflare state=open error_rate=0.65`). This surfaces in both structured logs and the Prometheus gauge, enabling alerting on degraded providers.

**Operational expectations**: prism is a debugging tool, not a production-critical service. There is no formal SLO. Availability depends on upstream DNS providers — when Cloudflare or Google DNS is degraded, so is prism (modulo the circuit breaker). The Prometheus metrics above are sufficient for alerting on error rate spikes and latency degradation. A formal SLO/error-budget framework is out of scope for Phase 1 but could be added if a public instance gains significant traffic.

The metrics port is configured via `[server] metrics_bind = "127.0.0.1:9090"`. Binding to localhost by default ensures metrics are only accessible from the host machine or via an internal network, not from the public internet.

### 8.6 Terms of Service

A public instance requires a ToS page (served as part of the SPA) covering:
- **Prohibited uses**: DDoS amplification, unauthorized DNS enumeration, zone transfer attempts
- **Rate limits**: Service may throttle or block abusive traffic without notice
- **No warranty**: DNS results provided as-is
- **Logging disclosure**: What is logged and retention period
- **GDPR rights**: Access, erasure, objection (for EU users)

## 9. Configuration

prism is configured via environment variables and/or a TOML config file. All values are validated at startup — the server refuses to start with invalid configuration rather than silently using defaults.

**Config discovery** (in precedence order, highest first):
1. CLI argument: `prism --config /path/to/config.toml`
2. Environment variable: `PRISM_CONFIG=/path/to/config.toml`
3. Default paths: `./prism.toml`, then `$XDG_CONFIG_HOME/prism/config.toml`
4. Built-in defaults (no config file required)

**Environment overrides**: Individual values can be overridden via env vars prefixed with `PRISM_`, using `__` as the section separator: `PRISM_LIMITS__PER_IP_PER_MINUTE=60` overrides `[limits] per_ip_per_minute`. Env vars take precedence over TOML file values, enabling partial TOML configs with per-deployment overrides.

**Config vs. hardcoded limits**: The hardcoded caps in §8.1 (10s query timeout, 30s stream timeout, 10 record types, 4 servers, 8KB body) are **upper bounds that cannot be overridden by config**. Config values are clamped to these maximums. For example, `max_timeout_secs = 30` in the config is silently clamped to 10 with a startup warning. Setting `per_ip_per_minute = 0` or `max_servers = 0` is rejected as invalid.

```toml
# prism.toml

[server]
bind = "127.0.0.1:8080"
metrics_bind = "127.0.0.1:9090"   # separate port, localhost-only by default
# Behind a reverse proxy, set the trusted proxy CIDRs:
trusted_proxies = ["173.245.48.0/20", "103.21.244.0/22"]  # Cloudflare ranges

[limits]
per_ip_per_minute = 30
per_ip_burst = 10
per_target_per_minute = 30
global_per_minute = 500
max_concurrent_connections = 256
per_ip_max_streams = 10
max_timeout_secs = 10
max_record_types = 10
max_servers = 4

[dns]
# Default servers when no @server is specified (used when no @server in query)
default_servers = ["cloudflare"]
# Allow @system (host's /etc/resolv.conf). Default true for private deployments.
# Set to false on public instances — system resolvers may leak infrastructure details
# or violate cloud provider AUP.
allow_system_resolvers = true
# Allow querying arbitrary IPs (true) or only predefined providers (false)
# Default is false — only predefined providers are allowed.
# Set to true only for private/internal deployments where open-proxy risk is acceptable.
allow_arbitrary_servers = false
```

When `allow_arbitrary_servers = false` (the default), users can only query the six predefined providers (Cloudflare, Google, Quad9, Mullvad, Wikimedia, DNS4EU) — plus system resolvers if `allow_system_resolvers = true`. This eliminates the open-proxy risk and ensures rate limiting protects infrastructure that can absorb the load.

**`@system` on public deployments**: System resolvers use the host's `/etc/resolv.conf`, which on cloud infrastructure is typically the provider's internal DNS. Risks:
- **Infrastructure leak**: Internal resolver IPs appear in response metadata.
- **DNS oracle**: An attacker querying `internal.corp.example.com @system` through a corporate resolver reveals whether internal domains exist, exposing internal DNS structure.
- **AUP violation**: Cloud providers may prohibit using their internal DNS as a public-facing resolver.

Public deployments should set `allow_system_resolvers = false` and configure `default_servers` to a predefined provider (e.g., `["cloudflare"]`).

## 10. Dependencies

### 10.1 Rust (prism crate)

| Crate | Version | Purpose | Weight |
|-------|---------|---------|--------|
| `mhost` | path dep | DNS library (resolver, resources, nameserver) | existing |
| `axum` | 0.8 | Web framework (routes, extractors, SSE) | ~hyper + tower |
| `tower-http` | 0.6 | CORS, compression, tracing, security headers | light |
| `tower-governor` | latest | Rate limiting (GCRA via governor) | light |
| `rust-embed` | 8 | Embed frontend assets in binary | compile-time only |
| `mime_guess` | 2 | Content-Type for static files | tiny |
| `tokio` | 1 | Async runtime | already in mhost |
| `tokio-stream` | 0.1 | `ReceiverStream` for SSE | tiny (tokio companion) |
| `serde` + `serde_json` | 1 | Serialization | already in mhost |
| `tracing` | 0.1 | Structured logging | already in mhost |
| `metrics` + `metrics-exporter-prometheus` | 0.24 / 0.16 | Prometheus metrics collection and `/metrics` endpoint | light |

### 10.2 Frontend (npm)

| Package | Purpose |
|---------|---------|
| `solid-js` | Reactive UI framework (~7KB) |
| `@codemirror/view` | Editor core |
| `@codemirror/state` | Editor state management |
| `@codemirror/autocomplete` | Autocomplete infrastructure |
| `@codemirror/language` | Syntax highlighting |
| `vite` | Build tool (dev dependency) |
| `vite-plugin-solid` | Solid JSX transform (dev dependency) |

Estimated frontend bundle: ~60KB gzipped (Solid 7KB + CodeMirror 6 ~40KB + application code ~13KB).

## 11. Build Process

```sh
# Development (two terminals)
cd prism/frontend && npm run dev    # Vite dev server with HMR on :5173 (proxies /api/* to :8080)
cd prism && cargo run               # axum server on :8080, reads frontend from disk

# Production build
cd prism/frontend && npm run build  # outputs to frontend/dist/
cd prism && cargo build --release   # rust-embed bakes dist/ into binary

# Result: single binary with embedded frontend
./target/release/prism              # serves everything on :8080
```

The `cargo build` step for prism depends on `frontend/dist/` existing. The `build.rs` script checks for `frontend/dist/index.html` and emits a clear compile error if missing:

```
error: frontend/dist/index.html not found.
  Run 'cd prism/frontend && npm ci && npm run build' before 'cargo build -p prism'.
```

A `just` recipe (or Makefile target) sequences the full build: `just build-web` runs `npm ci && npm run build` in `frontend/`, then `cargo build -p prism`. CI uses this recipe. Developers running bare `cargo build -p prism` after a fresh clone get the actionable error above.

### 11.1 Docker

prism uses a multi-stage Dockerfile:

```dockerfile
# Stage 1: Build frontend
FROM node:22-alpine AS frontend
WORKDIR /app/prism/frontend
COPY prism/frontend/package*.json ./
RUN npm ci
COPY prism/frontend/ ./
RUN npm run build

# Stage 2: Build Rust binary (musl for fully static binary)
FROM rust:1-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY build.rs ./
COPY prism/ prism/
COPY --from=frontend /app/prism/frontend/dist prism/frontend/dist
RUN cargo build --release -p prism

# Stage 3: Minimal runtime (static binary, no glibc needed)
FROM alpine:3
RUN apk add --no-cache ca-certificates
COPY --from=builder /app/target/release/prism /usr/local/bin/
EXPOSE 8080
ENTRYPOINT ["prism"]
```

The image is published as `lukaspustina/prism` alongside the existing `lukaspustina/mhost` image. Expected image size: ~20MB (statically-linked musl binary + Alpine base + ca-certificates).

**CI optimization**: The Dockerfile above copies all source before building, invalidating the cargo build cache on every source change. After workspace conversion, both `Cargo.toml` (root) and `prism/Cargo.toml` must be in the build context at the right paths for layer caching. For faster CI rebuilds, use `cargo-chef` to cache dependency compilation in a separate layer. This is a CI optimization, not a functional requirement — deferred to when build times become painful.

## 12. URL Design and Shareability

Every query state is representable as a URL:

```
https://web.mhost.example.com/?q=example.com+MX+TXT+@cloudflare+%2Btls
```

The `q` parameter contains the raw query string (URL-encoded). When the page loads with a `q` parameter, it auto-populates the input and submits the query. This enables:

- Sharing diagnostic URLs in Slack/email during incidents
- Bookmarking common debugging queries
- Browser history navigation between queries
- Deep-linking from documentation or runbooks

## 13. Design Decisions

### 13.1 SSE over WebSocket

DNS queries are unidirectional: client submits parameters, server streams results. SSE is strictly simpler for this pattern — no upgrade handshake, no framing, works through proxies, and is curl-debuggable. WebSocket would only be needed if the client sent messages during a stream (e.g., cancellation), which can instead be handled by aborting the HTTP connection.

The frontend uses `GET /api/query?q=...` with the browser's native `EventSource` API. The POST endpoint (structured JSON) is for programmatic clients that build their own SSE consumption.

**EventSource auto-reconnect caveat**: `EventSource` automatically reconnects on connection loss, which would silently re-execute the query and consume additional rate-limit tokens. The frontend **must** call `eventSource.close()` immediately upon receiving a `done` or `error` event to prevent reconnection after successful completion. For mid-stream connection drops (network blip before `done`), the reconnect is acceptable — DNS queries are idempotent, and the user sees the full result set on retry. The rate-limit cost of one extra query is negligible compared to the UX benefit of automatic recovery. The server sends `retry: 2000` in the SSE stream to limit reconnect frequency.

### 13.2 Parser Flow and Responsibilities

The `/api/query` endpoint accepts **two methods**, eliminating the dual-parser risk:

1. **GET with `?q=`** (frontend, shareable URLs, curl): `GET /api/query?q=example.com+MX+@cloudflare`. The **Rust parser** on the backend is the single source of truth — it parses the query language, validates it, and executes the query. This works with the native `EventSource` API.
2. **POST with JSON body** (programmatic clients): The `QueryRequest` body with explicit `domain`, `record_types`, `servers`, etc. This is the stable typed API for scripts, CI tools, and other non-browser consumers.

**Frontend flow**: The SPA submits queries via `GET /api/query?q=...` using native `EventSource`. The TypeScript code is limited to **driving the CodeMirror UI** — syntax highlighting tokens and populating the autocomplete dropdown from a static list (or from `/api/parse` in Phase 3). It does **not** parse the query language into structured JSON. This eliminates the dual-parser drift risk entirely: there is exactly one parser (Rust) that converts the query language into execution parameters.

**Why both methods**: GET with `?q=` gives the browser native SSE support, shareable URLs, and a single source-of-truth parser. POST with JSON gives programmatic clients a well-typed API that doesn't require implementing the query language. The two methods are complementary, not competing.

The `/api/parse` endpoint (Phase 3) runs the same Rust parser on partial input and returns context-aware completions. Until then, the frontend uses a static list of record types and predefined providers for autocomplete.

### 13.3 No Authentication in MVP

The initial version is anonymous-only with strict rate limits. Authentication (API keys, OAuth) adds significant complexity (key storage, session management, account UI) and is deferred to a future phase. The rate limiting and query restriction layers provide sufficient protection for a public instance.

### 13.4 Per-Query ResolverGroup

Each API request builds a fresh `ResolverGroup` from the requested server specs. There is no shared resolver pool across requests. This is simpler, avoids shared mutable state, and ensures each query uses exactly the servers the user requested. The performance cost (resolver setup is ~1ms) is negligible compared to DNS network latency.

### 13.5 No Server-Side Caching

DNS results are not cached by the web service. Each query hits the upstream DNS servers directly. Rationale:
- Users of a debugging tool expect fresh results, not cached ones
- The upstream resolvers already cache according to TTL
- Caching adds complexity (invalidation, storage, cache-poisoning surface)
- The rate limiter already protects upstream servers from excessive load

### 13.6 SolidJS over React

SolidJS was chosen for its fine-grained reactivity model. When an SSE event adds a row to the results table, Solid updates only that row — no virtual DOM diffing of the entire table. This matters for queries that produce 50+ results streaming in rapid succession. The 7KB runtime (vs. React's ~40KB) also keeps the embedded binary smaller. The tradeoff is a smaller ecosystem, but the frontend is simple enough (one input, one table, three tabs) that ecosystem breadth is not a concern.

### 13.7 Single Source-of-Truth Parser

The Rust query language parser is the **only** parser that converts query strings into execution parameters. The frontend submits queries via `GET /api/query?q=...` using native `EventSource` and never constructs structured JSON from the query language. This eliminates the dual-parser drift risk that would arise if both TypeScript and Rust had to agree on grammar semantics (IP:port parsing, reverse lookup detection, `ALL` disambiguation, error tolerance).

The TypeScript code in the frontend is limited to **UI concerns**: tokenizing the input string for syntax highlighting (coloring `@` tokens green, `+` tokens orange) and driving CodeMirror autocomplete from a static list. These are purely cosmetic — a misclassified token in the TS highlighter produces a wrong color, not a wrong query. The Rust parser is the authority.

## 14. Phased Delivery

### Phase 0 — Workspace Conversion

- Convert mhost from single crate to Cargo workspace (see §3.1)
- Verify CI, release workflow, and feature flags work correctly
- Add prism as empty workspace member with skeleton `main.rs`

### Phase 1 — MVP

- axum server with embedded SPA
- `GET /api/query?q=` (native EventSource) and `POST /api/query` (structured JSON) with SSE streaming
- Rust query language parser with record types and `@server` support
- CodeMirror 6 input with autocomplete for record types and predefined servers
- Results table grouped by record type, progressive population
- Rate limiting (per-IP, per-target, global) with query cost model and pre-check enforcement
- Per-provider circuit breaker for upstream degradation
- Query restrictions (blocked types, blocked target IPs)
- Security headers, CORS
- Config validation at startup
- JSON tab for raw output
- URL shareability via `?q=` parameter
- Dark mode
- `GET /api/health` (exempt from rate limiting)
- Prometheus metrics on separate port (`:9090`, localhost-only)

### Phase 1.5 — Library Extraction (prerequisite for Phase 2)

This is the highest-risk prerequisite for Phase 2 and should be completed independently before any Phase 2 web work begins. The lint functions and trace logic currently live in the `app` layer, gated behind `feature = "app"`. They must be extracted to the library so prism can use them without pulling in CLI dependencies.

**Scope:**
- **Lint extraction**: Move 13 lint modules from `src/app/modules/check/lints/` to a new `src/check/lints/` library module: `spf.rs`, `mx.rs`, `caa.rs`, `ns.rs`, `soa.rs`, `dmarc.rs`, `dnssec_lint.rs`, `open_resolver.rs`, `delegation.rs`, `cnames.rs`, `https_svcb.rs`, `axfr.rs`, `ttl.rs`. The `CheckResult` type and lint runner move with them. CLI-specific formatting (`SummaryFormatter` for check results) stays in `app/`.
- **Trace extraction**: Move delegation-walking logic from `src/app/modules/trace/trace.rs` to a new `src/trace/` library module. The `TraceHop` type and recursive resolution logic move with it. CLI-specific output stays in `app/`.
- **Consumer migration**: Update `app/modules/check/` and `app/modules/trace/` to re-export from the library modules. Update mdive TUI imports (`src/bin/mdive/lints.rs`) to use the new library paths. Both consumers must continue working unchanged.
- **Feature gate semantics**: The new library modules have no feature gate — they are available in all builds. The `app` feature continues to gate only CLI-specific code.
- **Testing**: All existing check and trace tests must pass. New unit tests for the extracted modules (without app dependencies) confirm library-only usability.

This refactoring benefits all three consumers (CLI, TUI, web) and reduces the `app` feature's surface area.

### Phase 2 — Check and Trace

- `POST /api/check` with lint results (depends on Phase 1.5)
- `POST /api/trace` with hop-by-hop delegation visualization (depends on Phase 1.5)
- `+check` and `+trace` flags in query language
- Transport selection (`+tls`, `+https`, `+tcp`, `+udp`)
- `+dnssec` flag
- Server comparison tab (multi-server divergence view)
- Expandable row details (rdata fields, server latency, human-readable interpretation)

### Phase 3 — Polish

- `POST /api/parse` for server-side autocomplete
- Per-server response time display
- Keyboard shortcuts (j/k navigation, ? help)
- Query history (localStorage)
- Light mode toggle
- Mobile-responsive layout
- Terms of Service page
- GDPR-compliant log rotation

### Phase 4 — Future (not committed)

- Authentication (API keys for higher rate limits)
- DNSSEC chain visualization
- Propagation map view
- Subdomain discovery (`+discover`)
- Diff mode (`+diff` with 2+ servers)
- Export results (JSON download, zone file format, curl command)
- Configuration profiles (saved server sets)

## 15. Testing Strategy

### 15.1 Rust (prism)

- **Query language parser**: Unit tests covering all token types, edge cases (IP:port, reverse lookup detection, ALL expansion, case insensitivity, unknown tokens as warnings, unimplemented flags as errors). This is the most critical test surface since the parser is the single source of truth.
- **Query policy validation**: Unit tests for `is_allowed_target()`, blocked query types, `max_record_types`/`max_servers` limits, `allow_system_resolvers`/`allow_arbitrary_servers` enforcement.
- **Rate limiting**: Unit tests for query cost calculation (`record_types * servers`). Integration tests verifying pre-check rejection when budget exceeded.
- **SSE streaming**: Integration tests using `axum::test` (or `tower::ServiceExt`) to verify the full request→SSE event flow. Assert batch event structure, done event stats, error events, stream timeout. These tests use mocked DNS responses (not real network) via a test `ResolverGroup` that returns canned `Lookups`.
- **Config validation**: Unit tests for startup validation — invalid values rejected, excessive values clamped, env var overrides applied correctly.
- **Circuit breaker**: Unit tests for state transitions (closed→open→half-open→closed), error rate tracking, cooldown timing.

### 15.2 Frontend

- **Tokenizer**: Unit tests (vitest) for syntax highlighting token classification. Verify that misclassified tokens produce wrong colors, not wrong queries.
- **Component tests**: Minimal component tests for `QueryInput` (submit, autocomplete dropdown) and `ResultsTable` (progressive batch accumulation, EventSource lifecycle).
- **No E2E tests in Phase 1**: Full browser E2E tests (Playwright) are deferred — the Rust integration tests cover the API surface, and the frontend is simple enough that component tests suffice.

## 16. Risks and Open Questions

1. **Workspace conversion.** mhost is currently a single crate, not a Cargo workspace. Converting to a workspace (Phase 0) is a prerequisite that touches the root `Cargo.toml`, CI workflows, and release automation. The migration itself is low-risk (the mhost crate's public API is unchanged), but it must be validated end-to-end before any prism development begins. See §3.1 for the detailed scope.

2. **Library extraction for check/trace.** Phase 1.5 requires moving 13 lint modules and the trace logic from `app/` to the library layer. This is the highest-risk prerequisite — it changes module boundaries, requires updating three consumers (CLI, TUI, web), and must maintain feature-gate semantics. See §14 Phase 1.5 for the detailed scope. Failure or delay here blocks all of Phase 2.

3. **Frontend build dependency.** The Rust binary requires `frontend/dist/` to exist at compile time. CI must run `npm run build` before `cargo build`. This adds Node.js as a build-time dependency. Mitigation: `build.rs` checks for `frontend/dist/index.html` and emits an actionable compile error if missing. A `just build-web` recipe sequences the full build (npm + cargo). `frontend/dist/` is `.gitignored` — it is never checked into the repository.

4. **CodeMirror 6 bundle size.** CodeMirror 6 is tree-shakeable but still contributes ~40KB gzipped. For a tool whose total frontend should be fast-loading, this is significant but acceptable — the autocomplete UX is worth the cost (IME, accessibility, structured editing). If bundle size becomes a concern, the CodeMirror input could be replaced with a simpler custom `contenteditable` input that sacrifices structured editing for ~38KB savings.

5. **SSE connection limits.** Browsers limit concurrent SSE connections per domain to ~6 (HTTP/1.1) or ~100 (HTTP/2). If a user opens many tabs, older connections may be dropped. Mitigation: ensure the server uses HTTP/2; the frontend should close the `EventSource` after the `done` event. Server-side: a global `ConcurrencyLimitLayer` (256 connections) and per-IP active stream cap (10) prevent connection exhaustion from malicious clients.

6. **Tokenizer drift.** The TypeScript tokenizer in the frontend drives syntax highlighting and autocomplete in CodeMirror. It classifies tokens by prefix (`@` = server, `+` = flag, uppercase = record type) for coloring purposes only — it does not parse the query language into execution parameters. The Rust parser is the single source of truth (queries are submitted via `GET /api/query?q=` and parsed server-side). If the TS tokenizer misclassifies a token, the only impact is a wrong syntax highlight color, not a wrong query. This is a cosmetic risk, not a correctness risk.

7. **mhost-lib API stability.** `prism` depends on mhost-lib's public types (`Lookups`, `Lookup`, `Record`, `RData`, etc.) being serializable and stable. Since these types are also used by mhost's own JSON output and snapshot features, they are unlikely to change incompatibly. But a breaking change in mhost-lib would require a coordinated update.

8. **ResolverGroup per-request cost.** Building a `ResolverGroup` for every request involves creating hickory-resolver instances. Benchmarking is needed to confirm this is fast enough under load. If it becomes a bottleneck, a cache of recently-used resolver configurations (keyed by server set + timeout) could be added.

9. **Abuse via high-cardinality queries.** A single request with 10 record types across 4 servers generates 40 DNS queries. The query cost model (§8.2) charges this as 40 tokens instead of 1. Requests exceeding the remaining budget are rejected before execution (pre-check). This forces attackers to choose between query breadth and request volume. The per-target-server limit (30 tokens/min) provides a second line of defense.
