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

## Phase 2 — Profile v3 and scoring

### API contract

- `crates/engine/profiles/v3.toml` (embedded): V1 shape — `[meta] name = "v3"`, `version = 3`; `[sections.<protocol>] weight`, `hard_fail = ["chain_trusted", …]` (section-local names); `[sections.<protocol>.checks] <name> = <weight>`; `[thresholds] "A+" = 97 …`. The check ID is `<protocol>.<name>`.
- `crates/engine/profiles/lock.toml`: `v3 = "<sha256 hex of v3.toml>"`.
- `netray_engine::profile::{Profile, SectionWeights, Profiles, ProfileError}`:
  - `Profile::from_toml(&str) -> Result<Profile, ProfileError>`; fields `name: String`, `version: u32`, `sections: BTreeMap<Protocol, SectionWeights>`, `thresholds: Vec<(Grade, u32)>` (descending). `SectionWeights { weight: u32, checks: BTreeMap<CheckId, u32>, hard_fail: Vec<CheckId> }`. Refuses a weight 0, a check name that is no valid ID, a hard-fail name not among the section's checks.
  - `Profiles::get(version: u32) -> Option<&'static Profile>`, `Profiles::current() -> &'static Profile` (v3).
  - `netray_engine::profile::check_lock(version: u32, toml: &str) -> Result<(), ProfileError>`: Err when the SHA-256 of `toml` differs from the lock entry or the version has none.
- `netray_engine::score(profile: &Profile, sections: &[SectionEvent]) -> Score`; `netray_engine::scoring::{Score, SectionScore}`:
  - `Score { sections: BTreeMap<Protocol, SectionScore>, overall_percentage: f64, grade: Grade, complete: bool, hard_fail_checks: Vec<CheckId>, not_applicable: BTreeMap<Protocol, String> }`; `SectionScore { earned: f64, possible: u32, percentage: f64 }`.
  - V1 rules: Pass earns the weight, Warn exactly half (f64), Fail 0; NotApplicable, NotTested and Unmeasured are excluded from earned and possible (V1 Skip; the Unmeasured split is `v2-unmeasured`). Checks without a profile weight are ignored. `Measured` with possible 0, `Incomplete` and `TimedOut` make the score incomplete (grade `Incomplete`); `NotApplicable` sections are excluded and listed; a profile section absent from `sections` is ignored. No finished section → `Incomplete`. A hard-fail check with status Fail forces `F` on a complete score. Overall = Σ percentage·weight / Σ weight over finished sections; grade by thresholds.

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R2: v3.toml weights exactly the catalogue's IDs (each ≥ 1), sections 20/35/20/15/10, §3.9 split weights, unchanged V1 weights, hard fail tls.{chain_trusted,not_expired,tls_reachable}, thresholds A+ 97 … F 0; `Profiles::current()` is v3 | green | crates/engine/tests/profile.rs |
| C2 | the lock: embedded v3.toml matches; a changed byte fails `check_lock` | green | crates/engine/tests/profile.rs |
| C3 | a section of one weight-1 check that warns scores 0.5 of 1 | green | crates/engine/tests/score.rs |
| C4 | V1's scoring cases hold in the engine (N/A excluded, possible 0 → incomplete, Incomplete/TimedOut → incomplete, hard fail → F, thresholds) | green | crates/engine/tests/score.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 profile v3, lock, engine scoring | 2 | opus | 56361 | 162 |

### Reader

COUNTS blockers=0 amendments=0 deferred=1 nits=4. v3.toml weights exactly the 54 IDs (V1 weights unchanged, §3.9 split), lock hash matches, rules equal V1 except the exact half-warn.

- DEFERRED: a weighted `unmeasured` check is excluded and the section stays complete (V1 Skip); SDD §3.5 makes it incomplete. Belongs to `v2-unmeasured`, whose change of `crates/engine/tests/score.rs:112` needs an `ADLC-Test-Change`.
- NIT: hard fail matches any duplicate of a check ID (V1: the first); duplicates are refused from phase 3 on (R3(e)).
- NIT: thresholds are not checked for grade order, an empty table parses (as V1).
- NIT: no `deny_unknown_fields` on the profile structs; a misspelt `hard_fails` drops every hard fail (as V1; v3's test pins its hard fails).
- NIT: workspace `sha2` is 0.10 while `crates/tls` pins 0.11 directly.

### Behavioural verification

skipped: library-only phase; lens still scores through its own module until phase 3.
