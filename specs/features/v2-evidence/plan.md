# Plan: v2-evidence

## Phase 1 — Model and engine

### Groups

- G1: C1–C8 (one group: `crates/model/src/evidence.rs`, `crates/model/src/lib.rs`, `crates/engine/src/lib.rs`)

### Plan

1. New `crates/model/src/evidence.rs` with the following items, exported as `netray_model::evidence`:
   - `BlockId`: parsed with the check-ID name rule. Its qualifier is an address, `address:port`, `[v6]:port` or a port. It serialises as a string.
   - `Block`, `Shape` with its six variants, and `Record`.
   - `Value`, serialised tagged as `{t, v}` through a custom serde implementation.
   - `ValueType` and `Register`.
   - `validate(module, register, blocks)`, which checks key, type, prefix and the registered block name, for every shape's keys and columns.
   - `check_references`.
   - `EVIDENCE_SCHEMA_VERSION`.
2. `CheckResult.evidence: Vec<BlockId>`, with `Deserialize`. `crates/model/tests/model.rs` uses BlockIds (ADLC-Test-Change).
3. `SectionOutcome::Measured` gains `evidence: Vec<Block>`. All 53 literal sites get `evidence: vec![]`, which is mechanical. Each test file touched carries an ADLC-Test-Change in a separate commit. `EvidencePath` documents the two path forms.

`cargo test -p netray-model --test evidence && cargo test --workspace --exclude netray-ip && cargo test -p netray-ip --features testing`
