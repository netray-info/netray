# beacon

DNS-only email security inspector for the [netray.info](https://netray.info) suite. Given a domain, beacon checks 12 email security categories (MX, SPF, DKIM, DMARC, MTA-STS, TLS-RPT, DANE, DNSSEC, BIMI, FCrDNS, DNSBL, cross-validation) and streams results incrementally via SSE, computing an aggregate grade A–F.

## Prerequisites

- Rust 1.85+
- Node 20+
- `NODE_AUTH_TOKEN` environment variable set to a GitHub personal access token with `read:packages` scope (required to install `@netray-info/common-frontend` from GitHub Packages)

## Quick Start

```sh
# Backend
cargo run -- --config beacon.toml

# Frontend (separate terminal)
cd frontend
npm install
npm run dev   # Vite dev server on :5176
```

Copy `beacon.toml.example` to `beacon.toml` and adjust settings as needed.

## Architecture

Beacon is an Axum 0.8 service with an embedded SolidJS 1.9 frontend (built with Vite, served via rust-embed).

Inspection runs in three phases over SSE:
- **Phase 0** (parallel): MX, SPF, DMARC, TLS-RPT, DNSSEC, BIMI — domain-only checks, no MX dependency
- **Phase 1** (parallel, after Phase 0): DKIM, MTA-STS, DANE, FCrDNS, DNSBL — require MX hosts/IPs from Phase 0
- **Phase 2** (sequential): cross-validation, grade computation

Each check result is streamed to the client immediately on completion. The aggregate grade A–F is sent in a final `summary` SSE event. A shared `DnsResolver` (built once at startup using `Vec<Resolver>` + round-robin `AtomicUsize`) handles all DNS lookups without per-request resolver creation.
