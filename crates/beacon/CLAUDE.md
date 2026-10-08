# CLAUDE.md -- beacon

## What this is

DNS-only email security inspector (`email.netray.info`), codename **beacon**. Fifth pillar in the netray suite: IP -> DNS -> TLS -> HTTP -> Email.

Given a domain, checks 12 email security categories (MX, SPF, DKIM, DMARC, MTA-STS, TLS-RPT, DANE, DNSSEC, BIMI, FCrDNS, DNSBL, cross-validation) and produces an aggregate grade A-F via SSE streaming.

## Architecture

Axum 0.8 service with embedded SolidJS 1.9 frontend. Follows suite patterns.

- `src/dns/resolver.rs` -- DnsResolver backed by `Vec<Resolver>` + `AtomicUsize` for round-robin (Send + Sync); built once at startup
- `src/checks/` -- 11 category check modules + cross_validation + orchestrator (mod.rs) + util (parse_tags) (11 category checks + `cross_validation` = 12 categories total)
- `src/checks/mod.rs` -- `run_all_checks` streams SSE via mpsc channel; three-phase execution with `JoinSet`
- `src/quality/` -- Verdict/Grade types, grade computation (0F/0W->A, 0F/1-2W->B, 0F/3+W->C, 1F->D, 2+F->F; Skip excluded)
- `src/input.rs` -- Domain validation (label rules, length limits) + DKIM selector validation
- `src/routes.rs` -- API handlers (`do_inspect` shared logic), health/ready endpoints
- `src/security/` -- IP extraction, rate limiting, security headers (delegates to netray-common)

### Key design decisions

- **DNS resolver**: Shared `DnsResolver` built at startup. `ResolverGroup::resolvers()` returns `&[Resolver]`; stored as `Vec<Resolver>` with round-robin via `AtomicUsize`. Each request calls `self.pick().lookup(MultiQuery::single(...))` which returns a `Send` future — no `spawn_blocking`, no `LocalSet`. A **second** `DnsResolver` (`dnsbl_resolver`, configured via `[dnsbl] resolvers`, default `["system"]`) is used only for DNSBL queries, because most public DNSBLs (Spamhaus) block queries from public/open resolvers and return synthetic `127.255.255.x` responses.
- **Three-phase SSE streaming**: Phase 0 (MX, SPF, DMARC, TLS-RPT, DNSSEC, BIMI) runs in parallel via `JoinSet`, emitting each result immediately to the SSE channel. Phase 1 (DKIM, MTA-STS, DANE, FCrDNS, DNSBL) runs in parallel after all Phase 0 tasks complete. Phase 2 (cross-validation, grade) runs sequentially and emits the Summary event.
- **30-second timeout**: `tokio::time::timeout` wraps `run_inspection_inner`. 30s timeout: on expiry, all 12 category verdicts are emitted as `Verdict::Skip` and `Grade::Skipped` is reported. The frontend renders this as grade badge 'Skipped'.
- **DKIM selectors**: Static provider map (Google, Outlook, Amazon SES, Proofpoint, Mimecast) + user-supplied. No brute-force enumeration.
- **DNSSEC**: Uses DNSKEY record presence as proxy (mhost doesn't expose AD bit).
- **MTA-STS**: No-redirect HTTP client per RFC 8461 section 3.3. Policy body capped at 64KB.
- **SPF expansion**: Recursive with depth cap (10), loop detection via visited set, void lookup counting (u16 + saturating_add).

## Config

TOML file + env overrides with `BEACON__` prefix (`__` for nesting, e.g.
`BEACON__SERVER__BIND`). The file is argv[1], else `$BEACON_CONFIG`, else
`beacon.toml` in the working directory; the `starting beacon` log line names
the path and source. A deployment that passes no argument lets
`BEACON_CONFIG` win over `beacon.toml` in the working directory.

Every config struct is `#[serde(deny_unknown_fields)]`: an unknown key in the
file or a `BEACON__*` env var fails startup instead of silently disabling a
feature; `[ecosystem]` uses netray-common's `EcosystemConfig`, strict as well.
`tests/fixtures/beacon.production.toml` pins the shape the argus-oci template
must render. `netray email --check-config <path>` validates a file, including
the values startup rejects (unparsable or zero `per_ip`), and exits 0
(`config ok: <path>`) or 1 with the error.

## Development

The verbs live in the root `justfile` (see the root `README.md`); run them from the repository root.

```sh
just adlc-verify                     # the gate, offline
cargo test -p beacon                 # this crate's tests only
npm run dev -w beacon-frontend       # Vite dev server on :5176
netray email crates/beacon/beacon.dev.toml   # run the service (after `just build`)
```

The Rust build embeds `frontend/dist` via RustEmbed and does not compile without
it; `dist` is gitignored, so `just adlc-setup` has to run once after a clone.

## Specs

- Performance/quality SDD: [`specs/sdd/beacon-review.md`](../specs/sdd/beacon-review.md)
- Apply [frontend-rules](../../specs/rules/frontend-rules.md) when modifying `frontend/`
- Apply [logging-rules](../../specs/rules/logging-rules.md) when modifying tracing/telemetry
- Apply [architecture-rules](../../specs/rules/architecture-rules.md) for health probes and middleware
- Apply [workflow-rules](../../specs/rules/workflow-rules.md) for CI/CD workflows
