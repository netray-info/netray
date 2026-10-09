# Plan: backend correctness

## Phase 1 — prism lints

### Groups

- G1: C2, C4, C5, C6, C7, C8, C9 (requirement 2), one production file. C1 and C3 (requirement 1) are held: see the report's halt.

### Plan

- G1, `crates/mhost-prism/src/api/check.rs`:
  - `pub fn unique_lines(Vec<CheckResult>) -> Vec<CheckResult>`: drop repeated results, first occurrence and order kept.
  - `fn unique_records(&Lookups) -> Lookups`: one record per name, type and data across every lookup, of duplicates the copy with the highest TTL, built through `Lookups`' serde form (mhost's `Lookup` and `Record` constructors are crate-private).
  - `pub fn lint_lookups(&Lookups) -> Vec<(&'static str, Vec<CheckResult>)>`: the synchronous record lints (caa, cname_apex, dnssec, dnskey_algorithm, dnssec_rollover, https_svcb, mx, ns, spf, ttl) over `unique_records`, each category through `unique_lines`.
  - `post_handler`: lint list built from `lint_lookups`, `ns_lame` and `ns_delegation` inserted after `ns`; the async NS and MTA-STS checks read the unique records; the emit loop passes every category through `unique_lines` before counting.
