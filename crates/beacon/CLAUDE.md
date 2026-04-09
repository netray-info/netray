# CLAUDE.md -- beacon

## What this is

DNS-only email security inspector (`email.netray.info`), codename **beacon**. Fifth pillar in the netray suite: IP -> DNS -> TLS -> HTTP -> Email.

Given a domain, checks 12 email security categories (MX, SPF, DKIM, DMARC, MTA-STS, TLS-RPT, DANE, DNSSEC, BIMI, FCrDNS, DNSBL, cross-validation) and produces an aggregate grade A-F via SSE streaming.

## Architecture

Axum 0.8 service with embedded SolidJS 1.9 frontend. Follows suite patterns.

- `src/dns/resolver.rs` -- DnsResolver wrapping mhost ResolverGroup (per-request, !Send)
- `src/checks/` -- 11 category check modules + cross_validation + orchestrator (mod.rs)
- `src/checks/mod.rs` -- `run_all_checks` uses `spawn_blocking` + current-thread runtime + `LocalSet` to isolate mhost's !Send futures from axum's Send requirement
- `src/quality/` -- Verdict/Grade types, grade computation (0F/0W->A, 0F/1-2W->B, 0F/3+W->C, 1F->D, 2+F->F)
- `src/input.rs` -- Domain validation (label rules, length limits)
- `src/routes.rs` -- API handlers, health/ready endpoints
- `src/security/` -- IP extraction, rate limiting, security headers (delegates to netray-common)

### Key design decisions

- **!Send workaround**: mhost's `ResolverGroup::lookup()` returns !Send futures (internal `Rc<ThreadRng>`). Solved by creating a fresh DnsResolver per request inside `tokio::task::spawn_blocking` with a `tokio::runtime::Builder::new_current_thread()` runtime + `LocalSet`. This isolates all DNS work on a dedicated thread where !Send is fine.
- **DKIM selectors**: Static provider map (Google, Outlook, Amazon SES, Proofpoint, Mimecast) + user-supplied. No brute-force enumeration.
- **DNSSEC**: Uses RRSIG record presence as proxy (mhost doesn't expose AD bit).
- **MTA-STS**: No-redirect HTTP client per RFC 8461 section 3.3.
- **SPF expansion**: Recursive with depth cap (10), loop detection via visited set, void lookup counting.

## Config

TOML file `beacon.toml` + env overrides with `BEACON_` prefix (`__` for nesting).

## Development

```sh
cargo check                          # fast compile check
cargo test                           # run all tests
cargo clippy -- -D warnings          # lint
cd frontend && npm install           # install frontend deps (needs NODE_AUTH_TOKEN)
cd frontend && npm run dev           # Vite dev server on :5176
cd frontend && npm run build         # production build into dist/
```

## Specs

- SDD: [`specs/sdd/beacon.md`](../specs/sdd/beacon.md)
- Apply [frontend-rules](../specs/rules/frontend-rules.md) when modifying `frontend/`
- Apply [logging-rules](../specs/rules/logging-rules.md) when modifying tracing/telemetry
- Apply [architecture-rules](../specs/rules/architecture-rules.md) for health probes and middleware
- Apply [workflow-rules](../specs/rules/workflow-rules.md) for CI/CD workflows
