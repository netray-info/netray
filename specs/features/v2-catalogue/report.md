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

## Phase 3 — Switch to v3

### API contract

- `netray_engine::Module::wants_facts(&self) -> bool` (default `false`; TLS `true`). `run` lets such a module wait for the resolve inside its own window; on a resolve error it runs with empty `Facts` (not `Incomplete`). `needs_addresses()` keeps its meaning (IP).
- Every module's `checks()` returns exactly its protocol's catalogue IDs (`netray_engine::catalogue()`), and every `Measured` outcome carries each of them exactly once. Missing results are `Status::NotApplicable` with the reason as the only finding: DNS/HTTP `"not reported"`, TLS `"no TLS handshake"` (when `tls.tls_reachable` fails), beacon `info`/`skip` messages, the MX-dependent email IDs `"no MX"` / `"Null MX"`.
- lens: `crates/lens/src/scoring/` is removed; lens calls `netray_engine::score(netray_engine::profile::Profiles::current(), …)`. JSON/SSE/snapshot shapes unchanged; check names are the v3 names without protocol prefix as today's shape uses them; titles, fix hints, owners and guide URLs from `netray_engine::catalogue()`; snapshot `earned` is a decimal and old integer entries still deserialize; old snapshots' V1 names render through a V1 label table.
- `tests/repo/test_checkmeta_catalogue.sh`: fails when `crates/lens/frontend/src/lib/checkMeta.ts` names or titles differ from `checks.toml`.

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R3(a) DNS: `prism.sse` → exactly the 10 `dns.*` IDs, unreported ones `not_applicable` "not reported", no `dns.infrastructure` | green | crates/dns/tests/module_v3.rs |
| C2 | R3(a) IP: loading the module logs no "user_agent_regexes" line; it emits only `ip.reputation` | green | crates/ip/tests/module_v3.rs |
| C3 | R3(b) HTTP: `spectra-inspect.json` → 12 `http.*` IDs; reported ones carry spectra's status, others "not reported"; `redirect_limit_reached` → finding on `http.https_redirect`, no `redirect_limit` check | green | crates/http/tests/module_v3.rs |
| C4 | R3(c) Email: 10 IDs; routing of `dane_without_tls_rpt`/`mta_sts_without_tls_rpt` → tls_rpt, `dane_without_dnssec` → dane; null/no MX → 7 MX-dependent N/A, spf+dmarc measured; dkim info → N/A with its message | green | crates/email/tests/module_v3.rs |
| C5 | R3(d) TLS: 21 IDs every run; worst-of over handshake addresses naming the failing address; reachable rule; all-fail → 20 N/A "no TLS handshake"; aia N/A on complete chain; consistency/alpn N/A with one address | green | crates/tls/tests/module_v3.rs |
| C6 | R3(d) engine: a `wants_facts` module sees the resolved Facts; on resolve error it runs with empty Facts | green | crates/engine/tests/wants_facts.rs |
| C7 | R3(e) every module's golden: IDs within its protocol, none twice, none missing, no ID from two modules | green | crates/lens/tests/catalogue_emitters.rs |
| C8 | R3(f) lens renders v3 names with catalogue texts and scores via the engine; a 0.24.0 snapshot with `email_authentication` renders its V1 label and guide link; integer `earned` still reads | green | crates/lens/tests/v3_render.rs |
| C9 | R3(f) checkMeta guard fails on a differing name or title | green | tests/repo/test_checkmeta_catalogue.sh |
| C10 | R3(f) regenerated goldens; the report explains each grade change | green | (report, below) |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 DNS, IP | 1 | sonnet | 46514 | 151 |
| G2 HTTP | 1 | sonnet | 36313 | 62 |
| G3 Email | 1 | opus | 53835 | 167 |
| G4 engine wants_facts, TLS | 1 | opus | 85442 | 249 |
| G5 checkMeta | 1 | sonnet | 38572 | 58 |
| G6 lens on engine scoring, goldens | 2 | opus | 233332 | 1058 |
| reader fixes | — | opus | 115545 | 852 |

### Grade changes of the regenerated goldens (C10)

Section grades dns/tls/http/email/ip.

