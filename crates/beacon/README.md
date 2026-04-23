# beacon

DNS-only email security inspector for the [netray.info](https://netray.info) suite. Given a domain, beacon checks 12 email security categories (MX, SPF, DKIM, DMARC, MTA-STS, TLS-RPT, DANE, DNSSEC, BIMI, FCrDNS, DNSBL, cross-validation) and streams results incrementally via SSE, computing an aggregate grade A–F.

## Prerequisites

- Rust 1.85+
- Node 20+
- `NODE_AUTH_TOKEN` environment variable set to a GitHub personal access token with `read:packages` scope (required for `npm install` to fetch `@netray-info/common-frontend` from GitHub Packages via `npm.pkg.github.com`)

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

## Configuration

Beacon loads configuration from a TOML file and allows every value to be overridden via environment variables.

- **`beacon.toml.example`** — template, committed to the repo. Copy this as a starting point.
- **`beacon.toml`** — production config. Deployed to the server, not checked into the repo with real values.
- **`beacon.dev.toml`** — local development config. Used with `cargo run -- --config beacon.dev.toml`.

### Environment variables

Environment variables use the `BEACON_` prefix and `__` (double underscore) to traverse nested sections:

```sh
# [backends.ip] url = "..."
BEACON_BACKENDS__IP__URL=http://ip.netray.info

# [telemetry] level = "..."
BEACON_TELEMETRY__LEVEL=info
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

Common tasks are exposed via the Makefile:

```sh
make           # default build (cargo + frontend)
make test      # run Rust and frontend tests
make dev       # run backend + frontend with dev configs
make pre-push  # full lint + test gate run before pushing
```

## Architecture

Beacon is an Axum 0.8 service with an embedded SolidJS 1.9 frontend (built with Vite, served via rust-embed).

Inspection runs in three phases over SSE:
- **Phase 0** (parallel): MX, SPF, DMARC, TLS-RPT, DNSSEC, BIMI — domain-only checks, no MX dependency
- **Phase 1** (parallel, after Phase 0): DKIM, MTA-STS, DANE, FCrDNS, DNSBL — require MX hosts/IPs from Phase 0
- **Phase 2** (sequential): cross-validation, grade computation

Each check result is streamed to the client immediately on completion. The aggregate grade A–F is sent in a final `summary` SSE event. A shared `DnsResolver` (built once at startup using `Vec<Resolver>` + round-robin `AtomicUsize`) handles all DNS lookups without per-request resolver creation.
