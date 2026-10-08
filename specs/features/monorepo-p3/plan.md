# Plan: contract deliverables

## Phase 1 — Config loader and check

### Groups

| Group | Criteria | Depends on |
|---|---|---|
| G1 | C5, C12 | — |
| G2 | C1, C2, C6, C7, C8, C9 | — |
| G3 | C3, C4, C10, C11 | G2 |

### Plan

**G1.** `crates/common/src/ip_extract.rs`: drop the `CF-Connecting-IP` step from `IpExtractor::extract`, delete `extract_cf_connecting_ip`, fix the doc comments; rewrite the unit tests that asserted CF priority.

**G2.**
- `crates/common/Cargo.toml`: `config` as a dependency.
- `crates/common/src/config.rs` (new), `pub mod config` in `lib.rs`: `load` / `load_with_env` over `vars_os()`, dropping non-UTF-8 entries, keeping only `prefix` keys, dropping `<NAME>_CONFIG`, file `required(true)`, `Environment::with_prefix(NAME).prefix_separator(rest).separator("__")`.
- `deny_unknown_fields` on common `BackendConfig`, `EcosystemConfig`, `TelemetryConfig` and every struct in prism, tlsight and ifconfig-rs `config.rs`.
- Each service's `Config::load` delegates to the common loader (`PRISM_`, `TLSIGHT_`, `IFCONFIG_`, `LENS_`, `SPECTRA__`, `BEACON__`), then its own `validate()`. lens drops `config_env` and keeps only a `LENS_LIVE_TESTS` filter; spectra's `load_with_env` takes env pairs; beacon keeps its signature and drops `StrictEcosystemConfig`.
- `prism.dev.toml` and `tlsight.dev.toml`: rename the unknown `[ecosystem]` keys to `*_base_url`.

**G3.** `crates/netray/src/main.rs`: `--check-config <PATH>` on the six service subcommands; a helper prints `config ok: <path>` and exits 0, or the error to stderr and exits 1, before any `run`. `ip --check` keeps its wider meaning (config plus data files); `--check-config` checks the config only.

## Phase 2 — Header and CORS parity

### Groups

| Group | Criteria | Depends on |
|---|---|---|
| G1 | C1, C2, C4, C5, C6, C7 | — |
| G2 | C3, C9 | G1 |
| G3 | C8 (production run, no files) | G2 |

### Plan

**G1.**
- **`crates/common/src/security_headers.rs`:**
  - `SecurityHeadersConfig.hsts`, default `max-age=31536000; includeSubDomains; preload`.
  - `include_permissions_policy` is dropped.
  - The strict CSP is exactly `csp-tool-spa`; the relaxed CSP appends `extra_script_src` to its `script-src`.
  - Permissions-Policy, COOP `same-origin` and CORP `cross-origin` are set on every response.
- **`crates/common/src/cors.rs`:** `cors_layer` allows any origin, methods `GET, POST, OPTIONS`, headers `content-type, accept`, max-age 600.
- **Service layer order** (lens, prism, tlsight, spectra, beacon, ifconfig-rs):
  - Security headers outermost, then CORS, so 413, 429, 304 and preflight responses carry them.
  - lens `/docs` moves inside the layers and gets the jsDelivr allowance.
  - beacon gains the CORS layer it lacked.
- **ifconfig-rs:**
  - `ifconfig_response_headers` loses its HSTS and CSP rewrites.
  - `build_app` sets `hsts = max-age=63072000; includeSubDomains; preload` and uses `cors_layer()`.

**G2.** A `just acceptance-local` recipe starts `netray site` and the six services on the `env.ts` local ports, then runs the header and assets specs with `TEST_ENV=local`.

**G3.** Run the two acceptance specs against production and record the result.

Prose (`architecture-rules.md` §Security Headers, `crates/common/CLAUDE.md`, `crates/ifconfig-rs/CLAUDE.md`, root `CLAUDE.md` verbs) goes in the spec's closing docs commit.

## Phase 3 — lens snapshot 404

### Groups

| Group | Criteria | Depends on |
|---|---|---|
| G1 | C1, C2, C3, C4 | — |

### Plan

**G1.** `crates/lens/src/routes.rs`: `snapshot_handler` answers a malformed shortid with `not_found_html()` instead of 400 JSON; `not_found_html` says the snapshot is expired or unknown. Expiry is already enforced by `SnapshotStore::get`.
