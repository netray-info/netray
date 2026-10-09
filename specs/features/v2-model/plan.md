# Plan: v2 model

## Phase 1 — Model and mappings

## Groups

G1: C1, C5, C6, C7, C8 · G2: C2, C3, C9, C10, C11, C12, C13 (depends on G1) · G3: C4, C14 (already_implemented, verification only)

## Plan

### G1
- Root `Cargo.toml`: `netray-model = { path = "crates/model" }` in `[workspace.dependencies]`.
- `crates/model/Cargo.toml`: package `netray-model`, workspace version and edition, `license = "MIT"`; `serde` (derive, workspace) only; dev `serde_json`.
- `crates/model/src/lib.rs`: `Protocol`, `CheckId` (`parse`, `protocol`, `name`, hand-written `Serialize` as `<protocol>.<name>`), `CheckIdError`, `Status`, `Severity`, `FixOwner`, `Grade` (`A+`, `incomplete` renames), `CheckResult`; snake_case; no `Deserialize`.
- `tests/repo/test_cargo_workspace.sh`: skip `crates/model/Cargo.toml` in the "depends on netray-common" loop (orchestrator, with `ADLC-Test-Change`).
- `Cargo.lock` regenerated.

### G2
- `netray-model = { workspace = true }` under `[dependencies]` of tlsight, spectra, beacon, lens, prism.
- tlsight: `impl From<CheckStatus> for Status` (`validate/mod.rs`); `pub fn status_of_error_code` (`tls/mod.rs`, beside `error_code`).
- spectra: `impl From<CheckStatus> for Status` (`quality/types.rs`).
- beacon: `impl From<Verdict> for Status`, Info → Pass (`quality/types.rs`).
- lens: `impl From<CheckVerdict> for Status`; `pub fn parse_grade` beside `lookup_grade` (`scoring/engine.rs`).
- prism: `pub fn lint_status(&CheckResult) -> Status` in `api/check.rs`, re-exported in `lib.rs`.

### G3
- `just adlc-verify` and `just deny`; goldens and results tables pass without `UPDATE_GOLDEN`.

## Phase 2 — Engine traits

## Groups

G1: C1–C7 (one crate, one manifest; written by the orchestrator, no planner run: a single group)

## Plan

### G1
- Root `Cargo.toml`: `netray-engine = { path = "crates/engine" }` in `[workspace.dependencies]`.
- `crates/engine/Cargo.toml`: package `netray-engine`, workspace version and edition, `license = "MIT"`; dependencies `netray-model` (workspace) only among workspace crates; dev `tokio` (workspace, `macros`, `rt`).
- `crates/engine/src/lib.rs`: `BoxFuture`, `Module`, `FactsProvider`, `RunContext`, `Facts`, `SectionOutcome`, `EvidencePath`, `Domain`, `ResolveError`, as the tests' contract; no orchestrator.
- `Module::checks` returns `&'static [CheckId]` as SDD §3.6 states; a module holds its list in a `LazyLock` (phase reader: `CheckId` needs no const constructor for that).
