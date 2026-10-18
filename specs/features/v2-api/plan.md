# Plan: v2-api

## Phase 1 — API module, stored findings, contract harness, catalogue, profiles, meta

Interface (binding for tests and code): `netray_web::api::{router, openapi, contract::{compat_errors, untyped_schemas}, ApiConfig, ApiState, ApiParts, AdmissionLimits, DomainRuns, DomainRefusal, RunTrigger, RunStart, RunRefusal, RefusalReason, RunEvents, RunEvent}`; document paths are full (`/api/v1/...`), `/openapi.json` and `/docs` are not paths of the document; `/catalogue` answers `{checks}`; profile thresholds `[{grade, min}]`; `/meta` limits nest `api`, `admission`, `rate_limit`; `lens::api_v1::LensRuns`; `lens::run_with(config, registry, ApiParts)`.

## Groups

G1: C3, C4, C33, C34, C35 · G2: C2, C5, C6, C10, C11, C12, C13, C17, C18, C19, C20, C21, C22, C23, C25, C26, C36 · G3: C1, C7, C8, C9, C14, C16, C24, C27, C28, C29, C30, C31, C32, C37, C38 · G4: C15

## Plan

### G1 — model and engine derives, RunCheck fields, build_run
- crates/model: utoipa dependency; ToSchema on lib.rs enums, hand-written string schemas for `CheckId` and `BlockId`; evidence.rs (`Value::Addr` as String, `Node` no_recursion), diff.rs, run.rs (`RunCheck.findings`/`.evidence`, serde default + skip empty). `fingerprint` and `diff` unchanged.
- crates/engine: utoipa to [dependencies]; `CheckSpec`, `SectionWeights`, `Profile` derive Serialize + ToSchema, thresholds via `serialize_with` as `[{grade, min}]` (`Threshold` type); `Profiles::versions()`.
- crates/lens/src/history.rs: `build_run` copies `messages` → `findings`, `evidence` → `evidence`; `catalogue_checks` empty.
- Existing test literals gain the fields (ADLC-Test-Change, R4): crates/model/tests/diff_presence.rs, crates/engine/tests/history.rs, crates/email/tests/dnsbl_domain.rs, crates/web/tests/entries_view.rs.
- SDD §3.5/§4.1 sentence travels with this group's commit.

### G2 — netray-web `api` module, contract harness, fixture
- Root Cargo.toml: `utoipa-axum`, `utoipa-scalar` into [workspace.dependencies]; lens uses them `{ workspace = true }`.
- crates/web/Cargo.toml: netray-common `schema`, serde, utoipa, utoipa-axum, utoipa-scalar, governor, futures.
- crates/web/src/lib.rs `pub mod api;`; crates/web/src/api/{mod,config,runs,state,error,limit,meta(+catalogue, profiles),docs,contract}.rs. Router relative to the mount, `OpenApiRouter` + `split_for_parts`, document nested at `/api/v1`; limiter as `route_layer` on the limited sub-router; JSON 404 fallback; `enabled = false` → empty router. Docs page via `Scalar::custom_html` naming `/api/v1/openapi.json`.
- No file under crates/web/src/api/ contains the no-verdict words (comments included).
- tests/fixtures/contracts/api-v1.openapi.json generated with `UPDATE_GOLDEN=1 cargo test -p netray-web --test openapi_contract`.

### G3 — lens wiring and crates/netray
- crates/lens/src/api_v1.rs `LensRuns` (validate over `input::validate_domain`; start → NotStarted in Phase 1).
- crates/lens/src/config.rs `Config.api` (`#[serde(default)]`), `validate` calls `api.validate()`; tests/repo/test_check_config.sh startup_rejects rows for `per_ip_per_minute = 0` / `per_ip_burst = 0` (ADLC-Test-Change).
- crates/lens/src/state.rs `AppState.api_parts`; crates/lens/src/lib.rs `run_with(.., parts)`, `app` nests `netray_web::api::router` at `/api/v1` when enabled, `security_headers_mw` relaxes CSP for `/api/v1/docs` as for `/docs`.
- crates/netray: depends on netray-web, passes `ApiParts::default()`.
- crates/lens/lens.example.toml `[api]` block; README line.
- Existing lens `Config { .. }` literals gain `api: Default::default()` (ADLC-Test-Change, R8): badge_routes, entries_og_card, privacy_mount, history_badge, entries_mount, snapshot_routes, og_routes, og_label_bounds, history_cache_badge_og, reference_domains, sdd_lens_badges_p2_c5_meta_badges, deadlines, sdd_lens_badges_p1_c11_cache_coalesce; in-src test modules of config.rs, state.rs, routes.rs.

