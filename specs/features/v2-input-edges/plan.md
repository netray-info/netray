# Plan: v2-input-edges

## Phase 1 — IDNA mapping in the domain rule

## Groups

- G1: C1–C14 (one production file, `crates/common/src/domain.rs`; every caller inherits it).

## Plan

- G1: `crates/common/src/domain.rs` `validate`: the address checks move into `refuse_address`,
  run on the trimmed input, then `idna::uts46::Uts46::to_ascii` (URL deny list, hyphens allowed,
  DNS length ignored), then `refuse_address` again on the mapped form; wildcard, length and label
  checks run on the mapped form. `idna` becomes a workspace dependency of `netray-common`.
  Planned and coded by the orchestrator: one function in one file, no planner or coder spawned.
