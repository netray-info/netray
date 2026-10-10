# Report: v2 catalogue

## Phase 1 — Catalogue

### API contract

- `netray_engine::catalogue::{Catalogue, CheckSpec, CatalogueError}`; `Catalogue::from_toml(&str) -> Result<Catalogue, CatalogueError>`; `netray_engine::catalogue() -> &'static Catalogue` (the embedded `crates/engine/catalogue/checks.toml`, parsed once); `Catalogue::entries() -> &[CheckSpec]`, `Catalogue::get(&CheckId) -> Option<&CheckSpec>`.
- `CheckSpec { id: CheckId, protocol: Protocol, title, finding, impact, fix_hint: String, fix_owner: FixOwner, severity: Severity, source: String, guide: String, short: String }` (FixOwner, Severity, Protocol, CheckId from netray-model; model gains `Deserialize` for them).
- TOML shape: `[[check]]` tables with those keys; `id = "tls.chain_trusted"`.

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: checks.toml holds the 54 checks with every field; loader refuses missing field, unknown enum, short > 8, duplicate ID, wrong per-protocol count | green | crates/engine/tests/catalogue.rs |
| C2 | the embedded catalogue: 54 entries, valid enums, shorts ≤ 8, unique IDs, 10/21/12/10/1 | green | crates/engine/tests/catalogue.rs |
| C3 | a copy with a 9-character short, a missing impact or a duplicate ID is refused naming the entry | green | crates/engine/tests/catalogue.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 catalogue loader + checks.toml | 1 | opus | 58841 | 203 |
| G1 text fixes, reader pass 1 (13 blockers) | 1 | opus | 132028 | 336 |
| G1 text fixes, reader pass 2 (9 blockers) | 1 | opus | — | — |

### Reader

- Pass 1: loader and model change sound; 13 BLOCKERs, all catalogue texts or severities contradicting what the checks measure (e.g. `tls.tls_version` described TLS 1.0/1.1 while the client negotiates only 1.2/1.3; `email.dmarc` recommended `p=quarantine`, which warns). Fixed; severity made mechanical (hard fail → critical, weight ≥ 5 high, 3–4 medium, ≤ 2 low; header comment of `checks.toml`).
- Pass 2: 9 BLOCKERs, new ones (routed email cross-checks not described, chain-wide `not_expired`, `ns_delegation` comparing authoritative servers, owners that cannot act). Fixed.
- Pass 2 also found five **check code defects**; operator decision (2026-10-10): the catalogue describes the intended behaviour, the code fixes are a separate feature `v2-check-fixes` before 0.25.0:
  - `http.cors`: every ACAO value other than `*` counts as reflected (`crates/http/src/inspect/cors.rs:17`), so an explicit allowlist with credentials fails.
  - `tls.caa_compliant`: any CAA record with the critical flag fails (`crates/tls/src/validate/caa_compliance.rs:21`); RFC 8659 refuses only unknown critical tags.
  - `tls.strong_signature`: unmapped algorithms rank weakest-but-pass (`crates/tls/src/validate/chain_trust.rs:147`, `crates/tls/src/tls/chain.rs:283`), so ecdsa-with-SHA1 passes; a served root's SHA-1 self-signature fails.
  - `tls.chain_complete` / `tls.aia_reachability`: a leaf sent alone passes both (`chain_trust.rs:106`, `crates/tls/src/quality/mod.rs:57`).
  - `tls.chain_trusted`: fails on hostname mismatch too (rustls `verify_server_cert` ends in `verify_server_name`), duplicating `tls.hostname_match` under a hard fail.
- DEFERRED (NIT): no loader test for an ID prefix that differs from `protocol`, none for `short` counted in chars vs bytes.

### Amendments

- AMENDMENT | phase 1 | repaired_in_phase: no — no guide page covers `http.deprecated_headers` or `http.caching`; per the routes.rs rule they link `/guide/http-redirects` (which holds only the info-leakage text). Site content gap, out of this feature.
- AMENDMENT | phase 1 | repaired_in_phase: yes — "severity from weight and hard fail" had no rule; fixed as above.

### Behavioural verification

skipped: phase 1 adds a library loader and data with no callable entry point; lens is unchanged until phase 3.
