# Report: v2 catalogue

## Phase 1 — Catalogue

### API contract

- `netray_engine::catalogue::{Catalogue, CheckSpec, CatalogueError}`; `Catalogue::from_toml(&str) -> Result<Catalogue, CatalogueError>`; `netray_engine::catalogue() -> &'static Catalogue` (the embedded `crates/engine/catalogue/checks.toml`, parsed once); `Catalogue::entries() -> &[CheckSpec]`, `Catalogue::get(&CheckId) -> Option<&CheckSpec>`.
- `CheckSpec { id: CheckId, protocol: Protocol, title, finding, impact, fix_hint: String, fix_owner: FixOwner, severity: Severity, source: String, guide: String, short: String }` (FixOwner, Severity, Protocol, CheckId from netray-model; model gains `Deserialize` for them).
- TOML shape: `[[check]]` tables with those keys; `id = "tls.chain_trusted"`.

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: checks.toml holds the 54 checks with every field; loader refuses missing field, unknown enum, short > 8, duplicate ID, wrong per-protocol count | open | crates/engine/tests/catalogue.rs |
| C2 | the embedded catalogue: 54 entries, valid enums, shorts ≤ 8, unique IDs, 10/21/12/10/1 | open | crates/engine/tests/catalogue.rs |
| C3 | a copy with a 9-character short, a missing impact or a duplicate ID is refused naming the entry | open | crates/engine/tests/catalogue.rs |
