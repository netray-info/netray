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

## Phase 2 — Email module

## Groups

G1: C1, C2, C4, C5 (engine `TimedOut` + netray-email) · G2: C3, C6, C7 (lens and the binary). Orchestrator-written, the Phase 1 shape.

## Plan

### G1 — netray-email
- `crates/engine/src/lib.rs`: `SectionOutcome::TimedOut`.
- `crates/email/src/config.rs`: refuse `BEACON_`, load `NETRAY_EMAIL_`; `ConfigSource::Env` prints `NETRAY_EMAIL_CONFIG`; `ModuleConfig { dns, dnsbl, http, dkim, backends, inspections }`.
- `crates/email/src/module.rs`: `translate(&[SseEvent])` ported verbatim from `crates/lens/src/backends/email.rs` (buckets, cross-validation routing, Null MX, skipped summary → `TimedOut`, structural errors → `Incomplete`) with lens's unit tests and the bucket-vocabulary test; `EmailModule` (`async new`, resolvers as `AppState::new`, inspection semaphore; `run` drives `run_all_checks` into a channel with the request's selectors, collects the events, translates).
- `crates/email/src/testing.rs`, `lib.rs` exports, `run()` reads `NETRAY_EMAIL_CONFIG`; Cargo: `netray-engine`, feature `testing`, `[[test]] module`.

### G2 — lens and the binary
- `crates/lens/src/modules.rs`: the adapter takes any protocol; email presentation → `BackendExtra::Email`; `TimedOut` → `SectionError::Timeout`, with the WARN log.
- `crates/lens/src/config.rs` refuses `backends.email.url`; `state.rs` email section from the registry; delete `backends/email.rs` and its uses; lens dev/example configs; dev-dep `netray-email` with `testing`.
- `crates/netray`: `lens_registry` builds `EmailModule` (async) from `[modules.email]`.

## Phase 3 — IP module

## Groups

G1: C1, C2, C4, C6, C7, C8 (netray-ip) · G2: C3, C5, C9 (lens and the binary, SIGHUP). Orchestrator-written, the Phase 1 shape.

## Plan

### G1 — netray-ip
- `crates/ip/src/config.rs`: refuse `IFCONFIG_`, load `NETRAY_IP_`; `ModuleConfig` with the data path keys.
- `crates/ip/src/module.rs`: `translate` ported verbatim from `crates/lens/src/backends/ip.rs`; `IpModule` (`async new` loading `EnrichmentContext` with the same refuse/warn semantics, `ArcSwap` + `reload()`, `DnsCache`, sampling 4+4 sorted public addresses from `Facts` via `is_allowed_target`, `get_ifconfig` with no reverse DNS).
- `crates/ip/src/testing.rs`, `lib.rs` exports and `run()` reading `NETRAY_IP_CONFIG`; Cargo: `netray-engine`, feature `testing`, `[[test]] module`.

### G2 — lens and the binary
- `crates/lens/src/modules.rs`: `Facts.a`/`aaaa` from `BackendContext.resolved_ips`; IP presentation → `BackendExtra::Ip`, `detail_url` as before.
- `crates/lens/src/config.rs` refuses `backends.ip.url`; `state.rs` IP section from the registry; delete `backends/ip.rs` and its uses; lens dev/example configs; dev-dep `netray-ip` with `testing`.
- `crates/netray`: `lens_registry` builds `IpModule` from `[modules.ip]`, keeps an `Arc` and reloads it on SIGHUP.

## Phase 4 — TLS module

## Groups

G1: C1, C2, C4, C5, C6 (netray-tls) · G2: C3, C7 (lens and the binary). Orchestrator-written, the Phase 1 shape.

## Plan

### G1 — netray-tls
- `crates/tls/src/config.rs`: refuse `TLSIGHT_`, load `NETRAY_TLS_`; `ModuleConfig` (limits subset incl. per-target, dns, validation, quality, backends.ip).
- `crates/tls/src/routes.rs`: extract the inspection core of `do_inspect` into a pub async fn returning `InspectResponse`; the route keeps the per-client cap-and-warn and its headers/response.
- `InspectResponse` and its parts derive `Deserialize` (`&'static str` fields become `String`/`Cow`).
- `crates/tls/src/module.rs`: `translate` ported verbatim from `crates/lens/src/backends/tls.rs` (`NOT_TESTED_FROM_HERE` → `NotTested`); `TlsModule` (`async new`, `parse_input`, per-target limit before the target policy, handshake semaphore, core, translate).
- `crates/tls/src/testing.rs`, exports, `run()` reads `NETRAY_TLS_CONFIG`; Cargo: feature `testing`, `[[test]] module`.

### G2 — lens and the binary
- `crates/lens/src/modules.rs`: TLS presentation → `BackendExtra::Tls`, `detail_url` as before; `config.rs` refuses `backends.tls.url`; `state.rs` TLS from the registry; delete `backends/tls.rs`; lens dev/example configs; dev-dep `netray-tls` with `testing`; justfile tls row crate dir.
- `crates/netray`: `lens_registry` builds `TlsModule` from `[modules.tls]`.
