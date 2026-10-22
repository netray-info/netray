# Plan: v2-backend-followups

## Phase 1 — HTTP evidence: every Cache-Control directive, unreadable isolation headers

### Groups

1. G1 (C1, C7-C11): `crates/http/src/module.rs` `caching_block` + new `cache_control_directives`.
2. G2 (C3, C4, C14-C18): `crates/http/src/inspect/assembler.rs` (`unreadable_headers`, serde skip),
   `inspect/mod.rs` (filled from the HTTPS headers), `quality/checks.rs` (test literal),
   `module.rs` (`REGISTER`, `isolation_block`, `coop_verdict`).
3. G3 (C2, C5, C12, C13, C19, C20): `crates/web/src/pages/views/http.rs` (`caching`, `isolation`).
4. C6: `specs/sdd/v2.md` §3.9 http row.

### Plan

Written and coded by the orchestrator directly: the change is a few functions whose shape the spec
fixes, so no planner or coder agent was spawned (implementer under the operator's autonomy
mandate, 2026-10-22).

## Phase 2 — Per-port checks in multi-port TLS exports

### Groups

1. G1 (C1-C9): `crates/web/src/export.rs` (`ExportInput.ports`, `PortChecks`, the three formats),
   `crates/web/src/pages/inspect.rs` (`port_checks`, `export_html`, `pending_export_html`),
   `ExportInput` literals in `pages/evidence.rs` and `pages/result/mod.rs`; SDD §6.1.

### Plan

Coded by the orchestrator directly, as in Phase 1 (implementer under the operator's autonomy
mandate, 2026-10-22).
