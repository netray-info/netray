# Prism Roadmap

Organized from quick wins to larger initiatives. Items within each tier are roughly priority-ordered.

---

## Tier 1 — Quick Wins (days each) ✓

All items completed.

- ✓ Expand all / Collapse all buttons in Results tab
- ✓ j/k navigation in all tabs (Results, Servers, Trace, DNSSEC), h/l for tab switching
- ✓ Result summary with agreement/divergence counts
- ✓ Lint remediation hints (contextual per category/result)
- ✓ Streaming progress context with cancel button
- ✓ Skeleton loaders for Results tab
- ✓ Divergence badge icons with aria-labels
- ✓ Touch target sizing (clear button, view buttons)
- ✓ Skip-to-content link
- ✓ Circuit breaker visibility (record type in messages, degraded_providers in done event)
- ✓ Rate limit feedback (scope field in 429 responses)
- ✓ Rate limiter memory documentation
- ✓ Deployment section in README (nginx, Caddy, security checklist, metrics)

---

## Tier 2 — Moderate Effort (1-2 weeks each) ✓

All items completed.

- ✓ **Shareable Permalinks** — server-side LRU result cache, short cache keys, Share button in query bar, permalink copy
- ✓ **Export Options** — Copy Markdown, Download CSV, Download JSON (HTML report deferred)
- ✓ **Record Semantics Interpretation** — SPF, DMARC, SVCB/HTTPS, TLSA, NAPTR with human-readable formatting and explain toggle
- ✓ **Mobile Responsiveness** — breakpoints, touch targets, safe areas, horizontal scroll for tables
- ✓ **Integration Tests** — backend HTTP handler tests (axum::test), frontend vitest for tokenizer

---

## Tier 3 — Significant Features (2-4 weeks each)

### Transport Comparison View

New UI mode: query the same domain over all supported transports (UDP, TCP, DoT, DoH) in parallel and display results side-by-side. Highlights transport-specific differences in answers, latencies, and failures. Surfaces firewall/middlebox interference that single-transport queries miss. Leverages existing `+udp/+tcp/+tls/+https` flags.

### Query History with Temporal Diff

Self-hosted killer feature — public tools can't do this.
- Server-side: persist query results in a bounded store (SQLite or in-memory with disk spill).
- UI: browseable history timeline per domain. Select two snapshots and diff them: added/removed/changed records, TTL shifts, new/disappeared servers.
- Enables the "what changed since yesterday?" workflow during migrations and incidents.

### Authoritative-vs-Recursive Split View

Automatically query both the domain's authoritative nameservers and the user's chosen recursive resolvers. Display a two-column comparison highlighting differences. Instantly reveals caching staleness, NXDOMAIN hijacking, or split-horizon inconsistencies.

### Batch Queries

`POST /api/query/batch` accepting a list of `{domain, record_types, servers}`. Returns a merged SSE stream with domain labels per batch event. Useful for migration verification ("did all 20 domains propagate?") and CI/CD integration.

### DNSSEC Expiry Timeline

Extend the chain-of-trust visualization to show the temporal dimension:
- Signature expiry dates per zone level.
- Key rollover schedule detection (pre-publish, double-sign).
- Warning badges when RRSIGs are within N days of expiry.
- Timeline chart showing validity windows across the chain.

---

## Tier 4 — Architectural Investments (months) ✓

Infrastructure integrated. Handler-level wiring (resolver pool into build_resolver_group, query dedup into SSE handlers, hot_state reads in handlers) deferred as follow-up.

- ✓ **Connection Pooling** — TTL+LRU resolver cache keyed by (provider, transport), background cleanup task
- ✓ **Query Deduplication** — broadcast-based coalescing with deterministic QueryHash, RAII guard cleanup
- ✓ **OpenTelemetry Integration** — opt-in OTLP HTTP tracing, configurable sampling, zero overhead when disabled
- ✓ **Hot Configuration Reload** — SIGHUP-based reload via ArcSwap, lock-free reads, rate limiter rebuild on change
- ✓ **Comprehensive Observability** — circuit breaker transition metrics (from/to labels), PerformanceConfig, TelemetryConfig with validation

---

## Tier 5 — pdt.sh Ecosystem Integration

### IP Enrichment via ifconfig-rs (dns.pdt.sh + ip.pdt.sh)

Link prism results to ifconfig-rs for IP context. A/AAAA records are bare addresses today — enrichment turns them into actionable intelligence.

**Configuration**: New `[ecosystem]` config section with configurable service URLs. Users running their own ifconfig-rs instance (or no instance at all) can point to it instead of the public `ip.pdt.sh`. When `ifconfig_url` is unset, enrichment features are disabled and IP links are omitted. `ifconfig_api_url` allows a separate backend-to-backend URL (e.g., loopback or internal network) for inline enrichment fetches, avoiding the public rate-limited endpoint.

```toml
[ecosystem]
# Base URL for IP links in the frontend (omit to disable IP cross-links)
ifconfig_url = "https://ip.pdt.sh"
# Backend-to-backend URL for inline enrichment API calls (defaults to ifconfig_url)
# Use an internal address to bypass public rate limits.
ifconfig_api_url = "http://127.0.0.1:8081"
```

- **Clickable IPs**: A/AAAA record values link to `{ifconfig_url}/?ip=<addr>` (new tab). Only rendered when `ifconfig_url` is configured.
- **Inline enrichment**: Optional async fetch to `{ifconfig_api_url}/network/json?ip=<addr>` for inline badges: cloud provider (AWS/GCP/Azure/CF), network type (residential/datacenter/VPN/Tor), threat flags (Spamhaus, Feodo C2). Only active when `ifconfig_api_url` (or `ifconfig_url` as fallback) is configured.
- **Check mode lint**: New lint category "Infrastructure" — flag MX/NS records pointing at residential IPs, known bot ranges, or threat-listed addresses. Requires `ifconfig_api_url`.
- **Trace mode context**: Annotate each delegation hop's nameserver IPs with provider/location from ifconfig.
- **Reverse link from ifconfig**: ifconfig's PTR/host view links back to `dns.pdt.sh/?q=<hostname>` for full DNS analysis (configured on the ifconfig-rs side).
- **Graceful degradation**: All enrichment is best-effort. If the ifconfig instance is unreachable or slow (>500ms), skip enrichment silently — never block or delay DNS results for enrichment data.

---

## Ongoing / Cross-Cutting

- **Fix SDD drift** — sync `docs/done/sdd-2025-03-07.md` directory layout, API event schemas, and phase status with actual implementation.
- **CONTRIBUTING.md** — PR expectations, test requirements, commit style, how to run CI locally.
- **CHANGELOG.md** — maintain release notes per version.
- **SECURITY.md** — vulnerability disclosure process, dependency audit policy.
- **Print styles** — CSS `@media print` for results, lint, and trace views.
- **EDNS diagnostics** — surface EDNS buffer size, NSID, client subnet, and cookie support per resolver in detail panels.
- **DNS-over-QUIC** — add DoQ transport support when mhost-lib gains it (track upstream).
