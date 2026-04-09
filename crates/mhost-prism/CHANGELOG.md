# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.8.4] - 2026-04-09

### Fixed
- Remove NODE_AUTH_TOKEN from .npmrc, use global auth (48649f8)
- Add SuiteNav compaction override (ef84d82)

### Changed
- Remove SuiteNav compaction override, now in common-frontend (a04d6b6)
- Move SuiteNav inside .app, remove stream-hint callout (ad9a8cb)
- Bump common-frontend to ^0.5.0 (73380d7)
- Bump common-frontend to ^0.4.0 (124017b)

## [0.8.3] - 2026-04-09

### Fixed
- Extract parseBatchEvent to separate module to fix vitest CI (6778f1b)

### Changed
- Move health and ready probes to root-level paths (33c0541)
- Condense CLAUDE.md rules and principles to avoid global duplication (f2984a7)
- Deduplicate frontend-rules and update spec paths in CLAUDE.md (9fced6f)

## [0.8.2] - 2026-04-09

### Fixed
- Use build_error_response() to eliminate double-logging (ada3aae)
- Record client_ip in span and add specific rejection logging (7931873)
- Log HTTP requests at INFO level with request_id (6f1c746)

## [0.8.1] - 2026-04-08

### Added
- Standardize default log filter (956614b)

### Fixed
- Add rate limit, proxy, and circuit breaker config to startup inventory (830e30a)
- Correct deploy webhook URL to deploy-prism (4768d00)

## [0.8.0] - 2026-04-08

### Added
- Implement MTA-STS policy file fetch and validation (2d78774)

### Fixed
- Add aria-label to query clear button (d1c04b7)

### Changed
- Add lint script (tsc --noEmit) to frontend CI (5bac3f3)
- Align workflows with netray.info workflow-rules spec (241f4f2)

## [0.7.0] - 2026-04-08

### Added
- Primary button uses shared `.btn-primary` from common-frontend; replace DnsCrossLinks with shared CrossLink component (ab52d50)
- Landing mode cards adopt shared `.mode-card` classes from common-frontend (6db3ebd)

## [0.6.1] - 2026-04-08

### Changed
- Bump rand 0.9→0.10, typescript 5→6, vite 7→8 (5fe374d, d990b17)

## [0.6.0] - 2026-04-07

### Added
- BIMI, MTA-STS, TLSRPT checks; deep links; mobile UX; trace ID propagation (cbc3cd7)
- MIT LICENSE file (f535ee9)

### Fixed
- Render MTA-STS, TLSRPT, BIMI TXT records as human-readable text (3bfc708)

### Changed
- Frontend: use shared SuiteNav and history factory from common-frontend (d59aeb5)
- Frontend: bump @netray-info/common-frontend to ^0.3.0 (51eaccc)
- Add human-readable docs link to OpenAPI description; CI integration examples in README (0ad52f2)

## [0.5.2] - 2026-04-07

### Fixed
- Relax common-frontend version to ^0.2.1 (6aed9f7)
- Upgrade vite 7.3.1 → 7.3.2 (CVE dev-server vulns) (359c064)

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