### G4 — SDD prose
- specs/sdd/v2.md §3.1 (apex API v1 in `crates/web/src/api/`, mounted by lens ahead of its SPA fallback until `v2-cutover` mounts it in `netray serve`), §10 Phase 3 row `0.30.0`. Committed with G3.

## Phase 2 — Domain and entry reads

## Groups

G1: C1-C16 (all five routes share `crates/web/src/api/mod.rs` `read_routes`, the new `V1Error` variants and the views).

## Plan

### G1
- crates/web/src/api/error.rs: `DomainInvalid` 400, `Gone` 410, `Internal` 500, `Unavailable` 503.
- crates/web/src/entries/{routes,mod}.rs: `well_formed` `pub(crate)`, re-exported, one ID rule with `/r/<id>`.
- crates/web/src/api/reads.rs (new): `EntryView`, `DiffView`, `HistoryResponse`, `HistoryItem` (Serialize + ToSchema), built from `netray_store::Entry` by destructuring with `..`; helpers `history` (503 first), `domain` (`DomainRuns::validate` → 400), `entry` (malformed 404, store error warn + 500, absent 410 naming no domain); handlers `domain_entry`, `domain_changes`, `domain_history`, `entry_read`, `entry_changes`, path params via `Result<Path<String>, PathRejection>`.
- crates/web/src/api/mod.rs: the five routes in `read_routes()` behind the per-IP limiter.
- tests/fixtures/contracts/api-v1.openapi.json regenerated (compatible growth).
- No lens or SDD change (§7.1 already lists the endpoints).

## Phase 3 — Fresh runs

### Groups

G1: C1 (and C19's 429 mapping) · G2: C2 · G3: C3–C20 (C19's refusal before `admit`). G3 needs G1's route; G2 is independent.

### Plan

#### G1 — the route in netray-web
1. `crates/web/src/api/error.rs`: `V1Error` gains `Throttled`, `BudgetExhausted`, `Busy` (429 + Retry-After) and `PrefetchRefused` (503, `Cache-Control: no-store`).
2. `crates/web/src/api/runs.rs`: `RunAnswer`, `RunSectionEvent` (from `netray_engine::SectionEvent`), the `Recorded` → `RunAnswer` mapping, handler `start_run` (no body extractor): validate, client as `limit::client_ip`, `is_prefetch`, `start(.., Api, prefetch)`, refusal mapping, SSE (`section` … `done`) or JSON after `Recorded`; a stream ending early → 500.
3. `crates/web/src/api/mod.rs`: schemas `RunAnswer`, `RunSectionEvent`; `run_routes()` merged into `openapi()` and into `router()` outside the `[api]` limiter's `route_layer`.
4. `tests/fixtures/contracts/api-v1.openapi.json`: regenerated (`UPDATE_GOLDEN=1 cargo test -p netray-web --test openapi_contract`).

Tests: `cargo test -p netray-web --test api_runs --test openapi_responses --test openapi_contract`.

