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
