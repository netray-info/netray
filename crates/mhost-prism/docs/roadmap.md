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

## Tier 3 — Significant Features ✓

All items completed.

- ✓ **Transport Comparison View** (`+compare`) — queries over UDP, TCP, DoT, DoH in parallel; 4-column side-by-side display; highlights transport-specific answer differences, latency, and failures; surfaces middlebox interference
- ✓ **Authoritative-vs-Recursive Split View** (`+auth`) — discovers authoritative NS via recursive lookup, queries both auth (RD=0) and recursive resolvers; 2-column comparison; reveals caching staleness, NXDOMAIN hijacking, split-horizon inconsistencies

---

## Future

Items deferred until there's a clear need or upstream support.

- **Query History with Temporal Diff** — server-side persistence (SQLite), timeline UI, snapshot diffing. Compelling but essentially a second product; revisit if monitoring use case emerges.
- **DNSSEC Expiry Timeline** — signature expiry dates, key rollover detection, validity window charts. Niche; existing chain-of-trust view covers most DNSSEC debugging.
- **EDNS diagnostics** — surface EDNS buffer size, NSID, client subnet, and cookie support per resolver. Blocked on mhost-lib upstream: `Response` struct doesn't expose EDNS data.
- **DNS-over-QUIC** — add DoQ transport support when mhost-lib gains it (track upstream).

---

## Tier 4 — Architectural Investments (months) ✓

Infrastructure integrated. Handler-level wiring (resolver pool into build_resolver_group, query dedup into SSE handlers, hot_state reads in handlers) deferred as follow-up.

- ✓ **Connection Pooling** — TTL+LRU resolver cache keyed by (provider, transport), background cleanup task
- ✓ **Query Deduplication** — broadcast-based coalescing with deterministic QueryHash, RAII guard cleanup
- ✓ **OpenTelemetry Integration** — opt-in OTLP HTTP tracing, configurable sampling, zero overhead when disabled
- ✓ **Hot Configuration Reload** — SIGHUP-based reload via ArcSwap, lock-free reads, rate limiter rebuild on change
- ✓ **Comprehensive Observability** — circuit breaker transition metrics (from/to labels), PerformanceConfig, TelemetryConfig with validation

---

## Tier 5 — pdt.sh Ecosystem Integration ✓

### IP Enrichment via ifconfig-rs (dns.pdt.sh + ip.pdt.sh) ✓

Implemented. IP enrichment integrated via separate SSE `enrichment` event — DNS results stream immediately, enrichment data merges in after completion.

- ✓ **`[ecosystem]` config** — `ifconfig_url`, `ifconfig_api_url`, `enrichment_timeout_ms` (default 500ms, hard cap 2000ms)
- ✓ **IpEnrichmentService** — reqwest + moka cache (1024 entries, 5min TTL), private IP filtering, parallel batch lookups
- ✓ **SSE enrichment events** — sent after DNS batches in query, check, and trace endpoints; included in cached permalinks
- ✓ **Clickable IPs** — A/AAAA values and trace server IPs link to `{ifconfig_url}/?ip=<addr>`
- ✓ **Inline badges** — cloud provider, IP type (datacenter/residential), threat flags (Tor, VPN, Spamhaus, C2)
- ✓ **Infrastructure lint** — new check mode category flagging Spamhaus/C2 (failed), Tor/residential (warning)
- ✓ **Trace annotations** — server IPs show org name when enrichment available
- ✓ **Graceful degradation** — enrichment never blocks DNS results; all errors silently skipped
- Deferred: **Reverse link from ifconfig** (configured on the ifconfig-rs side, not prism)

---

## Ongoing / Cross-Cutting

- **Fix SDD drift** — sync `docs/done/sdd-2025-03-07.md` directory layout, API event schemas, and phase status with actual implementation.
- **CONTRIBUTING.md** — PR expectations, test requirements, commit style, how to run CI locally.
- **CHANGELOG.md** — maintain release notes per version.
- **SECURITY.md** — vulnerability disclosure process, dependency audit policy.
- **Print styles** — CSS `@media print` for results, lint, and trace views.
