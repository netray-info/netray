# CLAUDE.md — lens

## Rules

- Do NOT add a `Co-Authored-By` line for Claude in commit messages.
- **SCORING SYNC RULE**: Any change to the scoring algorithm (`src/scoring/`),
  profile schema, weight tiers, grade thresholds, or hard-fail rules MUST be
  reflected in `README.md` (the Scoring section) in the same commit. The README
  is the transparency contract — users must understand why a domain received a
  grade without reading source code.
- Don't add heavy dependencies for minor convenience.
- Don't mix formatting-only changes with functional changes in the same commit.
- Don't modify unrelated modules while you're in there.
- Don't add speculative flags, config options, or abstractions without a current caller.
- Don't bypass failing checks (`--no-verify`, `#[allow(...)]`) without explaining why.
- Don't include PII, real email addresses, or real domains (other than example.com) in test data, docs, or commits.
- If uncertain about an implementation detail, leave a concrete `TODO("reason")` rather than a hidden guess.

## Engineering Principles

- **KISS**: Simplest solution that works.
- **YAGNI**: Don't build for hypothetical future requirements.
- **Fail Fast**: Validate at boundaries, return errors early.
- **Secure by Default**: lens makes outbound HTTP calls to user-specified domains — target policy validation is load-bearing.
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

## Build & Test

```sh
make          # frontend + release binary
make dev      # cargo run (dev mode)
make test     # Rust + frontend tests
make ci       # lint + test + frontend
```

Live reference domain tests are gated behind `#[ignore]` and `LENS_LIVE_TESTS=1`.
