# Plan: email scoring

## Phase 1 — beacon

### Groups

One group: the verdict order, the run loop, cross-validation, DKIM and the exports share `crates/beacon/src/checks/mod.rs` and `quality/types.rs`.

### Plan

- `quality/types.rs`: reorder `Verdict` to Skip, Info, Pass, Warn, Fail (derived `Ord`); add `Category::ALL: [Category; 12]` in declaration order. Check every place that relies on the old order (beacon's own rollups, frontend types if any) — the grade counts only Warn/Fail.
- `checks/mod.rs`: `pub const SKIPPED: &str = "skipped"` used by `skip_result`; after phase 1/2 joins, send a `SseEvent::Category` for every category that falls back to `skip_result` (before the summary). Compute `sends_no_mail = null_mx || spf_only_dash_all` once and pass it to `check_dkim` and into `AllResults`.
- `checks/spf.rs`: expose "the record's only mechanism is `-all`" (e.g. `spf_only_dash_all`) from the parse it already does.
- `checks/mx.rs`: `pub const NULL_MX: &str = "null_mx"`, used by the sub-check.
- `checks/cross_validation.rs`: `pub const SENDS_NO_MAIL`, `pub const CROSS_VALIDATION_CHECKS: &[&str]` (every name it can emit, incl. `sends_no_mail`); `AllResults.sends_no_mail`; emit `sends_no_mail` Info first when set, with a neutral detail (e.g. "domain declares it sends no mail (Null MX or SPF -all only)"); the category's issue count and verdict ignore it (so with nothing else: Pass, "all cross-validation checks passed").
- `checks/dkim.rs`: `check_dkim(..., sends_no_mail: bool)`; empty `p=` → `key_revoked` Info and not counted as usable; revoked-only → category Info (sends no mail) or Warn, detail "only revoked DKIM keys", returned `found` false; a usable key → as today.
- Goldens: the contract test's scenario details for `sends_no_mail` must equal the real detail string — align the literal in `crates/beacon/tests/contract_golden.rs` (orchestrator) or the code; then `UPDATE_GOLDEN=1 cargo test -p beacon --test contract_golden` and `UPDATE_GOLDEN=1 cargo test -p beacon --lib timeout_golden`.
- Review fixes: DKIM revoked-only raises to Warn only for senders and never lowers; SPF `only_dash_all` ignores modifiers, mechanism names case-insensitive; beacon frontend `VERDICT_ORDER` follows the server.