#### G2 — requirement 37 in lens
1. `crates/lens/src/state.rs`: `AppState::before_admit` (default `None`).
2. `crates/lens/src/admission.rs`: uncounted refusal builder plus `count_rejection`; `admit_uncounted` (today's `admit` body; `admit` counts); `join_recording(state, domain)` returns a joining `RunStream` when the domain's run has published `Done`.
3. `crates/lens/src/routes.rs` `run_check_handler`: keep the prior head id; a fresh held head first tries `join_recording`; await `before_admit`; on a `Throttle` refusal re-read the head and answer a different, fresh, held entry as a cache hit, else count and refuse.

Tests: `cargo test -p lens --test admission_head_race` plus the `admission_*` and `history_cache*` regressions.

#### G3 — `LensRuns::start`
1. `crates/lens/src/api_v1.rs`: `check_limits` → `Refused(IpLimit|GlobalLimit)`; `admit` (the counting one) with the trigger mapped; events mapped `Section`/`Done(run)`/`Recorded`; a fresh run counts `count_request("fresh")` and `client_runs.record`, a join `count_request("joined")`, as `/api/check` does.

Tests: `cargo test -p lens --test api_v1_runs --test api_v1_runs_join --test api_v1_runs_limits`.

## Phase 4 — ip. functions under v1 and the curl root

### Groups

G1: C8, C19, C26, C27 (the netray-ip seam, guarded by the `offline_routes` pins; the netray-ip half of C7 and C9) · G2: C7, C18, C25 (`[api.ip]` keys and `validate()`, manifests) · G3: C1–C6, C9–C17, C20–C24, C28 (v1 IP routes, `ApiState` seam, wiring in `crates/netray`). G3 needs G1 and G2.

### Plan

#### G1 — netray-ip seam
1. `crates/ip/src/lookup.rs` (new): `IpLookup` (object-safe, `BoxFuture`), `IpRefusal`, pub `enrich`, `check_address`, `parse_address`, `parse_diff_pair`, `parse_range`, `range_lookup`, `asn_lookup`, `diff`, `lookup_batch`; moved `RangeResponse`, `AsnLookupResponse`, `DiffRequest` (schema attributes verbatim) and new `DiffResponse` (`IpDiff`, `LocationDiff`, `NetworkDiff`, `TcpDiff`, `UserAgentDiff`, keys sorted). Lifted from `get_ifconfig`, `is_global_ip`, `is_special_prefix`, `range_handler`, `asn_lookup_by_number`, `diff_handler`'s diff block, `batch_dispatch`'s semaphore and timeout loop.
2. `crates/ip/src/lib.rs`: `pub mod lookup; pub use lookup::{IpLookup, IpRefusal};`.
3. `crates/ip/src/extractors.rs`: `pub fn header_filters(&[String]) -> RegexSet` (lifted from `AppState::new`); `state.rs` calls it.
4. `crates/ip/src/routes.rs`: `dispatch`, `ip_version_dispatch`, `batch_dispatch`, `diff_handler`, `asn_param_handler`, `range_handler` call the lookup functions and map refusals onto today's codes (`INVALID_IP`, `INVALID_CIDR`, `TIMEOUT`); answers byte-identical; `/diff` keeps the caller UA.
5. `crates/ip/src/handlers.rs`: `make_ifconfig`/`make_ifconfig_lang` removed once unused.
6. `crates/ip/src/module.rs`: `impl IpLookup for IpModule` (`lookup`: `enrich(ip, None, skip_dns)`; `lookup_caller`: `enrich(remote, ua, dns)`; `range`; `asn`); `Module::run` maps `Err(r)` to its text.
7. `crates/ip/src/testing.rs`: `StubIpLookup` (`new`, `Default`) per the interface sheet.

Tests: `cargo test -p netray-ip --lib` and `cargo test -p netray-ip --features testing --test module --test module_data --test module_v3 --test contract_status --test evidence --test offline_routes`.

#### G2 — `[api.ip]` keys
1. Root `Cargo.toml`: `netray-ip` in `[workspace.dependencies]`; `crates/web/Cargo.toml` `netray-ip` and `regex` as dependencies, the dev-dependency `{ workspace = true, features = ["testing"] }`; `crates/netray/Cargo.toml` `netray-ip = { workspace = true }`.
2. `crates/web/src/api/config.rs`: `ApiIpConfig`'s nine keys with a hand-written `Default`; `ApiIpConfig::validate` in the sheet's order and texts (caps from `netray_ip::config::HARD_CAP_*`); `ApiConfig::validate` calls it.
3. `crates/lens/lens.example.toml`: a commented `[api.ip]` block with the keys and defaults.

Tests: `bash tests/repo/test_check_config.sh`, `bash tests/repo/test_config_strict.sh`, `cargo test -p lens --lib config`.

#### G3 — v1 IP routes
1. `crates/web/src/api/error.rs`: `V1Error::BatchTooMany`; `From<IpRefusal>`.
2. `crates/web/src/api/state.rs`: `ApiParts { ip: Option<Arc<dyn IpLookup>> }`; `IpLimiters { per_ip, per_target, batch_addresses }` built in `ApiState::new`; accessors `ip_limiters`, `ip_lookup`; the header filter set.
3. `crates/web/src/api/ip.rs` (new): six handlers in the sheet's order (503, per-IP charge, validation, per-target or batch limiter, lookup), `AddressField`/`CallerField`, the field → `to_plain` mapping, batch types, `ip_routes()` and `ip_doc_routes()`, `range_missing`.
4. `crates/web/src/api/mod.rs`: `mod ip`; document merges `ip_doc_routes()`; router merges `ip_routes()` outside the `[api]` limiter, `/ip/range/{*cidr}` and `/ip/range` by hand; `/meta` `limits.ip`.
5. `crates/netray/src/main.rs`: the lens arm passes `ApiParts { ip: <the IpModule> }` to `lens::run_with`.
6. `tests/fixtures/contracts/api-v1.openapi.json`: regenerated.

Tests: `cargo test -p netray-web --test api_ip_lookup --test api_ip_fields --test api_ip_batch --test api_ip_range --test openapi_responses --test openapi_contract --test openapi_document`, then `just adlc-verify`.

## Phase 5 — TLS and HTTP inspectors

Binding interface: the spec's Phase 5 decisions and the Phase 5 interface sheet (TlsBackend, TlsInspector, HttpInspect, `[api.tls]`/`[api.http]`, compose).

## Groups

G1: C12, C14 · G2: C2 · G3: C3, C5, C6, C7, C8, C9, C10, C11 · G4: C1, C4, C13

G1 (netray-tls) and G2 (netray-http) share no file; G3 needs both; G4 needs G3. A criterion with test files in two groups sits in the group where its last test turns green. G3 breaks the `netray` binary build (its `ApiParts` literal) until G4 lands; no `--workspace` run between them.

## Plan

### G1 — netray-tls: backend seam and inspection type
- `crates/tls/src/backend.rs` (new): `TlsBackend` (`resolve`, `handshake`), `LiveBackend { resolver }` over `resolve_hostname` (`routes.rs:1295`) and `tls::inspect_ip`.
- `crates/tls/src/state.rs`: `AppState.backend`; `new` → `LiveBackend { None }`, `init_dns_resolver` → `Some(..)`.
- `crates/tls/src/routes.rs`: `resolve_target` → `state.backend.resolve`; `inspect_port` takes the backend; `do_inspect` calls `state.inspector().inspect_parsed(..)`, keeps counter, span, `x-cert-*`.
- `crates/tls/src/inspector.rs` (new): `TlsInspector`, `TlsInspectRequest`, `TlsRefusal` + `From<AppError>`; `inspect_parsed` = the moved `do_inspect` block; `inspect` = `parse_input`, port 0 refused, `ports = [port]`; `AppState::inspector`.
- `crates/tls/src/module.rs`: `TlsModule::inspector`.
- `crates/tls/src/security/rate_limit.rs`: `Quotas`, `Scopes`, `scoped`, `prune_if_due`; `new` = tlsight scopes.
- `crates/tls/src/lib.rs`: modules and re-exports. `crates/tls/src/testing.rs`: `StubTlsBackend`, `stub_config`, `stub_state` (installs the ring provider).
- Command: `cargo test -p netray-tls --features testing --test inspector`; `--lib`; regression `cargo test -p netray-tls --features testing`.

### G2 — netray-http: job seam
- `crates/http/src/job.rs` (new): `HttpInspect` (`target`, `inspect`), `run`, `inspect`, `HttpInspectRequest`, `HttpRefusal` + `From<AppError>`.
- `crates/http/src/routes.rs`: `do_inspect_inner` → `job::run`; `impl HttpInspect for AppState` (with `ip_detail_base`).
- `crates/http/src/module.rs`: `impl HttpInspect for HttpModule`.
- `crates/http/src/security/rate_limit.rs`: `Quotas`, `Scopes`, `scoped`, `prune_if_due`.
- `crates/http/src/lib.rs`, `crates/http/src/testing.rs`: re-exports, `StubHttpInspect` (real `pick_target`).
- Command: `cargo test -p netray-http --features testing --test job`; `--lib`; regression `cargo test -p netray-http --features testing`.

### G3 — netray-web: config, state, errors, routes, composition
- Root `Cargo.toml`: `netray-tls`, `netray-http` in `[workspace.dependencies]`; `crates/web/Cargo.toml`: both as deps, dev-deps `workspace = true, features = ["testing"]`.
- `api/config.rs`: `ApiTlsConfig` (30/10/60/20), `ApiHttpConfig` (10/5/30/10), zero rules `api.<t>.<key> must be greater than 0`, called after `ip.validate()`.
- `api/error.rs`: `V1Error::TargetBlocked` (403 `TARGET_BLOCKED`); `From<TlsRefusal>`, `From<HttpRefusal>`.
- `api/state.rs`: `ApiParts { ip, tls, http }`; `tls_limiters`/`http_limiters` via `scoped` (scopes `api_v1_tls[_target]`, `api_v1_http[_target]`), accessors.
- `api/ip.rs`: `json_body`, `Peer`, `peer_of` `pub(super)`.
- `api/inspectors.rs` (new): `tls_inspect`, `http_inspect`, doc routes, `routes()`; order 503 → body → client + prune → job → `V1Error::from`.
- `api/compose.rs` (new): component-collision renaming in parts, spectra's two maps typed.
- `api/mod.rs`: `openapi()` composes; `router()` merges inspector routes outside the `[api]` limiter; `/meta` `limits.tls`/`limits.http`.
- Command: `cargo test -p netray-web --test api_tls --test api_tls_policy --test api_tls_limits --test api_http --test openapi_document`; `--lib`; regression the `api_ip_*` and `api_edges` tests.

### G4 — binary wiring, example config, fixture
- `crates/netray/Cargo.toml`: `workspace = true`. `crates/netray/src/main.rs`: `lens_registry` returns the modules; `ApiParts { ip, tls: Some(tls.inspector()), http: Some(http as Arc<dyn HttpInspect>) }`; `Shared<T>` forwards `needs_address_records` and `wants_facts` (else lens runs change).
- `crates/lens/lens.example.toml`: commented `[api.tls]`/`[api.http]` blocks.
- `tests/fixtures/contracts/api-v1.openapi.json`: regenerate last (`UPDATE_GOLDEN=1 cargo test -p netray-web --test openapi_contract`).
- Command: `cargo test -p netray-web --test openapi_contract --test openapi_responses`; `cargo build -p netray && bash tests/repo/test_check_config.sh`; `cargo test -p lens --test api_v1_mount`; then `just adlc-verify`.

## Phase 6 — Email inspection

Binding interface: the spec's Phase 6 decisions and the Phase 6 interface sheet (EmailInspector/EmailInspect, EmailJob, InspectSlots, `[api.email]`, unnamed SSE frames, two-step compose).

## Groups

G1: C1, C4, C5 (crate halves), C6 (guard) · G2: C1 (route, `[api.email]`), C2, C3, C4, C5 (route halves) · G3: C1 (`startup_rejects`), C2, C3 (`openapi_responses` against the regenerated fixture)

Strict order G1 → G2 → G3. G2 breaks the `netray` build (its `ApiParts` literal) until G3.

## Plan

### G1 — netray-email job seam (beacon unchanged)
- `crates/email/src/security/rate_limit.rs`: `Quotas`, `Scopes`, `scoped`, `prune_if_due`; `new` = beacon scopes.
- `crates/email/src/quality/types.rs`: `SseEvent::Evidence.blocks` typed as `Block`.
- `crates/email/src/job.rs` (new): `EmailInspector<R>::start` (parse, selectors, limiter, slot, spawn `run_all_checks`), `EmailInspect`, `EmailJob` (abort on drop, `collect`), `InspectSlots`, `EmailInspectRequest`, `EmailInspectResult`, `EmailSummary`, `EmailRefusal` + `From<MailError>`; `AppState::inspector`, `EmailModule::inspector`.
- `crates/email/src/lib.rs`: module and re-exports. `crates/email/src/routes.rs`: both handlers through `start`, metrics/gauge/keep-alive unchanged; `validate_and_rate_limit` and `AbortOnDropStream` removed. `crates/email/src/testing.rs`: `example_dns`, `stub_inspector`.
- Command: `cargo test -p netray-email --features testing --test job --test inspect_route`; regression `cargo test -p netray-email --features testing`; `cargo build -p netray`.

### G2 — netray-web: `[api.email]`, state, route, document
- Root `Cargo.toml` + `crates/web/Cargo.toml`: `netray-email` (dev-dep with `testing`).
- `api/config.rs`: `ApiEmailConfig` 10/10/16, zero rules, called after `http`.
- `api/state.rs`: `ApiParts.email`, `email_limiter` (`api_v1_email`), `email_slots`, accessors.
- `api/error.rs`: `From<EmailRefusal>` (Busy → 503 naming the slots).
- `api/runs.rs`: `wants_event_stream` `pub(super)`. `api/email.rs` (new): handler 503 → body → client + prune → inspect → SSE or collected JSON (500 without a summary); doc routes.
- `api/mod.rs`: two-step compose (`Email` prefix), router merge, `/meta` `limits.email`.
- Command: `cargo test -p netray-web --test api_email --test api_email_limits --test openapi_document`; regression `cargo test -p netray-web` (`openapi_responses` red until G3).

### G3 — binary wiring, example config, fixture
- `crates/netray/Cargo.toml` workspace dep; `crates/netray/src/main.rs`: `LensModules.email`, `Shared(email.clone())`, `ApiParts.email`.
- `crates/lens/lens.example.toml`: commented `[api.email]` block.
- Fixture regenerated last (only additions).
- Command: `cargo build -p netray && bash tests/repo/test_check_config.sh`; `UPDATE_GOLDEN=1 cargo test -p netray-web --test openapi_contract`, then `cargo test -p netray-web --test openapi_contract --test openapi_responses --test openapi_document`; then `just adlc-verify`.
