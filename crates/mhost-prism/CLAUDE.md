# CLAUDE.md — prism

## Rules

- Do NOT add a `Co-Authored-By` line for Claude in commit messages.
- Don't add heavy dependencies for minor convenience — check if existing deps already cover the need.
- Don't mix formatting-only changes with functional changes in the same commit.
- Don't modify unrelated modules "while you're in there" — keep changes scoped.
- Don't add speculative flags, config options, or abstractions without a current caller.
- Don't bypass failing checks (`--no-verify`, `#[allow(...)]`) without explaining why.
- Don't hide behavior changes inside refactor commits — separate them.
- Don't include PII, real email addresses, or real domains (other than example.com) in test data, docs, or commits.
- If uncertain about an implementation detail, leave a concrete `TODO("reason")` rather than a hidden guess.

## Engineering Principles

- **Performance**: Prioritize efficient algorithms and data structures. Benchmark critical paths, avoid unnecessary allocations and copies.
- **Rust patterns**: Use idiomatic Rust constructs (enums, traits, iterators) for clarity and safety. Leverage type system to prevent invalid states.
- **KISS**: Simplest solution that works. Three similar lines beat a premature abstraction.
- **YAGNI**: Don't build for hypothetical future requirements — solve the current problem.
- **DRY + Rule of Three**: Tolerate duplication until the third occurrence, then extract.
- **SRP**: Each module/struct has one reason to change. Split when responsibilities diverge.
- **Fail Fast**: Validate at boundaries, return errors early, don't silently swallow failures.
- **Secure by Default**: Sanitize external input, no PII in logs, prefer safe APIs. This is an open DNS proxy — security is load-bearing (see Security Checklist below).
- **Determinism**: Same input → same output. Pin randomness in tests, avoid time-dependent logic where possible.
- **Reversibility**: Prefer changes that are easy to undo. Feature flags over big-bang migrations, small commits over monolithic ones.

## Project Overview

**prism** is a web-based DNS debugging service powered by mhost-lib. It serves an embedded SPA and exposes mhost-lib's capabilities (multi-server fan-out, streaming results, DNSSEC validation) through an HTTP API with Server-Sent Events streaming.

- **Author**: Lukas Pustina | **License**: MIT / Apache-2.0
- **Repository**: Standalone repo (separate from mhost). Depends on `mhost` as a published crate (no `app` feature).
- **README**: `README.md` — user-facing docs: features, query language, all three modes, API reference, configuration, security, dev setup.
- **SDD**: `docs/done/sdd-2025-03-07.md` (historical) — the original design document. Current architecture is documented in this file (CLAUDE.md) and README.md.

prism provides and all functionality must adhere to these core principles:

- high performance
- high efficiency
- high stability
- high security (defense-in-depth: query restrictions, rate limiting, IP extraction, security headers)

## Design Document

The original Software Design Document (`docs/done/sdd-2025-03-07.md`, historical) defined the initial architecture, API design, security model, and phased delivery. It is kept for reference but is no longer the source of truth — the implemented architecture is documented in this file (CLAUDE.md) and README.md. Key SDD sections for historical context:

- **§4** Query language syntax and semantics
- **§5** API endpoints (query, check, trace, parse, metadata)
- **§7** Backend architecture (axum, SSE streaming, FuturesUnordered pattern)
- **§8** Security architecture (4-layer defense-in-depth)
- **§9** Configuration (TOML + env vars)
- **§14** Phased delivery plan

## Technology Decisions

Decisions made during project setup, supplementing the SDD:

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Repository** | Standalone repo, `mhost` via crates.io | Independent release cadence; gaps in mhost-lib addressed upstream separately |
| **CSS** | Plain CSS with custom properties | Frontend is ~3 components; custom properties map directly to mhost's color palette and dark mode toggle; zero build config |
| **Config parsing** | `config` crate | Built-in layering (TOML file + env vars + defaults); handles `PRISM_` prefix and `__` section separators natively |
| **Error handling** | `thiserror` | Structured `ApiError` enum maps to specific HTTP status + error codes; no need for `anyhow`'s erased types |
| **Request IDs** | `uuid` crate with `v7` feature | Time-ordered UUIDs; universally recognized in headers/logs/JSON |
| **TypeScript** | Strict mode (`strict: true`) | Frontend is thin — low ceremony cost, catches bugs at compile time |

## Roadmap

