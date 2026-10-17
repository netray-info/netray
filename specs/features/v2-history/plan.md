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

## Phase 4 — lens records every run

## Groups

G1: C7, C20, C21 (crates/http/src/module.rs, crates/dns/src/module.rs).
G2: C1–C5, C8–C18 (crates/lens: Cargo.toml, config.rs, state.rs, history.rs new, check.rs, lib.rs, metrics.rs, routes.rs; lens.example.toml, lens.dev.toml).
G3: C19 (gate rows in justfile and tests/repo/*.sh; protected files, edited with ADLC-Test-Change).
C6: SDD amendments, prose.

## Plan

### G1
Append the R17 paths to http VOLATILE (module.rs ~98) and the dns `records_blocks!` VOLATILE (module.rs ~73, plus `dns.rrsig/data`). Command: `cargo test -p netray-http --test history_volatile && cargo test -p netray-dns --test history_volatile`.

### G2
Per contract scratchpad/v2-history-lens-api.md: `HistoryConfig` beside `SnapshotsConfig`; `AppState.history_store`; `history.rs` with `volatile`, `build_run`, `record`; call at the end of `run_check_with_evidence`; startup open/migrate/exit 1 and hourly sweep in lib.rs beside the snapshot store; counter in `init_zero_series`; badge reads `score.complete`. Command: `cargo test -p lens --test history_record --test history_badge --test history_config`.

### G3
`LENS_HISTORY__DB_PATH=$tmp/history.db` beside every `LENS_SNAPSHOTS__DB_PATH` row; admission script sets it in the environment of `start_bg`. Command: `bash tests/repo/test_history_db_path.sh`.

## Phase 5 — Rotating address sets are volatile

## Groups

G1: C2 · G2: C4, C7 · G3: C1, C3 (after G2: the fingerprint must drop whole blocks). C5, C6, C8 already_implemented.

## Plan

### G1
1. crates/engine/src/lib.rs — `EvidencePath::parse` refuses a `*` key with a row key; doc comments name `<block name>/*`.

### G2
1. crates/model/src/canonical.rs — `volatile_for` refuses `*` with a row; new `pub(crate) fn block_volatile(id, paths) -> bool` (a `*` key without row, block matched without qualifier).
2. crates/model/src/run.rs — `fingerprint` leaves out every block `block_volatile` covers; doc names `<block name>/*`.
3. crates/model/src/diff.rs — `blocks_of`/`evidence_changes` exclude covered blocks before the id union, so they are never added, removed or changed.

### G3
1. crates/email/src/module.rs — `EmailModule::volatile` returns `email.mx_addresses/*`, `email.fcrdns/*`, `email.dnsbl/*` (the list was empty; the SDD's email TTLs and timings were never declared).
2. specs/sdd/v2.md §3.7 — grammar gains `<block>/*`; email row lists the three whole blocks; `email.mx` and `dns.a`/`dns.aaaa` stay compared.

## Phase 6 — Domain-zone DNSBL rows stay compared

## Groups

G1: C1, C2, C3, C4 (one production file). C5 checked by regenerating the lens goldens; C6 is SDD prose.

## Plan

### G1
1. crates/email/src/module.rs — the evidence projection (the `m.dnsbl` branch, ~line 593) splits answers by whether `target` parses as an `IpAddr`: address rows stay in `email.dnsbl`, the rest go to a new table `email.dnsbl_domain` (same columns, key `query`), pushed only when non-empty; register the block (`ShapeKind::Table`) and its five keys beside `email.dnsbl`'s; `references` adds `email.dnsbl_domain` for `dnsbl` (absent blocks are dropped by the caller).
2. specs/sdd/v2.md §3.7 — the email evidence row describes the split; the email volatile row names the per-address facts the whole-block paths hide (MX address change of the same hosts, PTR change still forward-confirmed, a further zone listing an already-listed address) and that `email.dnsbl_domain` stays compared.
