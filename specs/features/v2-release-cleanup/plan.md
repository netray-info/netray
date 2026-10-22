# Plan: v2-release-cleanup

## Phase 1 — Image-run budget

## Groups

- G1: C1 to C17 (one group: every criterion hangs on `gate`, its state and its config).

## Plan

- `crates/web/src/admission/image_budget.rs`: `ImageBudget` token bucket (`peek`, `take`) on tokio's clock.
- `crates/web/src/admission/mod.rs`: `Reason::ImageBudget`, `count_rejection` label `image_budget`, `gate` peeks the budget before the recompute limiter and takes it after.
- `crates/web/src/admission/state.rs`: `ApexState::image_budget` from `AdmissionLimits::image_runs_per_minute`.
- `crates/web/src/admission/runs.rs`: map `ImageBudget` (unreachable for `DomainRuns`).
- `crates/web/src/api/state.rs`, `crates/web/src/config.rs`: the field and its default 10.
- `crates/web/src/metrics.rs`: zero series `image_budget`.
- `crates/netray/src/config.rs`: `[admission] image_runs_per_minute`, default, `validate` zero, `apex_config`; `serve.rs` startup log; `netray.example.toml`.
- `specs/sdd/v2.md`: S44 and §4.4.

## Phase 2 — Label and logo

## Groups

- G1: C1 to C7.

## Plan

- `crates/web/src/config.rs` `default_badge_label`, `crates/web/src/og/layout.rs` `DEFAULT_LABEL`: "netray".
- `crates/web/templates/base.html`: the logo's `href` through `frame.origin()`.
- `crates/netray/netray.example.toml`: `default_label = "netray"`.

## Phase 4 — Crate docs describe V2

## Groups

- G1 to G6: one crate each (dns, tls, http, email, ip, common); no shared file.

## Plan

- `crates/<c>/README.md`, `CLAUDE.md`: rewritten from the code (module type, checks, `[modules.<c>]`, `[api.<c>]`, `[inspect.<c>]`, root verbs).
- `crates/<c>/CHANGELOG.md`: pointer paragraph before the first entry, history unchanged.
