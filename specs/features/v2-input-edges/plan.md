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

## Phase 2 — Refuse a cost no wait can admit

## Groups

- G1: C1, C2, C5–C10 (DNS: `crates/dns/src/error.rs`, `crates/dns/src/jobs/mod.rs`,
  `crates/dns/src/security/rate_limit.rs`).
- G2: C3, C11–C14 (TLS: `crates/tls/src/error.rs`, `crates/tls/src/inspector.rs`,
  `crates/tls/src/security/rate_limit.rs`).
- G3: C15 (`crates/web/src/api/jobs.rs`), after G2.

## Plan

- G1: `ApiError::CostExceedsLimit { cost, limit }` (422 `COST_EXCEEDS_LIMIT`, no retry);
  `DnsRefusal::from` maps it to `InvalidInput`; `RateLimitState` keeps its `Quotas` and
  `check_query_cost` compares the costs with the bursts before the stream check.
- G2: `AppError::CostExceedsLimit` likewise; `TlsRefusal::from` to `InvalidInput`;
  `RateLimitState::cost_within_bursts` called first by `check_cost`; `remaining_budget` the smaller
  burst; the inspector's not-one-address refusal becomes `CostExceedsLimit`.
- G3: `admit_tls` lets `CostExceedsLimit` through uncharged; `start_tls` answers
  `cost_within_bursts`'s message as the notice.
- Planned and coded by the orchestrator: three small groups, no planner or coder spawned.
