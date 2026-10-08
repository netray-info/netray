# Plan: one outbound fetch policy

## Phase 1 — Results table

### Groups

One group: C1–C11 (beacon tables) share `crates/beacon/src/state.rs`. C12 (spectra) passes today and needs no production change.

### Plan

- `crates/beacon/src/state.rs`: extract the two inline client builders (`:46-54`, `:56-73`) into `pub(crate) fn http_client_builder(timeout_ms: u64) -> reqwest::ClientBuilder` and `pub(crate) fn http_client_follow_builder(timeout_ms: u64) -> reqwest::ClientBuilder`, each returning the builder before `.build()` with today's timeout, redirect policy and user agent. The state constructor calls them and `.build()`s as before. Behaviour unchanged.

The planner agent was not spawned: one file, two functions, nothing to group.