Phased delivery as originally defined in the SDD (historical):

- ~~**Phase 0**: Workspace conversion (mhost repo)~~ — N/A (standalone repo)
- ~~**Phase 1**: MVP — query endpoint, parser, results table, rate limiting, circuit breaker, metrics~~
- ~~**Phase 2**: Transport & UI enhancements — transport flags, DNSSEC, server comparison, expandable rows~~
- ~~**Phase 3**: Polish — server-side autocomplete, keyboard shortcuts, history, mobile~~
- ~~**Phase 4**: check/trace endpoints, stream timeouts, domain-length validation, `+check`/`+trace` routing~~
- ~~**Phase 5**: IP enrichment via ifconfig-rs — clickable IPs, inline badges, infrastructure lint, trace annotations~~
- ~~**Phase 6**: Transport comparison (`+compare`) and auth-vs-recursive (`+auth`) endpoints + frontend~~

## Build & Test

Use `make` targets for all build and test operations.

```sh
# Prerequisites: Node.js (for frontend), Rust toolchain

# Full production build (frontend + backend)
make                                  # or: make all

# Individual targets
make check                            # cargo check (fast compile check)
make test                             # cargo test
make clippy                           # cargo clippy -- -D warnings
make fmt                              # cargo fmt
make fmt-check                        # cargo fmt -- --check
make lint                             # clippy + fmt-check
make frontend                         # cd frontend && npm ci && npm run build
make clean                            # remove target/ + frontend/dist/ + node_modules/
make ci                               # lint + test + frontend (CI pipeline; also use before pushing)

# Development (two terminals)
make frontend-dev                     # Vite dev server :5173 (proxies /api/* to :8080)
make dev                              # cargo run (axum server :8080)
```

### Test Guidelines

- **Query language parser** is the most critical test surface — it is the single source of truth for query semantics.
- **Query policy validation**: Unit tests for `is_allowed_target()`, blocked types, limits enforcement.
- **SSE streaming**: Integration tests via `axum::test` with mocked DNS responses (no real network).
- **Rate limiting**: Unit tests for query cost calculation (`record_types * servers`).
- **Circuit breaker**: Unit tests for state transitions (closed → open → half-open → closed).
- **Config validation**: Test startup validation — invalid values rejected, excessive values clamped.
- **Frontend tokenizer**: Unit tests (vitest) for syntax highlighting classification.

## Architecture

