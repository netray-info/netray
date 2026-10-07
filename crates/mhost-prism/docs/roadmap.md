# Prism — Future Work

Items deferred until there's a clear need or upstream support.

---

## Features

- **Query History with Temporal Diff** — server-side persistence (SQLite), timeline UI, snapshot diffing. Compelling but essentially a second product; revisit if monitoring use case emerges.
- **DNSSEC Expiry Timeline** — signature expiry dates, key rollover detection, validity window charts. Niche; existing chain-of-trust view covers most DNSSEC debugging.
- **EDNS diagnostics** — surface EDNS buffer size, NSID, client subnet, and cookie support per resolver. Blocked on mhost-lib upstream: `Response` struct doesn't expose EDNS data.
- **DNS-over-QUIC** — add DoQ transport support when mhost-lib gains it (track upstream).

## Infrastructure

### High Priority

- **Trusted CIDR / network-based access** — replace the current single trusted-IP config with a list of trusted CIDR ranges (e.g. `10.0.0.0/8`, `172.16.0.0/12`). Required for realistic deployment behind a private load balancer or within a cluster network where the egress IP is not a single stable address. Affects `config.rs` (add `trusted_networks: Vec<IpNet>`), `security/ip_extract.rs` (CIDR match instead of exact IP), and config documentation. Use the `ipnet` crate (already a transitive dep via `hickory-proto` — check before adding directly).

### Deferred

Handler-level wiring deferred from Tier 4 architectural investments:

- **Resolver pool wiring** — integrate TTL+LRU resolver cache into `build_resolver_group` (infrastructure exists, not yet wired)
- **Query dedup wiring** — integrate broadcast-based coalescing into SSE handlers (infrastructure exists, not yet wired)
- **Hot config reads** — wire ArcSwap hot_state reads into request handlers (infrastructure exists, not yet wired)

## Documentation

- **CONTRIBUTING.md** — PR expectations, test requirements, commit style, how to run CI locally.
- **CHANGELOG.md** — maintain release notes per version.
- **SECURITY.md** — vulnerability disclosure process, dependency audit policy.
- **Print styles** — CSS `@media print` for results, lint, and trace views.
