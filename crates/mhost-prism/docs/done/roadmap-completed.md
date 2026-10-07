# Prism — Completed Roadmap

All tiers completed. This file is the archive of delivered work.

---

## Tier 1 — Quick Wins

- Expand all / Collapse all buttons in Results tab
- j/k navigation in all tabs (Results, Servers, Trace, DNSSEC), h/l for tab switching
- Result summary with agreement/divergence counts
- Lint remediation hints (contextual per category/result)
- Streaming progress context with cancel button
- Skeleton loaders for Results tab
- Divergence badge icons with aria-labels
- Touch target sizing (clear button, view buttons)
- Skip-to-content link
- Circuit breaker visibility (record type in messages, degraded_providers in done event)
- Rate limit feedback (scope field in 429 responses)
- Rate limiter memory documentation
- Deployment section in README (nginx, Caddy, security checklist, metrics)

---

## Tier 2 — Moderate Effort

- **Shareable Permalinks** — server-side LRU result cache, short cache keys, Share button in query bar, permalink copy
- **Export Options** — Copy Markdown, Download CSV, Download JSON (HTML report deferred)
- **Record Semantics Interpretation** — SPF, DMARC, SVCB/HTTPS, TLSA, NAPTR with human-readable formatting and explain toggle
- **Mobile Responsiveness** — breakpoints, touch targets, safe areas, horizontal scroll for tables
- **Integration Tests** — backend HTTP handler tests (axum::test), frontend vitest for tokenizer

---

## Tier 3 — Significant Features

- **Transport Comparison View** (`+compare`) — queries over UDP, TCP, DoT, DoH in parallel; 4-column side-by-side display; highlights transport-specific answer differences, latency, and failures; surfaces middlebox interference
- **Authoritative-vs-Recursive Split View** (`+auth`) — discovers authoritative NS via recursive lookup, queries both auth (RD=0) and recursive resolvers; 2-column comparison; reveals caching staleness, NXDOMAIN hijacking, split-horizon inconsistencies

---

## Tier 4 — Architectural Investments

Infrastructure integrated. Handler-level wiring deferred (see future work).

- **Connection Pooling** — TTL+LRU resolver cache keyed by (provider, transport), background cleanup task
- **Query Deduplication** — broadcast-based coalescing with deterministic QueryHash, RAII guard cleanup
- **OpenTelemetry Integration** — opt-in OTLP HTTP tracing, configurable sampling, zero overhead when disabled
- **Hot Configuration Reload** — SIGHUP-based reload via ArcSwap, lock-free reads, rate limiter rebuild on change
- **Comprehensive Observability** — circuit breaker transition metrics (from/to labels), PerformanceConfig, TelemetryConfig with validation

---

## Tier 5 — pdt.sh Ecosystem Integration

IP enrichment integrated via separate SSE `enrichment` event — DNS results stream immediately, enrichment data merges in after completion.

- **`[ecosystem]` config** — `ifconfig_url`, `ifconfig_api_url`, `enrichment_timeout_ms` (default 500ms, hard cap 2000ms)
- **IpEnrichmentService** — reqwest + moka cache (1024 entries, 5min TTL), private IP filtering, parallel batch lookups
- **SSE enrichment events** — sent after DNS batches in query, check, and trace endpoints; included in cached permalinks
- **Clickable IPs** — A/AAAA values and trace server IPs link to `{ifconfig_url}/?ip=<addr>`
- **Inline badges** — cloud provider, IP type (datacenter/residential), threat flags (Tor, VPN, Spamhaus, C2)
- **Infrastructure lint** — new check mode category flagging Spamhaus/C2 (failed), Tor/residential (warning)
- **Trace annotations** — server IPs show org name when enrichment available
- **Graceful degradation** — enrichment never blocks DNS results; all errors silently skipped
