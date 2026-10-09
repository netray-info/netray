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

## Phase 2 — IP reputation

### Groups

- G1: C2, C11 (ifconfig-rs)
- G2: C1, C3, C4, C5, C6, C7, C8, C9, C10 (lens)

The groups share no production file. G2 does not depend on G1: the lens tests read the committed `tests/fixtures/contracts/ifconfig-json.json`, which G1 leaves unchanged (`/json`'s order stays).

### Plan

#### G1 (test: crates/ifconfig-rs/src/classify_tests.rs)

1. `crates/ifconfig-rs/src/backend/mod.rs`: add `NetworkFlags` and `classify_network_type(NetworkFlags) -> &'static str` (internal > c2 > bot > cloud > vpn > tor > spamhaus > datacenter > residential); `get_ifconfig` uses it in place of its inline chain. `/network` shares `get_ifconfig` with `/json`, so it needs no change of its own. `infra_type` stays.
2. `crates/ifconfig-rs/src/routes.rs` `range_handler`: look up Tor exit nodes and the bot DB for the network address, derive `is_internal`, fold the ASN VPN heuristic into `is_vpn`, and classify through `classify_network_type`.

#### G2 (tests: crates/lens/tests/ip_reputation_flags.rs, ip_enrichment_errors.rs, ip_sampling.rs, contract_ip.rs, unknown_verdicts.rs)

1. `crates/lens/src/backends/ip.rs`:
   - `NetworkInfo` gains the required `is_spamhaus`, `is_c2`, `is_tor`, `is_vpn`, and `network` loses its serde default.
   - `IpBackend` gains `allow: fn(IpAddr) -> bool`, and `check_ip` takes it as its sixth parameter.
   - The sample is the public addresses, split by family, each family sorted, at most four of each, IPv4 first. `MAX_IPS` goes.
   - A failed or timed-out enrichment returns Err, and the `unknown` arm goes.
   - The verdict comes from the flags, and `network_type_verdict` goes.
   - The messages are `<ip>: <flag>`, plus `checked N of M addresses` when N < M.
   - No public address: `check_ip` returns Ok with no checks, and `IpBackend::run` returns `NotApplicable { reason: "no public addresses" }`.
2. `crates/lens/src/state.rs`: `allow: netray_common::target_policy::is_allowed_target`.
3. `crates/lens/src/routes.rs` (cfg(test) only): `ip_backend_uses_json_path` moves to the new signature and serves a body that carries the flags.
4. `tests/fixtures/contracts/lens-*.json`: regenerated. The only address in `prism.sse` is the documentation address 192.0.2.10, which the production policy does not enrich, so the ip section of each golden becomes N/A.

#### G3 — requirement 7, C12–C17 (test: crates/ifconfig-rs/src/rate_limit_exempt_tests.rs)

Added after the reader's rate-budget AMENDMENT and the operator's decision. One group, planned inline: it touches one crate's config, state and middleware.

1. `crates/ifconfig-rs/src/config.rs` `RateLimitConfig`:
   - Add `#[serde(default)] pub exempt_cidrs: Vec<String>`, with an empty default; `deny_unknown_fields` stays.
   - `Config::validate` rejects an entry that does not parse as an `IpNetwork`, with a message naming `rate_limit.exempt_cidrs`.
2. `crates/ifconfig-rs/src/state.rs`: `AppState` gains `rate_limit_exempt: Arc<Vec<IpNetwork>>`, parsed the way `trusted_proxies` is.
3. `crates/ifconfig-rs/src/middleware.rs` `rate_limit`: the per-IP check is skipped when the request's `ConnectInfo<SocketAddr>` peer lies in `rate_limit_exempt`. The match uses the peer and never the forwarded `RequesterInfo.remote`. The per-target limiter in `routes.rs` is untouched.
4. `crates/ifconfig-rs/tests/fixtures/ifconfig.production.toml`: `exempt_cidrs = []` under `[rate_limit]`, so argus's key-path comparison sees the key.

## Phase 3 — Messages as text

### Groups

- G1: C5, C6, C7 (lens Markdown export)
- G2: C8 (prism Markdown export; reuses G1's helper)

C2, C3, C4 and C9 pass at the baseline: their tests pin what holds. C1 is covered by C2–C9 and by the existing OG/SVG escape tests in `crates/lens/src/og/render.rs`.

### Plan

**G1.**
- `packages/common-frontend/src/utils.ts`: add `inlineCode(value)`. It returns a CommonMark inline code span whose fence is one backtick longer than the longest backtick run in the value. It pads with one space when the value starts or ends with a backtick, or starts and ends with a space. It does not escape `|` or newlines; that is the caller's job. The package is a workspace member and is served from `src` through the existing `./utils` export, so it needs no publish and no version bump.
- `crates/lens/frontend/src/lib/export.ts` `toMarkdown`: put the heading's domain in `inlineCode`. Check messages in `renderSection` and the IP loop become `messages.map(inlineCode).join('; ')`.

**G2.**
- `crates/mhost-prism/frontend/src/lib/export.ts` `toMarkdown`: the heading becomes `inlineCode(ctx.query)`. A table-cell helper next to `mdEscape` collapses newlines, wraps the value with `inlineCode`, then escapes `|`. Name and Value cells use it.
- Test setup already in the tree, committed with this group: prism `package.json` gains the dev deps `jsdom` and `@solidjs/testing-library`; `vitest.config.ts` includes `src/**/*.test.tsx`; `package-lock.json` is updated.

### Groups (amendment, 2026-10-09)

- G3: C10 (beacon) · G4: C11 (tlsight) · G5: C12 (spectra). They share no production file. Each reuses `inlineCode` from `@netray-info/common-frontend/utils`.

### Plan (amendment)

**G3.** In `crates/beacon/frontend/src/components/SummaryCard.tsx` `ExportButtons.copyMarkdown`, the heading domain, `result.detail` and each `sc.detail` go through `inlineCode`.

**G4.** In `crates/tlsight/frontend/src/components/ExportButtons.tsx` `copyMarkdown`, the heading hostname, each chain subject, `ip.error.message` and every quality `c.detail` go through `inlineCode`.

**G5.** In `crates/spectra/frontend/src/components/ExportButtons.tsx` `copyMarkdown`, the following go through `inlineCode`: the heading URL, the enrichment IP, org and threat, quality `c.message`, header values and messages, CSP issues, cookie names (which replace their hand-written single backticks), `cors.message`, and each redirect hop's URL and location. The test setup ships with this group: spectra's `package.json` gains a `test` script and the dev deps vitest, jsdom and @solidjs/testing-library, and `package-lock.json` is updated.

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
