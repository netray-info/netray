# Plan: v2-history

## Phase 1 — Run, fingerprint and diff in netray-model

## Groups

G1: C1–C19 (one group: `lib.rs` declares `run` and `diff`; fingerprint and evidence diff share the canonicalisation; `tests/fingerprint.rs` imports `diff`).

## Plan

### G1
1. `crates/model/Cargo.toml`: `sha2` and `serde_json` (workspace) under `[dependencies]`; still no workspace crate.
2. `crates/model/src/lib.rs`: `Grade` derives `Deserialize`; `pub mod run; pub mod diff; mod canonical;`.
3. `crates/model/src/run.rs`: `Run`, `RunSection`, `SectionKind`, `RunCheck`; `fingerprint` = SHA-256 hex over compact JSON of profile_version, checks sorted by id, blocks sorted by id as `canonical_block`.
4. `crates/model/src/canonical.rs`: volatile path parsing (second copy of `EvidencePath::parse`, Rule of Three), matching by block module+name whatever the qualifier; `canonical_block` drops volatile keys/cells, sorts records, table rows by key, repeated fields keys by value, normalises CSP nonces.
5. `crates/model/src/diff.rs`: contract types; `diff` over checks (R4/R5), evidence (blocks of sections measured in both, per-shape items), grade, `engine_changed`.
6. SDD §3.7 (repeated fields keys) and §4.3 (not measured, precedence, release classes) amended in the phase commit (CLAUDE.md: SDD in the same commit as the code; R16 lists them for Phase 4, done here instead).

Group command: `cargo test -p netray-model`

## Phase 2 — Profile-effect grade in netray-engine

## Groups

G1: C1–C4 (one new module).

## Plan

### G1
1. `crates/engine/src/history.rs` (new): `rescore(run, profile)` rebuilds one `SectionEvent` per `RunSection` (Measured with the run's checks of that protocol as `CheckResult`, NotApplicable, Incomplete, TimedOut) and calls `scoring::score`; `diff(prev, cur, volatile, cur_profile)` calls `netray_model::diff::diff` with `Some(rescore(prev, cur_profile))` exactly when the profile versions differ.
2. `crates/engine/src/lib.rs`: `pub mod history;`.

Group command: `cargo test -p netray-engine --test history`

## Phase 3 — netray-store

## Groups

G1: C1–C16 (one crate; record, latest and sweep share the pool, the write lock and the latest-entry query).

## Plan

### G1
1. Root `Cargo.toml`: `sqlx = "0.8"` and `nanoid = "0.4"` into `[workspace.dependencies]`; `crates/lens/Cargo.toml` takes both with `workspace = true` and its feature list (second user, CLAUDE.md).
2. `crates/store/Cargo.toml`: `sqlx` (runtime-tokio, sqlite, migrate), `nanoid`, `serde_json`, `tokio` (sync), `netray-model`.
3. `crates/store/migrations/0001_create_entries.sql`: table `entries` and index per the contract, portable SQL only.
4. `crates/store/src/lib.rs`: `HistoryStore` (pool as `crates/lens/src/snapshot/store.rs`, WAL, foreign keys on; a `tokio::sync::Mutex<()>` serialising record and sweep), `StoreError`, `Recorded`, `Entry`, `RETENTION_SECS`; id via nanoid 8 over `[0-9A-Za-z]` with collision retry; `record` reads the latest entry, compares `netray_model::run::fingerprint`, updates or inserts in one transaction; `latest` orders by `created_at DESC` and, among ties, takes the entry no other entry of the domain names as predecessor; `sweep(now)` deletes in one transaction per R11.

Group command: `cargo test -p netray-store`
