# Spec: v2 modules

Status: Done
Created: 2026-10-10
Finished: 2026-10-10

## Goal

The five check services become V2 modules and lens runs them in-process through `netray-engine` (V2 SDD Phase 1a, V1.3–V1.7): each crate is renamed to its protocol, implements `Module` and carries the translation lens does today; lens's check path asks the engine; `crates/lens/src/backends/` is gone; and lens's whole output for every fixture equals what `0.23.x` produced, pinned before the first move.

## Non-goals

- Catalogue v3, profile v3, evidence shapes, the status split of error `Skip`s (V2 Phase 1b, S32).
- One process in production (S1, cutover): the services keep running as their own subcommands.
- In-process address enrichment: modules keep their HTTP `EnrichmentClient`.
- Moving the own lookups of tlsight, beacon and prism onto `Facts` (1b).
- Metric names (S11), config file names and config sections of the services (S10): unchanged.

## Context and constraints

- V2 SDD (planning repo `specs/sdd/v2.md`): §3.1, §3.2, §3.6, S9 (rename with the module change), S32, Phase 1a V1.3–V1.7.
- `netray-model` and `netray-engine` exist (`crates/model`, `crates/engine`); the engine has `Module`, `FactsProvider`, `RunContext { deadline }` (`crates/engine/src/lib.rs:37`), `Facts { a, aaaa, mx, caa, ns }` (`:42`), `SectionOutcome::Measured(Vec<CheckResult>)` (`:53`), no `Registry`; neither crate derives `Deserialize` (`crates/model/src/lib.rs:156`).
- lens's backends (`crates/lens/src/backends/`): DNS `POST /api/check` SSE (`dns.rs:84`), TLS `GET /api/inspect` (`tls.rs:127`), HTTP `GET /api/inspect` (`http.rs:151`), email `POST /inspect` SSE (`email.rs:64`), IP `GET /json?ip=` per sampled address (`ip.rs:128`); IP runs after DNS on `BackendExtra::Dns.resolved_ips` (`check.rs:120`).
- In-process entry points: spectra `input::parse_url`, `input::validate_target` (resolves with `tokio::net::lookup_host`, `input.rs:46`), `inspect::inspect`, `inspect::assemble_response`; beacon `checks::run_all_checks`; ifconfig-rs `backend::get_ifconfig` with `EnrichmentContext::load`; tlsight `routes::do_inspect` (private, returns an axum `Response`); prism's pipeline inline in `api/check.rs` `post_handler` (`:142-612`).
- ifconfig-rs data: a missing GeoIP city or ASN database or `user_agent_regexes` refuses startup; the other sources (Tor, Feodo, CINS, cloud, VPN, datacenter, bot, Spamhaus, ASN info) warn and continue (`crates/ifconfig-rs/src/enrichment.rs:164`, `:310`). The data never enters the image; the deployment mounts `/netray/data`.
- Env prefixes today: `PRISM_`, `TLSIGHT_`, `IFCONFIG_`, `SPECTRA__`, `BEACON__` (`crates/beacon/src/config.rs:158`), each with `<NAME>_CONFIG`; lens `LENS_`. CLAUDE.md "Config stays per service" fixes them; repo checks and the release smoke test set them (`tests/repo/test_service_config_and_metrics.sh:45,47`, `test_smoke_services.sh:32`, `test_header_parity.sh`, `test_image_data.sh`, `justfile`, `.github/workflows/release.yml:77`).
- lens tests drive stub HTTP servers serving contract goldens (`crates/lens/tests/lens_golden.rs:185`, `contract_backends.rs`, `contract_beacon.rs`, `contract_ip.rs`, `ip_sampling.rs`, `ip_reputation_flags.rs`, `ip_enrichment_errors.rs`, `unknown_verdicts.rs`, `incomplete_results.rs`); `lens_golden` compares a projection without prose (`lens_golden.rs:7`). `beacon-timeout.sse`, `beacon-partial.sse`, `beacon-sending-no-dkim.sse` have no lens result golden.
- `tests/repo/test_check_config.sh:125` checks the timeout budget over four `[backends.*] timeout_ms` rows; CLAUDE.md "Startup rejects are checked".
- Every protected test a phase rewrites carries `ADLC-Test-Change` naming the requirement.

## Requirements

