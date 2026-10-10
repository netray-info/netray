# Spec: v2 modules

Status: Draft
Created: 2026-10-10

## Goal

The five check services become V2 modules and lens runs them in-process through `netray-engine` (V2 SDD Phase 1a, V1.3–V1.7): each crate is renamed to its protocol, implements `Module`, and carries the translation lens does today; lens's check path asks the engine, `crates/lens/src/backends/` is gone, and for every contract golden and reference fixture lens's result equals `0.23.x`'s lens result goldens.

## Non-goals

- Catalogue v3, profile v3, evidence shapes, the status split of error `Skip`s (V2 Phase 1b, S32).
- One process in production (S1, cutover): the services keep running as their own subcommands for their UIs and APIs.
- In-process address enrichment: modules keep their `EnrichmentClient` over HTTP to the IP service.
- Metric names (S11) and the services' config file names and sections (S10): unchanged.

## Context and constraints

- V2 SDD (planning repo `specs/sdd/v2.md`): §3.1 crates, §3.2 one domain run, §3.6 the traits, S9 (rename in the same change that turns a crate into a module), S32 (in 1a every V1 `Skip` is `not_applicable`), Phase 1a V1.3–V1.7.
- `netray-model` and `netray-engine` exist (`crates/model`, `crates/engine`, feature `v2-model`); every V1 crate maps its status word onto `netray_model::Status`.
- lens today (`crates/lens/src/backends/`, 2.5k lines): DNS `POST /api/check` SSE from prism (`dns.rs:84`), TLS `GET /api/inspect` JSON from tlsight (`tls.rs:127`), HTTP `GET /api/inspect` JSON from spectra (`http.rs:151`), email `POST /inspect` SSE from beacon (`email.rs:64`), IP `GET /json?ip=` per sampled address from ifconfig-rs (`ip.rs:128`); two waves, IP after DNS's addresses (`check.rs:45-130`).
- In-process entry points (read 2026-10-10): spectra `input::parse_url`, `input::validate_target`, `inspect::inspect`, `inspect::assemble_response` (pub); beacon `checks::run_all_checks` (pub, streams `SseEvent`); ifconfig-rs `backend::get_ifconfig` with `EnrichmentContext::load` (pub); tlsight `routes::do_inspect` (private, returns an axum `Response`); prism's check pipeline is inline in `api/check.rs` `post_handler` (`:142-617`).
- ifconfig-rs needs its data files (GeoLite2, reputation lists, `asn_patterns.toml`), which never enter the image (`specs/features/monorepo-no-data-in-image`); the deployment mounts them at `/netray/data`.
- Env prefixes today: `PRISM_`, `TLSIGHT_`, `SPECTRA__`, `BEACON_`, `IFCONFIG_`, plus `<NAME>_CONFIG`; lens `LENS_`.
- Results stay as `0.23.x`: `crates/lens/tests/lens_golden.rs` (lens result goldens `tests/fixtures/contracts/lens-*.json`) and the contract goldens (`tests/fixtures/contracts/{prism,tlsight,spectra,beacon,ifconfig}-*`).

## Requirements

