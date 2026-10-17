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