```
mhost-prism/                  # standalone crate (not a workspace member)
  Cargo.toml                  # depends on mhost = "<version>" (crates.io)
  prism.example.toml          # full annotated config reference (all defaults documented)
  prism.dev.toml              # relaxed limits + arbitrary servers for local development
  src/
    main.rs                   # Entry point, axum server setup, graceful shutdown,
                              #   request_id_middleware, http_metrics_middleware
    parser.rs                 # Query language parser (single source of truth for query semantics)
                              #   ParsedQuery fields: truncated_servers: bool (set when @all/@public
                              #   exceed cap), recursive: bool (false when +norecurse)
                              #   Server group aliases: @public → Google+Cloudflare+Quad9,
                              #   @cloudflare → 1.1.1.1+1.0.0.1, @google → 8.8.8.8+8.8.4.4,
                              #   @quad9 → 9.9.9.9+149.112.112.112, @all → all public (capped to 4)
    config.rs                 # config crate: TOML + env vars (PRISM_ prefix)
    error.rs                  # thiserror ApiError enum → HTTP status + error codes
    record_format.rs          # Human-readable formatting for TXT, CAA, MX, SOA, SVCB, TLSA, etc.
    telemetry.rs              # tracing-subscriber init; optional OTel OTLP export; log_format switch
    circuit_breaker.rs        # Per-provider sliding-window breaker (CircuitBreakerRegistry)
    dns_raw.rs                # Raw UDP/TCP hickory-proto queries, glue resolution, build_server_list
    dns_trace.rs              # Iterative delegation walker (root → TLD → authoritative)
    dns_dnssec.rs             # DNSSEC chain-of-trust fetch helpers
    ip_enrichment.rs          # IpEnrichmentService: reqwest + moka cache, batch lookups
    reload.rs                 # SIGHUP hot config reload via ArcSwap
    result_cache.rs           # LRU result cache for permalink sharing
    api/
      mod.rs                  # Route definitions, AppState, shared BatchEvent / STREAM_TIMEOUT_SECS
      query.rs                # GET/POST /api/query → SSE stream (FuturesUnordered fan-out)
      check.rs                # POST /api/check → SSE stream (15 types + DMARC lint)
                              #   Additional checks: lame delegation (AA bit per NS), delegation
                              #   consistency (parent vs. child NS diff), DNSSEC rollover detection
                              #   (multiple KSKs, orphaned DS, missing DS for new KSK), DNSKEY
                              #   algorithm security rating (RSA/MD5 and RSA/SHA-1 deprecated)
      trace.rs                # POST /api/trace → SSE stream (iterative delegation walk)
      compare.rs              # POST /api/compare → SSE stream (transport comparison)
      authcompare.rs          # POST /api/authcompare → SSE stream (auth vs recursive)
      dnssec.rs               # POST /api/dnssec → SSE stream (DNSSEC chain-of-trust)
      parse.rs                # POST /api/parse → completion hints
      results.rs              # Shared result serialisation helpers (lookups → JSON)
      meta.rs                 # GET /api/health (liveness), GET /api/ready (readiness),
                              #   GET /api/servers, GET /api/record-types, GET /api/config
                              #   GET /docs → Scalar API reference UI
                              #   GET /api-docs/openapi.json → OpenAPI spec
    security/
      mod.rs                  # Middleware composition, cors_layer, security_headers
      rate_limit.rs           # 3-tier GCRA (per-IP, per-target, global)
      ip_extract.rs           # Real client IP from CF-Connecting-IP / X-Real-IP / X-Forwarded-For
      query_policy.rs         # Target validation (is_allowed_target), type restrictions, limits
  frontend/                   # SolidJS + Vite (strict TypeScript)
    src/
      App.tsx
      components/
        QueryInput.tsx        # CodeMirror 6 single-line input with autocomplete
        ResultsTable.tsx      # Streaming results table with expand/collapse
        LintTab.tsx           # Check mode lint results with remediation hints
        TraceView.tsx         # Delegation hop visualisation
        DnssecView.tsx        # DNSSEC chain-of-trust display
        ServerComparison.tsx  # Multi-server divergence view
        TransportComparison.tsx # Transport comparison (UDP/TCP/TLS/HTTPS)
        AuthComparison.tsx    # Authoritative vs recursive comparison
      lib/
        tokenizer.ts          # Syntax highlighting (cosmetic only — never affects query execution)
      styles/                 # Plain CSS with custom properties
    dist/                     # Build output, .gitignored, embedded via rust-embed
```

**Dependency rules**:
- prism depends on `mhost` as a published crate (no `app` feature). If mhost-lib lacks needed API surface, address upstream separately.
- prism never imports CLI parsing, terminal formatting, or TUI code.
- The Rust query language parser is the **single source of truth** — the frontend never parses queries into structured JSON.
- The TypeScript tokenizer is cosmetic only (syntax highlighting) — misclassification produces wrong colors, not wrong queries.

## Common Patterns

- **SSE streaming**: Per-record-type queries via `FuturesUnordered`, all record types in parallel; each completed batch streamed as a `batch` SSE event. All streams have a hard 30s deadline. All streaming endpoints also accept `?stream=false` (or `Accept: application/json`) to collect the full stream server-side and return `{ "events": [...], "truncated": bool }` as a single JSON response.
- **Per-request ResolverGroup**: Fresh `ResolverGroup` per API request — no shared resolver pool.
- **No server-side DNS caching**: Debugging tool = fresh results. Upstream resolvers cache per TTL.
- **Query cost model**: Rate limit tokens = `record_types * servers`. Pre-check enforcement before execution. Check endpoint cost = `16 * server_count` (16 steps × number of servers). Trace endpoint cost = flat 16 tokens. Compare endpoint cost = `record_types * servers * 4` (4 transports). Auth compare cost = `record_types * servers + 16` (recursive + NS discovery + auth queries).
- **Circuit breaker**: Per-provider, shared via `Arc<CircuitBreakerRegistry>` in axum app state.
- **Config precedence**: `PRISM_CONFIG` env var or CLI arg > TOML file > built-in defaults. Env vars override TOML (`PRISM_` prefix, `__` section separator). Hardcoded caps are upper bounds that config cannot exceed. Notable options: `PRISM_SERVER__TRUSTED_PROXIES` accepts individual IPs and CIDR ranges (e.g. `["10.0.0.1", "172.16.0.0/12"]`); invalid entries are skipped with a warning at startup.
- **Routing flags**: `+check`, `+trace`, `+compare`, and `+auth` in a query string are routing hints — the frontend detects them and calls the dedicated endpoint. The backend parser accepts them silently; they do not affect query execution at `/api/query`.
- **Query flags**: `+norecurse` sets RD=0 (non-recursive query, stored as `recursive: false` on `ParsedQuery`). `+short` suppresses TTL display in output.

