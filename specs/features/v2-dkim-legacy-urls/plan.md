## Phase 1 — Old site URLs answer 301

## Groups

- G1: C1-C10 (crates/web/src/pages/guides.rs, crates/web/src/pages/mod.rs)

## Plan

- G1: `guides::show` maps a legacy slug through `legacy_target` before the lookup; `guides::moved` answers the 301; `pages::router` adds `/compare/`, `/status/`, `/tools`, `/tools/`. SDD §7.3 gains two rows.