| golden | V1 → v3 | sections V1 → v3 | why |
|---|---|---|---|
| healthy | C 73.1 → B 87.5 | C/B/B/F/A+ → C/A+/A/C/A+ | TLS: checks are computed per address; the tlsight golden's hand-written port quality said `expiry_window` warn for a 60-day certificate, tlsight's own rule (30 days) passes it; `ocsp_stapled` from tlsight's rule is warn (the golden's summary said fail). HTTP: `security_headers` (5, warn) is now `csp` (1, warn), the other headers are "not reported". Email: split weights (dkim fail 3, dmarc warn 4, mta_sts warn 2) and the exact half-warn, 12/18. DNS: unreported checks N/A "not reported", grade unchanged |
| null-mx | B 82.8 → A 92.5 | C/B/B/A+/A+ → C/A+/A/A+/A+ | TLS, HTTP as above; email A+: MX-dependent N/A by design, dkim info → N/A |
| no-mx | C 67.8 → B 77.5 | C/B/B/F/A+ → C/A+/A/F/A+ | TLS, HTTP as above; email F (spf and dmarc fail) |
| mx-cname | B 77.8 → B 88.5 | C/B/B/C/A+ → C/A+/A/C/A+ | TLS, HTTP as above; email C (mx fail 2, dnsbl fail 2) |
| http-only | F 44.4 → F 53.1 | C/F/B/F/A+ → C/F/A/C/A+ | still a `tls_reachable` hard fail; the other 20 TLS checks N/A "no TLS handshake" |
| no-address-records | incomplete 29.3 → 46.4 | email F → C | email as in healthy; still incomplete |
| no-weighted-tls | incomplete 68.3 → 81.7 | C/–/B/F/A+ → C/–/A/C/A+ | HTTP, email as in healthy; TLS nothing possible, still incomplete |
| full-beacon-sending-no-dkim | B 79.1 → A 91.2 | C/B/B/B/A+ → C/A+/A/A/A+ | email: `reject_no_dkim` is a dkim warn (3) instead of an auth-bucket warn (10) |
| full-beacon-partial, full-beacon-timeout | incomplete 79.8 → 91.2 | tls B → A+, http B → A | email errored or timed out; still incomplete |

The hygiene skip fix shows in HTTP: `hygiene` (skip) is gone, its three checks are measured or "not reported". The TLS shift of the goldens is mostly a fixture artefact (hand-written port quality inconsistent with tlsight's rules); the real shift is measured against the V1 baseline before the release (R4).

### Reader

COUNTS blockers=1 amendments=0 deferred=3 nits=3; all fixed in this phase except where noted.

- BLOCKER (fixed): the live check list showed a warn as `floor(weight/2)` while the score gives exactly half (`crates/lens/frontend/src/components/CheckList.tsx`); now `weight / 2`, "2.5/5", with a vitest.
- DEFERRED (fixed): `[scoring] profile_path` loaded a profile with non-catalogue IDs and graded everything incomplete; now refused at startup naming the ID.
- DEFERRED (fixed): TLS waits for the resolve; config validation now also requires `resolve_timeout_ms < [backends.tls] timeout_ms`.
- DEFERRED (fixed): V1 cases lost with `crates/lens/src/scoring/` (several hard fails, every threshold row, lens `grade_for`) — `crates/engine/tests/score_v1_cases.rs`, `crates/lens/tests/grade_for.rs`.
- NIT (fixed): email findings carried beacon `info` notes of routed sub-checks; only warn/fail (and `sends_no_mail`) now.
- NIT (fixed): V1 bucket names in user-facing copy (`App.tsx`, `site/guide/lens.html`, `site/api/lens.html`).
- NIT (fixed): the DNS module ran prism's IP enrichment for the dropped `infrastructure` check; it no longer does, and the startup warning is gone.

### Deferred

- `[modules.dns.backends.ip]` is still accepted and now ignored (the production config sets it, `crates/lens/tests/fixtures/lens.production.toml:72`); refusing it is a config change for argus, left for a later feature.
- The TLS section budget (`[backends.tls] timeout_ms` 15000 vs `handshake_timeout_secs` 5 + HSTS checks + the resolve wait of ≤ 1.5 s) does not strictly cover the worst case; it did not before either.
- Five check code defects for `v2-check-fixes` (phase 1 reader).

### Behavioural verification

`cargo test -p lens --test v3_render --test catalogue_emitters` green: a run over every module golden emits exactly the 54 IDs once each, lens's grade equals `netray_engine::score(Profiles::current(), …)` with a weight-5 warn earning 2.5, a 0.24.0 snapshot with `email_authentication` and integer `earned` renders "Authentication" and `/guide/email-auth`.