## Key Dependencies

### Rust
- `mhost` (crates.io) — DNS library (no `app` feature)
- `axum` 0.8 — Web framework (routes, extractors, SSE)
- `tower-http` 0.6 — CORS, compression, tracing, security headers
- `tower-governor` — Rate limiting (GCRA)
- `rust-embed` 8 — Embed frontend assets
- `tokio`, `tokio-stream` — Async runtime, `ReceiverStream` for SSE
- `config` — Layered configuration (TOML + env vars)
- `thiserror` — Structured error enums
- `uuid` (v7 feature) — Time-ordered request IDs
- `metrics` + `metrics-exporter-prometheus` — Prometheus metrics

### Frontend
- `solid-js` — Reactive UI (~7KB)
- `@codemirror/*` — Editor core, state, autocomplete, language
- `vite` + `vite-plugin-solid` — Build tooling

## Logging & Telemetry

Rules: [`specs/logging-rules.md`](../specs/logging-rules.md) in the netray.info meta repo. Follow those rules when modifying tracing init, log filters, or `[telemetry]` config.

Default filter: `info,prism=debug,hyper=warn,h2=warn`. Telemetry config via `[telemetry]` section or `PRISM_TELEMETRY__*` env vars. Production uses `log_format = "json"` and `service_name = "prism"`.

## CI/CD

Workflow rules: [`specs/workflow-rules.md`](../specs/workflow-rules.md) in the netray.info meta repo. Follow those rules when creating or modifying any `.github/workflows/*.yml` file.

Workflows: `ci.yml` (PR gate: fmt, clippy, test, frontend, audit), `release.yml` (tag-push: test → build → merge), `deploy.yml` (fires after release via webhook).

**GitHub Packages auth**: Any CI step that runs `npm ci` for the frontend must set `NODE_AUTH_TOKEN: ${{ secrets.GITHUB_TOKEN }}` as an env var on that step. The project `.npmrc` uses `${NODE_AUTH_TOKEN}` as a placeholder (not a hardcoded token) so the token must be injected at runtime. Missing this env var causes E401 from `https://npm.pkg.github.com`.

```yaml
- name: frontend build
  run: npm ci && npm run build
  working-directory: frontend
  env:
    NODE_AUTH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

## Frontend Rules

Full spec: [`specs/frontend-rules.md`](../specs/frontend-rules.md) in the netray.info meta repo.

### Directory & Tooling
- Mirror structure from tlsight: `src/{index.tsx,App.tsx,components/,lib/,styles/global.css}` + `vite.config.ts`, `vitest.config.ts`, `tsconfig.json`, `package.json`, `.npmrc`
- No barrel `index.ts` files — import directly
- tsconfig: `strict: true`, `jsx: "preserve"`, `jsxImportSource: "solid-js"`, `moduleResolution: "bundler"`
- Build: `tsc && vite build`; dev proxy: `/api` → `http://127.0.0.1:808x` (next port after 8081)
- Separate `vitest.config.ts`: `happy-dom` for component tests, `node` for utility tests
- CI `npm ci` steps must set `NODE_AUTH_TOKEN: ${{ secrets.GITHUB_TOKEN }}`

### Common-Frontend — Mandatory
- Import all four shared stylesheets in `global.css`: `theme.css`, `reset.css`, `layout.css`, `components.css`
- Theme: `createTheme('toolname_theme', 'system')` + `<ThemeToggle>` — never custom
- Footer: `<SiteFooter>` with aboutText, links (GitHub, `/docs`, Author), version from `/api/meta`
- Modals: always `<Modal>` (includes focus trap); localStorage: always `storageGet/storageSet`
- Keyboard shortcuts: `createKeyboardShortcuts()` — handles editor exclusions automatically

