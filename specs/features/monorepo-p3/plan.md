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
