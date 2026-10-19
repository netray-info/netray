# Plan: admission-prefetch-race

## Phase 1 — Deregister before Recorded

## Groups

- G1: C1, C2, C3, C4, C5, C6 (one production file).

## Plan

- G1: `crates/lens/src/admission.rs`, `run_task`: bind the `Deregister` guard (`deregister`) and
  drop it after `cache::hold` and before `tx.send_modify(|s| s.recorded = ...)`; `Deregister`'s
  doc comment states the new order. The guard's `Drop` and its `same_channel` check are unchanged.
  Planned directly by the implementer: one statement moves in one function.
