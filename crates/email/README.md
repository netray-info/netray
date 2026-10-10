# beacon

DNS-only email security inspector for the [netray.info](https://netray.info) suite. Given a domain, beacon checks 12 email security categories (MX, SPF, DKIM, DMARC, MTA-STS, TLS-RPT, DANE, DNSSEC, BIMI, FCrDNS, DNSBL, cross-validation) and streams results incrementally via SSE, computing an aggregate grade A–F.

## Prerequisites

- Rust (pinned by the root `rust-toolchain.toml`)
- Node 20+
- `just adlc-setup` once per checkout, from the repository root (npm workspaces, frontend builds)

## Quick Start

```sh
# Backend (from the repository root, after `just build`)
netray email crates/email/beacon.dev.toml

# Frontend (separate terminal)
npm run dev -w beacon-frontend   # Vite dev server on :5176
```

Copy `beacon.toml.example` to `beacon.toml` and adjust settings as needed.

## Configuration

Beacon loads configuration from a TOML file and allows every value to be overridden via environment variables.

- **`beacon.toml.example`** — template, committed to the repo. Copy this as a starting point.
- **`beacon.toml`** — production config. Deployed to the server, not checked into the repo with real values.
- **`beacon.dev.toml`** — local development config. Used with `netray email crates/email/beacon.dev.toml`.

### Environment variables

Environment variables use the `BEACON__` prefix (two underscores) and `__` (double underscore) to traverse nested sections:

```sh
# [backends.ip] url = "..."
BEACON__BACKENDS__IP__URL=http://ip.netray.info

# [telemetry] level = "..."
BEACON__TELEMETRY__LEVEL=info
```

## API

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/inspect` | Start an inspection from a JSON body; streams SSE results |
| `GET`  | `/inspect/{domain}` | Start an inspection from a path parameter; streams SSE results |
| `GET`  | `/api/meta` | Ecosystem metadata (version, sibling services) |
| `GET`  | `/health` | Liveness probe |
| `GET`  | `/ready` | Readiness probe |
| `GET`  | `/docs` | Scalar UI over the OpenAPI schema |
| `GET`  | `/api-docs/openapi.json` | OpenAPI 3.1 schema |

### SSE example

```sh
curl -N https://email.netray.info/inspect/example.com
```

Each category emits its own SSE event as it completes; a final `summary` event carries the aggregate grade.

## Testing & Build

The verbs live in the root `justfile`; see the root `README.md`. For this crate alone:

```sh
cargo test -p netray-email               # Rust tests
npm test -w beacon-frontend        # frontend tests
```

## Architecture

Beacon is an Axum 0.8 service with an embedded SolidJS 1.9 frontend (built with Vite, served via rust-embed).

Inspection runs in three phases over SSE:
- **Phase 0** (parallel): MX, SPF, DMARC, TLS-RPT, DNSSEC, BIMI — domain-only checks, no MX dependency
- **Phase 1** (parallel, after Phase 0): DKIM, MTA-STS, DANE, FCrDNS, DNSBL — require MX hosts/IPs from Phase 0
- **Phase 2** (sequential): cross-validation, grade computation

Each check result is streamed to the client immediately on completion. The aggregate grade A–F is sent in a final `summary` SSE event. A shared `DnsResolver` (built once at startup using `Vec<Resolver>` + round-robin `AtomicUsize`) handles all DNS lookups without per-request resolver creation.
