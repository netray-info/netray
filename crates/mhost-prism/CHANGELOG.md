# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

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
