# Plan: v2 modules

## Phase 0 — Baseline

## Groups

G1: C1–C3 — no production code; the goldens are written by the test with `UPDATE_GOLDEN=1` from the 0.23.1 code.

## Plan

### G1
- `tests/fixtures/contracts/lens-full-*.json` (10): generated, committed.

## Phase 1 — HTTP module

## Groups

G1: C1 (engine half), C5 · G2: C2, C3, C6, C7, C8 · G3: C1 (lens half), C4, C9, C10

## Plan

### G1 — engine
- `crates/engine/Cargo.toml`: `serde_json`.
- `crates/engine/src/lib.rs`: `RunContext { deadline, domain, options }`, `RunOptions`, `SectionOutcome::Measured { checks, presentation }`, `Registry` (`new`, `with` — a second module for one protocol replaces the first —, `with_facts`, `module`, `facts`).

### G2 — netray-http
- `crates/common/src/config.rs`: `refuse_legacy_prefix(legacy, new, env)` naming the new prefix and `<new>CONFIG`; a loader test.
- `crates/http/src/config.rs`: refuse `SPECTRA_`, load `NETRAY_HTTP_`; `ModuleConfig { inspect, enrichment }` (`deny_unknown_fields`).
- `crates/http/src/input.rs`: `pick_target(addrs, port)` out of `validate_target`.
- `crates/http/src/inspect/mod.rs`: one pub fn for inspect + enrichment + assemble, shared by the route and the module.
- `crates/http/src/module.rs`: `translate` (verbatim port of lens `parse_inspect` and helpers, six checks in order), `HttpModule` (`Facts` addresses through `pick_target`; empty `Facts` falls back to `validate_target` until the engine run supplies them), errors → `Incomplete`; ported lens unit tests.
- `crates/http/src/testing.rs`: `golden_module`; `lib.rs` exports, `run()` reads `NETRAY_HTTP_CONFIG`.
- `crates/http/Cargo.toml`: `netray-engine`, feature `testing`, `[[test]] module required-features = ["testing"]`, dev `toml`.
- Docs: CLAUDE.md "Config stays per service" amended, architecture-rules, crate README/CLAUDE.md.

### G3 — lens and the binary
- `crates/lens/Cargo.toml`: `netray-engine`; dev `netray-http` with `testing`.
- `crates/lens/src/config.rs`: `modules: BTreeMap<String, toml::Table>`; `validate()` refuses `backends.http.url`; budget over `timeout_ms`.
- `crates/lens/src/modules.rs`: `ModuleSection` adapter implementing lens's `Backend` (RunContext from the request, lens's own timeout; `Measured` → lens checks + `BackendExtra::Http` from presentation, `detail_url` as before; `Incomplete` → `BackendError`; `NotApplicable` → `NotApplicable`; non-pass statuses back to V1 verdicts).
- `crates/lens/src/state.rs`: `with_registry`; HTTP section when `[backends.http]` and the module exist; `new` = empty registry.
- `crates/lens/src/backends/http.rs` deleted; `lib.rs` `run_with`; `routes.rs` `/ready` without the http probe; lens example/dev configs.
- `crates/netray`: `lens_registry(&Config)` (unknown module tables refused; `[modules.http]` → `ModuleConfig`, errors prefixed `modules.http:`), `run_with`, `--check-config` builds it.