1. **Baseline.** Before any move, lens's full sync output (sections, checks with messages, headlines, extras, score; durations and IDs removed) is committed per lens fixture and for `beacon-timeout`, `beacon-partial`, `beacon-sending-no-dkim`, under `tests/fixtures/contracts/lens-full-*.json`, produced by the `0.23.x` code with `UPDATE_GOLDEN=1`; a test compares lens's output with them.
2. **Shared seams.** `RunContext` carries `domain: Domain` and `options: RunOptions { dkim_selectors }`; `SectionOutcome::Measured` carries `checks` and a transitional `presentation: serde_json::Value` (the section's V1 headline and extras); `netray-engine` has `Registry` (modules plus an optional `FactsProvider`); `crates/netray` builds the modules from lens's `[modules.<p>]` tables and calls `lens::run_with(config, registry)`; `AppState::with_registry` serves tests.
3. **HTTP rename.** `crates/spectra` moves by `git mv` to `crates/http` (package `netray-http`, library `netray_http`); it reads `NETRAY_HTTP_` (nesting `__`) and `NETRAY_HTTP_CONFIG`, and refuses startup when a `SPECTRA_`-prefixed variable is set, naming the new prefix. Paths, repo checks, the release smoke test, docs and CLAUDE.md follow.
4. **HTTP module.** `netray-http` has a `ModuleConfig` (`deny_unknown_fields`) and implements `Module` with check IDs `http.<v1 name>`; `run()` inspects in-process on the addresses in `Facts` and translates with a pure function moved from `crates/lens/src/backends/http.rs`; behind feature `testing`, `golden_module(contract)` runs that translation on a contract golden.
5. **HTTP section through the engine.** lens takes the HTTP section from the registry; `backends/http.rs` and `[backends.http] url` are gone (a leftover `url` is refused), `timeout_ms` is the section deadline; lens's tests use `golden_module` (dev-dependency); the full-output and lens result goldens are unchanged.
6. **Email rename.** `crates/beacon` → `crates/email` (`netray-email`), `NETRAY_EMAIL_`, `NETRAY_EMAIL_CONFIG`, a `BEACON_`-prefixed variable refuses startup; paths, checks, docs follow.
7. **Email module.** `netray-email` has a `ModuleConfig` and implements `Module` (`email.<v1 name>`), running `run_all_checks` in-process with the request's DKIM selectors and translating with the function moved from `backends/email.rs`; `golden_module` behind `testing`; its inspection semaphore stays.
8. **Email section through the engine.** As 5 for email: `backends/email.rs` and `[backends.email] url` gone; full-output goldens unchanged, the three beacon-only ones included.
9. **IP rename.** `crates/ifconfig-rs` → `crates/ip` (`netray-ip`), its `data/` directory moves along; `NETRAY_IP_`, `NETRAY_IP_CONFIG`, an `IFCONFIG_`-prefixed variable refuses startup; the data image, the release's no-data check, paths, checks, docs follow.
10. **IP module.** `netray-ip` has a `ModuleConfig` with the data paths and loads them as ifconfig-rs does (GeoIP city, ASN and user-agent regexes refuse startup when they fail to load, the others warn); `netray lens --check-config` loads them too and has a `startup_rejects` row. lens reloads the IP data on SIGHUP as ifconfig-rs does. `Module` (`ip.<v1 name>`) samples four IPv4 and four IPv6 public addresses from `Facts`, sorted, and translates with the function moved from `backends/ip.rs`; `golden_module` behind `testing`.
11. **IP section through the engine.** As 5 for IP; until the DNS module exists, lens fills `Facts.a`/`aaaa` from the DNS backend's resolved addresses.
12. **TLS rename.** `crates/tlsight` → `crates/tls` (`netray-tls`), `NETRAY_TLS_`, `NETRAY_TLS_CONFIG`, a `TLSIGHT_`-prefixed variable refuses startup; paths, checks, docs follow.
13. **TLS module.** The inspection core is extracted from `do_inspect` into a pub function that both the HTTP route and the module call; tlsight's route responses stay as they are. `ModuleConfig`, `Module` (`tls.<v1 name>`), the handshake semaphore stays, the translation moves from `backends/tls.rs`; `golden_module` behind `testing`.
14. **TLS section through the engine.** As 5 for TLS.
15. **DNS rename.** `crates/mhost-prism` → `crates/dns` (`netray-dns`), `NETRAY_DNS_`, `NETRAY_DNS_CONFIG`, a `PRISM_`-prefixed variable refuses startup; paths, checks, docs follow.
16. **DNS module.** The check pipeline is extracted from `post_handler` into a pub function that both the SSE route and the module call; prism's event stream stays as it is. `ModuleConfig`, `Module` (`dns.<v1 name>`), the query semaphore stays, the translation moves from `backends/dns.rs`; `netray-dns` implements `FactsProvider` (A, AAAA, MX, CAA, NS, HTTPS RR); `golden_module` behind `testing`.
17. **DNS section through the engine.** As 5 for DNS; `Facts` come from the DNS module's `FactsProvider`.
18. **Engine run.** `netray-engine` runs a domain: one resolve stage through the registry's `FactsProvider` looks up A, AAAA, MX, CAA, NS and the HTTPS RR once per name; every module runs on those `Facts` concurrently, each under its section deadline inside the hard deadline; each finished section is sent on a bounded channel as it completes; a module that overruns makes its section incomplete and finished sections keep their results. lens's check path calls it.
19. **No own HTTP client.** A repo check fails when a module crate's sources construct a `reqwest` client (`reqwest::Client::new`, `Client::builder`, `ClientBuilder`, after any `use` alias) outside `netray_common`; the module crates pass.
20. **lens without backends.** `crates/lens/src/backends/` does not exist, `crates/lens/Cargo.toml` `[dependencies]` names no module crate, and the full-output and lens result goldens are unchanged.

## Phase 0 — Baseline

**Depends on:** none
**Requirements:** 1

### Test Scenarios

- GIVEN each lens fixture and the three beacon-only contract goldens WHEN lens runs at `0.23.x` code with `UPDATE_GOLDEN=1` THEN `lens-full-*.json` are written; without it the test compares and passes.
- GIVEN a headline changed by one word in lens's code WHEN the test runs THEN it fails naming the fixture and field.

## Phase 1 — HTTP module

**Depends on:** 0
**Requirements:** 2, 3, 4, 5

### Test Scenarios

- GIVEN a stub `Module` WHEN run with a `RunContext` carrying domain and DKIM selectors THEN it reads both; GIVEN `Measured { checks, presentation }` THEN both reach lens.
- GIVEN the workspace WHEN built THEN `crates/http` is `netray-http` and no tracked path names `crates/spectra`.
- GIVEN `NETRAY_HTTP_SERVER__BIND` WHEN `netray http` starts THEN it binds there; GIVEN `SPECTRA__SERVER__BIND` set THEN startup is refused naming `NETRAY_HTTP_`.
- GIVEN `spectra-inspect.json` WHEN translated by `netray_http` THEN checks and presentation equal the HTTP section of `lens-full-*.json`.
- GIVEN `[modules.http]` with an unknown key, or `[backends.http] url` set, WHEN `netray lens --check-config` THEN exit 1 naming it.
- GIVEN lens with the golden modules WHEN the full-output and `lens_golden` tests run THEN unchanged.

## Phase 2 — Email module

**Depends on:** 1
**Requirements:** 6, 7, 8

### Test Scenarios

- GIVEN `BEACON__SERVER__BIND` or `BEACON_CONFIG` set WHEN `netray email` starts THEN refused naming `NETRAY_EMAIL_`.
- GIVEN each `beacon-*.sse` WHEN translated by `netray_email` THEN equal to the email section of its full-output golden (Null MX, no MX, timeout incomplete, partial, cross-validation routing).
- GIVEN DKIM selectors on a lens request WHEN the email module runs THEN it receives them.
- GIVEN lens WHEN the full-output and `lens_golden` tests run THEN unchanged.

## Phase 3 — IP module

**Depends on:** 2
**Requirements:** 9, 10, 11

### Test Scenarios

- GIVEN `ifconfig-json.json` WHEN translated THEN equal to the IP section of its full-output golden; GIVEN nine public and two private addresses in `Facts` THEN four IPv4 and four IPv6 public ones are sampled, sorted.
- GIVEN `[modules.ip] geoip_city_db` missing WHEN `netray lens --check-config` and WHEN lens starts THEN both refuse; GIVEN `feodo_botnet_ips` missing THEN both start with a warning.
- GIVEN `IFCONFIG_CONFIG` set WHEN `netray ip` starts THEN refused naming `NETRAY_IP_CONFIG`.
- GIVEN a data file replaced and SIGHUP sent to lens WHEN the next IP section runs THEN it uses the new data.
- GIVEN the release image WHEN checked THEN it carries no data file.
- GIVEN lens WHEN the full-output and `lens_golden` tests run THEN unchanged.

## Phase 4 — TLS module

**Depends on:** 3
**Requirements:** 12, 13, 14

### Test Scenarios

- GIVEN `tlsight-inspect.json`, `tlsight-not-tested.json`, `tlsight-unreachable.json` WHEN translated THEN equal to the TLS section of their full-output goldens.
- GIVEN tlsight's `GET /api/inspect` WHEN its existing tests run THEN responses unchanged.
- GIVEN `TLSIGHT_CONFIG` set WHEN `netray tls` starts THEN refused naming `NETRAY_TLS_CONFIG`.
- GIVEN lens WHEN the full-output and `lens_golden` tests run THEN unchanged.

## Phase 5 — DNS module

**Depends on:** 4
**Requirements:** 15, 16, 17

### Test Scenarios

- GIVEN `prism.sse`, `prism-no-address.sse` WHEN translated THEN equal to the DNS section of their full-output goldens, the email categories dropped and DNSSEC-absent as Skip.
- GIVEN prism's `POST /api/check` WHEN its existing tests run THEN the event stream is unchanged.
- GIVEN a stub resolver WHEN `netray-dns`'s `FactsProvider` resolves a domain THEN `Facts` hold its A, AAAA, MX, CAA, NS and HTTPS answers.
- GIVEN `PRISM_CONFIG` set WHEN `netray dns` starts THEN refused naming `NETRAY_DNS_CONFIG`.
- GIVEN lens WHEN the full-output and `lens_golden` tests run THEN unchanged.

## Phase 6 — Engine run

**Depends on:** 5
**Requirements:** 18, 19, 20

### Test Scenarios

- GIVEN a counting stub `FactsProvider` and modules that read `Facts` WHEN one run executes THEN each of A, AAAA, MX, CAA, NS and HTTPS is resolved once per name.
- GIVEN stub modules finishing after 10, 50 and 100 ms WHEN a run streams THEN sections arrive in that order, each before the slower ones finish.
- GIVEN a stub module that overruns its section deadline WHEN the run ends THEN its section is incomplete and the others keep their results.
- GIVEN a fixture module source with `use reqwest::Client as C; C::new()` WHEN the repo check runs on it THEN it fails; GIVEN the module crates THEN it passes.
- GIVEN lens WHEN built THEN `crates/lens/src/backends/` does not exist and `[dependencies]` names no module crate.
- GIVEN lens WHEN the full-output and `lens_golden` tests run THEN unchanged.

## Decision log

- One feature, one phase per module, then the engine run, over two features: no translation exists twice, every phase bisectable (operator, 2026-10-10).
- Directories `crates/dns`, `tls`, `http`, `email`, `ip` (operator, 2026-10-10).
- Env prefixes `NETRAY_<P>_` with `__` nesting and `NETRAY_<P>_CONFIG`, a hard switch: an old-prefix variable refuses startup; argus-oci renders the new names from the release on (operator, 2026-10-10). Amends CLAUDE.md "Config stays per service".
- Module config as `[modules.<p>]` in lens.toml (operator, 2026-10-10).
- lens mounts `/netray/data` read-only and loads the IP data itself (operator, 2026-10-10).
- Modules keep their HTTP enrichment client in 1a (operator, 2026-10-10).
- A transitional `presentation` JSON in `SectionOutcome::Measured`, over evidence shapes now or lens naming module crates (operator, 2026-10-10).
- Golden modules behind a `testing` feature, used by lens as dev-dependencies; P40's ban covers `[dependencies]` (operator, 2026-10-10).
- Phase 0 pins lens's full output, prose included, before any move (operator, 2026-10-10).
- V1.5 in 1a: one resolve stage, HTTP and IP read `Facts`; tlsight, beacon and prism keep their own lookups until 1b, an SDD amendment (operator, 2026-10-10).
- `[backends.<p>] url` goes with its section, `timeout_ms` stays as the section deadline, budget check unchanged (operator, 2026-10-10).
- `RunContext` carries domain and options, a `Registry` in the engine, `lens::run_with` from the binary (operator, 2026-10-10).
- The IP module reads `Facts` from Phase 3; lens fills them from the DNS backend until Phase 5 (operator, 2026-10-10).
- Order HTTP, email, IP, TLS, DNS (operator, 2026-10-10).
- lens's container limit rises to 768m for the in-process IP data (heap-loaded as in ifconfig-rs, no mmap), and lens reloads that data on SIGHUP; both argus-side in the version-gated change (operator, 2026-10-10, after argus-oci's feasibility check).
- Check IDs `<p>.<v1 name>` in 1a; the catalogue renames them in 1b (operator, 2026-10-10).

## Open decisions

None.

## Out of scope

- argus-oci, new K items for the planning session: render `NETRAY_<P>_` env names, lens.toml `[modules.*]` without `[backends.<p>] url`, mount `/netray/data` into lens; version-gated to this release (`0.24.0`).
- SDD amendments for the planning session: V1.5's scope in 1a; `RunContext`, `Registry` and `presentation` added to §3.6.
