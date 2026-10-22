---
class: neighbour-artefact-left-inconsistent
repeat-of: specs/solutions/2026-10-14-golden-scenarios-no-run-can-produce.md
---
# A legacy-key refusal sent the operator to a key the same table refuses

**What failed.** `crates/netray/src/legacy.rs` mapped spectra's top-level `[enrichment]` to `[modules.http].enrichment` and ifconfig's `[rate_limit] exempt_cidrs` to `[api.ip] exempt_cidrs`. Requirement 28 had meanwhile moved enrichment in-process (the same table refuses `modules.http.enrichment`), and `ApiIpConfig` never had `exempt_cidrs`. An operator following either message got a second refusal; the CHANGELOG table, generated row for row, repeated both. The branch review found it (f268717d..90a63b3, B1 and B2); the legacy tests only asserted that a message named the destination.
**What worked.** Both rows became "removed in 0.30.0" (`crates/netray/tests/serve_review_fixes.rs` asserts the message and its absence of the old destination), and the CHANGELOG table was regenerated from `ROWS`.
**How to notice next time.** A `Now("<dest>")` whose destination key is itself a legacy row, or is no field of the config type, is a stale row.
**Check to build.** A test in `crates/netray/tests/` that, for every `Now(dest)` row of `legacy.rs`, writes a file with the destination key at a valid value and asserts `serve --check-config` accepts it.