1. **Rename.** `crates/spectra`, `beacon`, `ifconfig-rs`, `tlsight`, `mhost-prism` move by `git mv` to `crates/http`, `email`, `ip`, `tls`, `dns`; packages `netray-http`, `-email`, `-ip`, `-tls`, `-dns`, libraries `netray_<p>`. Subcommands, config file names, config sections, metric names and frontend package names stay. Docs, contract-golden producers, the Dockerfile, CI, release and repo checks follow the paths.
2. **Env prefixes.** Each module reads `NETRAY_<P>_` (nesting `__`, e.g. `NETRAY_DNS_SERVER__BIND`) and `NETRAY_<P>_CONFIG`. A variable with a module's old prefix (`PRISM_`, `TLSIGHT_`, `SPECTRA_`, `BEACON_`, `IFCONFIG_`, and their `_CONFIG`) present at startup is refused with a message naming the new prefix. lens keeps `LENS_`.
3. **Module config.** Each module defines a `ModuleConfig` with the fields its check needs (`deny_unknown_fields`). lens's config holds `[modules.<p>]` as raw tables; `crates/netray` builds each module from its table and hands lens an engine `Registry`; `netray lens --check-config` validates every module table.
4. **Module.** Each module implements `Module`: `checks()` lists `<p>.<v1 check name>` IDs, `run()` computes the section in-process and translates it with a pure function moved from `crates/lens/src/backends/<p>.rs`; the module's own bulkhead (prism's query semaphore, tlsight's handshake semaphore, beacon's inspection semaphore) stays; the service's per-client rate limiters do not apply in-process.
5. **Presentation.** `SectionOutcome::Measured` carries `presentation: serde_json::Value` with the section's V1 headline and extras; lens builds its V1 output from it. The field is transitional and ends with the evidence shapes (1b).
6. **lens through the engine.** For each converted section, lens's check path gets the outcome from the registry's module, never over HTTP; when all five are converted, `crates/lens/src/backends/` is deleted and lens depends on no module crate.
7. **Engine run.** `netray-engine` orchestrates a run: one resolve stage through `FactsProvider` (implemented by `netray-dns`) answers A, AAAA, MX, CAA, NS and the HTTPS RR once per run (V1.5); each finished section is sent on a bounded channel before slower ones finish (V1.6); a module that overruns its deadline makes its section incomplete while finished sections keep their results, and no module crate builds its own HTTP client outside `netray_common` (V1.7).
8. **Results unchanged.** After every phase, lens's result for every contract golden and reference fixture equals its lens result golden; the module translation of each contract golden is tested in the module crate.
9. **IP data.** The IP module loads its data from the paths in `[modules.ip]`; a configured path that fails to load refuses startup, as today.

## Phase 1 — HTTP module

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 5, 6, 8 (for spectra → `netray-http`)

### Test Scenarios

- GIVEN the workspace WHEN built THEN `crates/http` is package `netray-http` and no path or manifest names `crates/spectra` (repo check).
- GIVEN `NETRAY_HTTP_SERVER__BIND` WHEN `netray http` starts THEN it binds there; GIVEN `SPECTRA__SERVER__BIND` set THEN startup is refused naming `NETRAY_HTTP_`.
- GIVEN `tests/fixtures/contracts/spectra-inspect.json` WHEN translated by `netray_http` THEN its checks and presentation equal what lens derived from it in `0.23.x`.
- GIVEN lens with an HTTP module returning the translated golden WHEN `lens_golden` runs THEN every lens result golden is unchanged.
- GIVEN `[modules.http]` with an unknown key WHEN `netray lens --check-config` THEN exit 1 naming the key.
- GIVEN lens's check path WHEN the HTTP section runs THEN no request goes to `http_url` (the backend is gone).

## Phase 2 — Email module

**Depends on:** 1
**Requirements:** 1, 2, 3, 4, 5, 6, 8 (beacon → `netray-email`)

### Test Scenarios

- GIVEN each `beacon-*.sse` golden WHEN translated by `netray_email` THEN checks and presentation equal `0.23.x`'s lens email section, including Null MX, no MX, a timeout (incomplete) and the cross-validation routing.
- GIVEN `BEACON_CONFIG` set WHEN `netray email` starts THEN refused naming `NETRAY_EMAIL_CONFIG`.
- GIVEN lens WHEN `lens_golden` runs THEN unchanged; GIVEN dkim selectors on the request THEN the module receives them.

## Phase 3 — IP module

**Depends on:** 2
**Requirements:** 1, 2, 3, 4, 5, 6, 8, 9 (ifconfig-rs → `netray-ip`)

### Test Scenarios

- GIVEN `ifconfig-json.json` WHEN translated THEN the reputation check and presentation equal `0.23.x`'s IP section; sampling stays four IPv4 and four IPv6 public addresses, sorted.
- GIVEN `[modules.ip]` naming a missing data file WHEN lens starts THEN refused; GIVEN no data paths THEN the features they feed are off, as in ifconfig-rs today.
- GIVEN `IFCONFIG_CONFIG` set WHEN `netray ip` starts THEN refused naming `NETRAY_IP_CONFIG`.
- GIVEN the release image WHEN inspected THEN it still carries no data file.

