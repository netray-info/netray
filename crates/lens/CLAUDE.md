# CLAUDE.md — lens

## Tool-specific rules

- **SCORING SYNC RULE**: Any change to the scoring algorithm (`src/scoring/`),
  profile schema, weight tiers, grade thresholds, or hard-fail rules MUST be
  reflected in `README.md` (the Scoring section) in the same commit. The README
  is the transparency contract.
- **Secure by Default**: lens makes outbound HTTP calls to user-specified domains -- target policy validation is load-bearing.
- **Partial failure is expected**: one backend down must never block the others.

## Project Overview

**lens** is the unified domain health check service for the netray.info suite
(`lens.netray.info`). It takes a domain, calls mhost-prism (DNS), tlsight (TLS),
and ifconfig-rs (IP) in parallel, and streams aggregated results via SSE with a
letter-graded health score.

- DNS + TLS run in parallel via `tokio::join!`
- IP enrichment runs after DNS (needs resolved IPs)
- Results stream as SSE events: `dns`, `tls`, `ip`, `summary`, `done`
- 20s hard deadline — return whatever is ready
- Scoring driven by external TOML profile (embedded default, file override)

## Architecture

```
lens/
  Cargo.toml
  Makefile
  Dockerfile
  lens.example.toml
  profiles/
    default.toml          # Default scoring profile (embedded at compile time)
  src/
    main.rs               # Entry point, Axum server, graceful shutdown
    config.rs             # TOML + LENS_ env vars
    error.rs              # AppError enum → HTTP status + error codes
    input.rs              # Domain validation (no IPs, no wildcards, max 253 chars)
    state.rs              # AppState (config, rate limiters, reqwest client, cache, profile)
    routes.rs             # Route definitions + handlers
    check.rs              # Orchestration: parallel DNS+TLS, then IP; SSE streaming
    backends/
      mod.rs
      dns.rs              # Call prism, parse CollectedResponse, extract lint + IPs
      tls.rs              # Call tlsight, parse quality checks
      ip.rs               # Call ifconfig-rs /batch, map network.type to verdict
    scoring/
      mod.rs
      profile.rs          # Profile, SectionProfile structs (serde from TOML)
      engine.rs           # compute_section, compute_overall, apply_hard_fails
    cache.rs              # moka TTL cache keyed by domain
    security/
      mod.rs
      rate_limit.rs       # GCRA per-IP + global
      ip_extract.rs       # Client IP from proxy headers
      target_policy.rs    # No RFC1918, no IPs, domain-only
  frontend/               # SolidJS + Vite (strict TypeScript)
```

## Scoring Algorithm

See README.md — Scoring section. That is the authoritative description.
Any code change to scoring must update README.md in the same commit.

## Key Dependencies

- `netray-common` 0.5 — IP extraction, security headers, GCRA wrappers, request IDs
- `axum` 0.8 — Web framework
- `reqwest` 0.12 — Backend HTTP calls (rustls-tls, no OpenSSL)
- `moka` 0.12 — In-process TTL cache
- `tokio-stream` + `futures` — SSE streaming
- `config` — TOML + env var layering (LENS_ prefix, __ separator)
- `toml` — Scoring profile deserialization

## Frontend Rules

Full spec: [`specs/rules/frontend-rules.md`](../specs/rules/frontend-rules.md) in the netray.info meta repo. Apply when modifying anything under `frontend/`.

## Architecture Rules

Rules: [`specs/rules/architecture-rules.md`](../specs/rules/architecture-rules.md) in the netray.info meta repo. Apply when modifying health probes or readiness checks.

## Logging & Telemetry

Rules: [`specs/rules/logging-rules.md`](../specs/rules/logging-rules.md) in the netray.info meta repo. Follow those rules when modifying tracing init, log filters, or `[telemetry]` config.

Default filter: `info,lens=debug,hyper=warn,h2=warn`. Telemetry config via `[telemetry]` section or `LENS_TELEMETRY__*` env vars. Production uses `log_format = "json"` and `service_name = "lens"`.

## CI/CD

Workflow rules: [`specs/rules/workflow-rules.md`](../specs/rules/workflow-rules.md) in the netray.info meta repo. Follow those rules when creating or modifying any `.github/workflows/*.yml` file.

Workflows: `ci.yml` (PR gate: fmt, clippy, test, frontend, audit), `release.yml` (tag-push: test → build → merge), `deploy.yml` (fires after release via webhook).

## Build & Test

```sh
make          # frontend + release binary
make dev      # cargo run (dev mode)
make test     # Rust + frontend tests
make ci       # lint + test + frontend
```

Live reference domain tests are gated behind `#[ignore]` and `LENS_LIVE_TESTS=1`.
