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

## Phase 4 — `@system`

### Groups

G1: C2 · G2: C7 · G3: C3, C4, C5 · G4: C1, C6 (no production change, only report statuses)

### Plan

#### G1 (C2)
- `crates/mhost-prism/src/api/meta.rs` — `ClientConfig` gains a documented, always-serialized `allow_system_resolvers: bool`, set in `client_config` from `state.config.dns.allow_system_resolvers`. The refusal in `security/query_policy.rs:81-91` stays. No golden covers `/api/config`.

#### G2 (C7)
- `crates/mhost-prism/tests/fixtures/prism.production.toml` — `[dns]` gains `allow_system_resolvers = false` after `default_servers` (argus-oci's key-path comparison needs the key present).

#### G3 (C3, C4, C5)
- `crates/mhost-prism/frontend/src/lib/servers.ts` — new: the one server list. `serverSuggestions(allowSystem)` and `helpServerRows(allowSystem)` append `@system` only when `allowSystem === true`; `helpServerRows` keeps the `@1.2.3.4` custom-IP row; `systemResolversAllowed(cfg)` is true only for an object whose `allow_system_resolvers` is strictly `true`.
- `crates/mhost-prism/frontend/src/components/QueryInput.tsx` — `SERVERS` removed; `prismCompletions` becomes a factory over a getter of the new optional prop `allowSystemResolvers`, so a later config answer takes effect without remounting. `tokenizer.ts` unchanged.
- `crates/mhost-prism/frontend/src/App.tsx` — signal `allowSystem` (starts false), set from `systemResolversAllowed(cfg)` in the `onMount` `/api/config` fetch; failures leave it false; passed to `<QueryInput>`; the help modal's "Predefined servers" rows come from `helpServerRows(allowSystem())`.

#### G5 (C8, added by the operator's amendment of req 6)
- `crates/mhost-prism/src/api/parse.rs` — `parse_handler` takes `State<AppState>` and, after `completions_at`, drops the `@system` completion when `!state.config.dns.allow_system_resolvers`; `completions_at`'s signature stays (its unit tests call it).

#### G4 (C1, C6)
- Report statuses only: C6 already holds (`QueryPolicy::check_system_resolvers`).