## Phase 4 — TLS module

**Depends on:** 3
**Requirements:** 1, 2, 3, 4, 5, 6, 8 (tlsight → `netray-tls`)

### Test Scenarios

- GIVEN `tlsight-*.json` goldens WHEN translated THEN equal to `0.23.x`'s TLS section (first port, `tls_reachable`, `NOT_TESTED_FROM_HERE`).
- GIVEN tlsight's HTTP route WHEN called THEN its response is unchanged (the extracted core serves both).
- GIVEN `TLSIGHT_CONFIG` set WHEN `netray tls` starts THEN refused naming `NETRAY_TLS_CONFIG`.

## Phase 5 — DNS module

**Depends on:** 4
**Requirements:** 1, 2, 3, 4, 5, 6, 8 (mhost-prism → `netray-dns`)

### Test Scenarios

- GIVEN `prism*.sse` goldens WHEN translated THEN equal to `0.23.x`'s DNS section, including the email categories it drops and DNSSEC-absent as Skip.
- GIVEN prism's `POST /api/check` WHEN called THEN its event stream is unchanged (the extracted pipeline serves both).
- GIVEN `PRISM_CONFIG` set WHEN `netray dns` starts THEN refused naming `NETRAY_DNS_CONFIG`.
- GIVEN lens WHEN `lens_golden` runs THEN unchanged, and `crates/lens/src/backends/` holds no section file.

## Phase 6 — Engine run

**Depends on:** 5
**Requirements:** 6, 7, 8

### Test Scenarios

- GIVEN a counting stub resolver WHEN one run executes THEN each of A, AAAA, MX, CAA, NS and HTTPS is looked up once per name (V1.5).
- GIVEN stub modules finishing after 10 ms, 50 ms and 100 ms WHEN a run streams THEN sections arrive in that order, each before the slower ones finish (V1.6).
- GIVEN a stub module that overruns its deadline WHEN the run ends THEN its section is incomplete and the others keep their results (V1.7).
- GIVEN the module crates WHEN the repo check scans their sources THEN no `reqwest::Client::builder` or `ClientBuilder` outside `netray_common` (V1.7).
- GIVEN lens WHEN built THEN `crates/lens/src/backends/` does not exist and `crates/lens/Cargo.toml` names no module crate.
- GIVEN every lens result golden WHEN `lens_golden` runs THEN unchanged (V1.4).

## Decision log

- One feature, one phase per module, then the engine run, over two features: no translation exists twice, and every phase is bisectable (operator, 2026-10-10).
- Directories `crates/dns`, `tls`, `http`, `email`, `ip`, beside `crates/model` and `crates/engine` (operator, 2026-10-10).
- Env prefixes become `NETRAY_<P>_` with `__` nesting and `NETRAY_<P>_CONFIG`, a hard switch: an old-prefix variable refuses startup; argus-oci renders the new names from the release on (operator, 2026-10-10).
- Module config as `[modules.<p>]` in lens.toml, the shape of S10's `netray.toml` (operator, 2026-10-10).
- lens mounts `/netray/data` read-only and loads the IP data itself (operator, 2026-10-10).
- Modules keep their HTTP enrichment client in 1a (operator, 2026-10-10).
- A transitional `presentation` JSON in `SectionOutcome::Measured` carries the V1 headline and extras, over evidence shapes now or lens naming module crates (operator, 2026-10-10).
- Order HTTP, email, IP, TLS, DNS: from the cleanest in-process API to the two that need their core extracted from a handler.
- Check IDs in 1a are `<p>.<v1 name>`; the catalogue renames them in 1b.

## Open decisions

None.

## Out of scope

- The argus-oci side, new K items for the planning session: render `NETRAY_<P>_` env names and `[modules.*]` in lens.toml, and mount `/netray/data` into lens, version-gated to this release.
