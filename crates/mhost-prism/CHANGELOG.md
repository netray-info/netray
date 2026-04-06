# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.5.1] - 2026-04-06

### Added
- Default theme to system preference (cb438a6)

### Changed
- Bump @netray-info/common-frontend to 0.2.2 (2e076f6)
- Update Cargo.lock; omit dev deps from npm audit (7488dea)

## [0.5.0] - 2026-04-06

### Added
- Suite nav, DNS→TLS cross-links, stream timeout feedback, Cache-Control headers, OpenAPI CORS note (f3c6d7c)
- Favicon with prism motif in purple (229e5f8)
- Explicit robots.txt route returning text/plain before SPA fallback (f3c6d7c)

### Changed
- border-radius aligned to 8px across suite (f3c6d7c)
- Add deploy task for release pipeline (5459467)

## [0.3.0] - 2026-03-14

### Added

- Server group aliases: `@public` (Google + Cloudflare + Quad9), `@cloudflare` (1.1.1.1 + 1.0.0.1), `@google` (8.8.8.8 + 8.8.4.4), `@quad9` (9.9.9.9 + 149.112.112.112), `@all` (all public resolvers, capped to 4); `ParsedQuery` gains `truncated_servers: bool`
- `+norecurse` flag: sets RD=0 for non-recursive queries; `ParsedQuery` gains `recursive: bool`
- `+short` flag: suppresses TTL display in output
- Non-streaming JSON mode: all endpoints accept `?stream=false` or `Accept: application/json`; returns `{ "events": [...], "truncated": bool }` collected server-side
- `/api/results/{key}` permalink endpoint: retrieve a cached result by its share key
- Check mode — NS lame delegation: validates AA bit per nameserver
- Check mode — delegation-consistent NS: diffs parent vs. child NS sets and reports discrepancies
- Check mode — DNSSEC rollover detection: identifies multiple KSKs, orphaned DS records, and missing DS for a new KSK
- Check mode — DNSKEY algorithm security rating: flags RSA/MD5 and RSA/SHA-1 as deprecated
- Frontend: TTL countdown — displayed TTLs decrement live in the browser after records arrive
- Frontend: SOA serial age badge — shows relative age (e.g. "45 days ago") next to the SOA serial number
- Frontend: trace latency heatmap — per-hop coloured latency bars in Trace view
- Frontend: server latency summary — median latency per resolver in the Servers tab
- Frontend: TTL-only divergence shown in amber (data divergence remains red)
- Semaphore-bounded fan-out to cap concurrent DNS queries per request
- `/api/ready` readiness probe (503 when a circuit breaker is open)
- `tls_url` ecosystem config option for cross-links to tlsight
- Bumped `netray-common` to v0.4.1

### Changed

- Migrated frontend to `@netray-info/common-frontend` v0.2.0 (shared theme, hooks, CSS custom properties)
- Migrated backend to `netray-common` v0.3.0+ (shared enrichment, telemetry, CORS, middleware, IP filter)
- Removed `query_dedup` and `resolver_pool` modules (superseded by simpler per-request resolver construction)

### Fixed

- SSRF blocklist: additional private ranges blocked; glue IPs from delegation walk validated through `is_allowed_target`
- Schema feature flag and OpenAPI doc corrections
- Accessibility fixes in frontend components

## [0.1.3] - 2026-03-11

### Fixed

- Serve `index.html` with `text/html` content-type for root path (`/`); previously `mime_guess` fell back to `application/octet-stream` causing browsers to download instead of render the SPA

## [0.1.2] - 2026-03-11

### Fixed

- Dockerfile: replace `addgroup`/`adduser` with `groupadd`/`useradd` for Ubuntu runtime image

## [0.1.1] - 2026-03-11

### Fixed

- Docker release workflow: add `tags` field to `build-push-action` so push-by-digest succeeds

### Changed

- Roadmap: add trusted CIDR network access as high-priority infrastructure item

## [0.1.0] - 2026-03-11

### Added

- Multi-resolver fan-out query endpoint (`/api/query`) with SSE streaming
- Check mode: 15-type DNS audit plus DMARC lint (`/api/check`)
- Trace mode: iterative delegation walk from root to authoritative (`/api/trace`)
- Compare mode: simultaneous UDP/TCP/TLS/HTTPS transport comparison (`/api/compare`)
- Auth mode: authoritative vs recursive comparison (`/api/authcompare`)
- DNSSEC mode: chain-of-trust fetch and display (`/api/dnssec`)
- GCRA rate limiting: three-tier per-IP, per-target, and global token buckets
- Per-provider sliding-window circuit breaker
- IP enrichment via ifconfig-compatible API with in-memory cache
- OpenTelemetry distributed tracing (OTLP/HTTP export, optional)
- Prometheus metrics endpoint
- Hot config reload via SIGHUP
- Result cache (LRU) for permalink sharing
- Query deduplication via broadcast coalescing
- Embedded SPA (SolidJS + TypeScript strict mode)
- CodeMirror 6 query input with tab-complete autocomplete
- Streaming results table with expand/collapse per record type
- Query history dropdown
- Interactive API docs at `/docs` (Scalar UI) and OpenAPI spec at `/api-docs/openapi.json`
