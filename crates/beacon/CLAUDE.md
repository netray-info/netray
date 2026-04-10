# CLAUDE.md -- beacon

## What this is

DNS-only email security inspector (`email.netray.info`), codename **beacon**. Fifth pillar in the netray suite: IP -> DNS -> TLS -> HTTP -> Email.

Given a domain, checks 12 email security categories (MX, SPF, DKIM, DMARC, MTA-STS, TLS-RPT, DANE, DNSSEC, BIMI, FCrDNS, DNSBL, cross-validation) and produces an aggregate grade A-F via SSE streaming.

## Architecture

Axum 0.8 service with embedded SolidJS 1.9 frontend. Follows suite patterns.

- `src/dns/resolver.rs` -- DnsResolver backed by `Vec<Resolver>` + `AtomicUsize` for round-robin (Send + Sync); built once at startup
- `src/checks/` -- 11 category check modules + cross_validation + orchestrator (mod.rs) + util (parse_tags)
- `src/checks/mod.rs` -- `run_all_checks` streams SSE via mpsc channel; three-phase execution with `JoinSet`
- `src/quality/` -- Verdict/Grade types, grade computation (0F/0W->A, 0F/1-2W->B, 0F/3+W->C, 1F->D, 2+F->F; Skip excluded)
- `src/input.rs` -- Domain validation (label rules, length limits) + DKIM selector validation
- `src/routes.rs` -- API handlers (`do_inspect` shared logic), health/ready endpoints
- `src/security/` -- IP extraction, rate limiting, security headers (delegates to netray-common)

### Key design decisions

- **DNS resolver**: Shared `DnsResolver` built at startup. `ResolverGroup::resolvers()` returns `&[Resolver]`; stored as `Vec<Resolver>` with round-robin via `AtomicUsize`. Each request calls `self.pick().lookup(MultiQuery::single(...))` which returns a `Send` future — no `spawn_blocking`, no `LocalSet`.
- **Three-phase SSE streaming**: Phase 0 (MX, SPF, DMARC, TLS-RPT, DNSSEC, BIMI) runs in parallel via `JoinSet`, emitting each result immediately to the SSE channel. Phase 1 (DKIM, MTA-STS, DANE, FCrDNS, DNSBL) runs in parallel after all Phase 0 tasks complete. Phase 2 (cross-validation, grade) runs sequentially and emits the Summary event.
- **30-second timeout**: `tokio::time::timeout` wraps `run_inspection_inner`; on expiry a partial Summary with `Verdict::Skip` is emitted.
- **DKIM selectors**: Static provider map (Google, Outlook, Amazon SES, Proofpoint, Mimecast) + user-supplied. No brute-force enumeration.
- **DNSSEC**: Uses RRSIG record presence as proxy (mhost doesn't expose AD bit).
- **MTA-STS**: No-redirect HTTP client per RFC 8461 section 3.3. Policy body capped at 64KB.
- **SPF expansion**: Recursive with depth cap (10), loop detection via visited set, void lookup counting (u16 + saturating_add).

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

- SDD: [`specs/done/sdd/beacon.md`](../specs/done/sdd/beacon.md)
- Performance/quality SDD: [`specs/done/sdd/beacon-review.md`](../specs/done/sdd/beacon-review.md)
- Apply [frontend-rules](../specs/rules/frontend-rules.md) when modifying `frontend/`
- Apply [logging-rules](../specs/rules/logging-rules.md) when modifying tracing/telemetry
- Apply [architecture-rules](../specs/rules/architecture-rules.md) for health probes and middleware
- Apply [workflow-rules](../specs/rules/workflow-rules.md) for CI/CD workflows