### Suite Navigation
- Use `<SuiteNav>` from `netray-common-frontend` (BEM: `suite-nav`, `suite-nav__brand`, `suite-nav__sep`, `suite-nav__link`, `suite-nav__link--active`)
- Labels uppercase: IP, DNS, TLS, LENS. Current tool: `suite-nav__link--active` + `aria-current="page"`
- All URLs from `meta.ecosystem.*_base_url` — no hardcoded production URLs. Fall back to `https://*.netray.info`

### Meta Endpoint
- Fetch `/api/meta` on mount; set `document.title` from `meta.site_name`; failure must never block the tool
- Cross-tool deep links: always `meta().ecosystem.*_base_url` + `encodeURIComponent()`

### Page Structure
- `<h1>` = tool name; tagline as adjacent `<span>` — not in the h1
- Required landmarks: `<nav>`, `<main>`, `<footer>`
- Skip link ("Skip to results"), visually hidden, revealed on `:focus`
- `?` help button (min 32×32px) in toolbar → `<Modal>`
- Example usage cards on idle state when tool has distinct modes or non-obvious inputs

### Input UX
- Placeholder: real example, not generic text
- Input must have `aria-label` (not just placeholder)
- `×` clear button inside input when non-empty (`type="button"`, `aria-label="Clear"`, `tabIndex={-1}`)
- Combobox with history: `role="combobox"`, `aria-expanded`, `aria-autocomplete="list"`, `aria-controls`
- History: max 20 entries, deduplicated on insert, stored as `toolname_history` via `storageSet`
- Preset chips (if applicable): ghost/outline style below input
- Note: prism uses CodeMirror 6 for its query input — this is specific to prism's query language and not a suite-wide requirement

### Results & Errors
- Errors: inline red-border box in results area, `role="alert"` — not toast, not modal
- Validation summary: pass/fail/warn/skip chip row at top of results
- Loading: `role="status"` `aria-live="polite"`
- Toasts: ephemeral actions only (copy, export), 2s, `role="status"` `aria-live="polite"`

### API Client
- All fetches via `fetchWithTimeout(url, init, timeoutMs=5000)`
- Extract backend error: `body?.error?.message ?? \`HTTP ${res.status}\``
- `fetchMeta()` returns `null` on failure — never throws

### SolidJS Patterns
- No prop destructuring; access via `props.field`
- `export default` only — no named component exports
- `<Show>` for conditionals, `<For>` for lists — no ternary JSX
- Async data: `createSignal` + `onMount` + try/catch/finally — not `createResource`
- `ErrorBoundary` wraps `<App>` in `index.tsx`
- Component-scoped styles: inline `<style>` tag inside the component

### Styling
- CSS custom properties only — no Tailwind, no utility classes, no CSS-in-JS
- Dark-mode default; `[data-theme="light"]` on `:root`. Light mode must remap ALL color tokens:
  `--accent: #0077cc`, `--pass: #008800`, `--fail: #cc0000`, `--warn: #b86e00`, `--skip: #4a5568`
- Tool-specific semantic tokens in `:root` (e.g. `--pass`, `--fail`) — never raw hex in component CSS

### Accessibility
- Primary buttons: min 37px tall; secondary/toolbar: min 32×32px; nav links: 44px touch target on mobile
- Icon-only buttons: `aria-label` required; query input: `aria-label` required (not just placeholder)
- Keyboard shortcuts skip `INPUT`, `TEXTAREA`, `contenteditable`, `.cm-editor`

### Testing
- Test all non-trivial `lib/` utilities: history, parsers, formatters, domain logic (`node` environment)
- Test components with real interaction logic: `happy-dom` + `@solidjs/testing-library`
- Mock `fetch` via `vi.stubGlobal`; mock `localStorage` in `src/test-setup.ts`
- Test files co-located: `lib/foo.test.ts` next to `lib/foo.ts`

## Security Checklist

When modifying API endpoints or adding features, verify:

- [ ] Blocked query types enforced (ANY, AXFR, IXFR)
- [ ] Target IP validation (no RFC 1918, localhost, link-local, CGNAT, multicast, IPv6 ULA fc00::/7)
- [ ] Glue IPs from trace delegation walk also pass `is_allowed_target` (dns_trace.rs)
- [ ] Query limits respected (max 10 record types, max 4 servers)
- [ ] Timeouts enforced (10s per-query, 30s stream)
- [ ] Rate limiting applied with correct cost calculation
- [ ] No PII in logs (no full DNS response content)
- [ ] Security headers present on all responses
- [ ] CORS restricted to same origin
