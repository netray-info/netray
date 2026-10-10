# Spec: v2 catalogue

Status: Draft
Created: 2026-10-10

## Goal

Every module emits exactly the check IDs of one catalogue file in every run, scored by profile v3 inside `netray-engine` (V2 SDD Phase 1b, V1.8–V1.12): `checks.toml` holds the 54 checks of §3.9 with every field of §3.5, the roll-ups of HTTP and email are split, TLS checks are scored over every tested address, `profiles/v3.toml` is locked by hash, and lens serves its V1 output shape with the v3 checks.

## Non-goals

- `unmeasured` per check, S31 (definite negatives) and S32 (error skips) — feature `v2-unmeasured`.
- Evidence shapes, registered keys, volatile paths; the transitional `presentation` stays — feature `v2-evidence`.
- The services' own HTTP APIs and UIs: prism keeps its email lints and `infrastructure`, tlsight keeps its `hsts`/`https_redirect` for its own page; only the modules change.
- The V2 UI (Phase 4).

## Context and constraints

- V2 SDD (planning repo `specs/sdd/v2.md`): §3.5 check model (CheckSpec fields), §3.8 profiles, §3.9 catalogue v3 and initial weights, Phase 1b V1.8–V1.12.
- Today (monorepo main at 0.24.0):
  - DNS emits the 10 v3 DNS IDs plus `infrastructure` (conditional, unweighted) (`crates/dns/src/module.rs:35-52`, api/check.rs:636).
  - TLS declares the 21 v3 IDs but emits them conditionally and reads only the first port's first successful address (`crates/tls/src/module.rs:192-213`, `quality/mod.rs:32-74`); CAA and ECH come from tlsight's own lookups (routes.rs:774-813).
  - HTTP emits 6 checks; `security_headers` and `hygiene` are roll-ups with a `max()` over spectra's `Pass < Skip < Warn < Fail`, so `hygiene` reads `skip` on a healthy site (`crates/http/src/module.rs:149-234`; `tests/fixtures/contracts/lens-full-healthy.json:132-134`); `redirect_limit` exists only when hit.
  - Email emits 4 bucket roll-ups (`crates/email/src/module.rs:24-39`, routing :182-214); `mta_sts_without_tls_rpt`, `dane_without_tls_rpt`, `dane_without_dnssec` route by prefix to transport.
  - IP emits `reputation` (matches v3).
- Scoring lives in lens (`crates/lens/src/scoring/`, profile `crates/lens/profiles/default.toml` version 2, 42 weighted checks, hard fail `tls.{chain_trusted, not_expired, tls_reachable}`, thresholds A+ 97 … F 0; Warn earns `w/2` by integer division, NotFound and Fail 0, Skip excluded; a section with no possible points is incomplete).
- Per-check texts are in three copies: `crates/lens/src/routes.rs` `fix_for` (:109-371) and `guide_url_for` (:61-106), `crates/lens/src/snapshot/render.rs` labels and guide URLs (:716-808, which disagree with routes.rs for HTTP), `crates/lens/frontend/src/lib/checkMeta.ts`; `fix_owner` has 14 free-text values.
- The IP module logs "user_agent_regexes not configured; User-Agent parsing disabled" at every load (`crates/ip/src/enrichment.rs:147`), although lens never parses a User-Agent (argus, 2026-10-10).
- Grades change in this release (0.25.0); the release notes state it.

## Requirements

1. **Catalogue.** `crates/engine/catalogue/checks.toml` (embedded) holds the 54 checks of §3.9, one entry per ID with `id`, `protocol`, `title`, `finding`, `impact`, `fix_hint`, `fix_owner` (one of `dns_provider`, `registrar`, `certificate_provider`, `web_server`, `mail_provider`, `hosting`), `severity` (`critical`, `high`, `medium`, `low`), `source`, `guide`, `short` (≤ 8 characters). `netray_engine::catalogue()` loads it; a test fails on a missing field, an unknown enum value, a `short` over 8 characters, a duplicate ID or a count other than 54. Texts are drafted from lens's current copies; the operator reviews the table before the release.
2. **Profile v3 in the engine.** `crates/engine/profiles/v3.toml` (embedded) weights exactly the catalogue's IDs (each ≥ 1) with §3.9's section weights, unchanged checks' weights and its mechanical split of the roll-ups; hard fail `tls.chain_trusted`, `tls.not_expired`, `tls.tls_reachable`; V1's thresholds. `Profiles::get(n)` / `Profiles::current()`; a lock file holds each profile's SHA-256 and a test fails when a released profile changes. A test fails when a catalogue ID lacks a weight or a weight names no catalogue ID.
3. **Scoring in the engine.** `netray_engine::score(&Profile, sections) -> Score` carries lens's V1 rules (N/A sections excluded, a section without possible points incomplete, hard fail → F, thresholds) with one change: a `warn` earns exactly half its weight (no integer rounding). lens's `crates/lens/src/scoring/` is removed and lens maps the engine's score into its V1 output.
4. **DNS and IP emit their catalogue IDs.** The DNS module emits exactly the 10 `dns.*` IDs (no `infrastructure`); the IP module `ip.reputation`.
5. **HTTP split.** The HTTP module emits the 12 `http.*` IDs, one per spectra quality check; `redirect_limit` becomes a finding of `http.https_redirect`; a check spectra did not emit is `not_applicable` with a reason.
6. **Email split.** The email module emits the 10 `email.*` IDs, one per beacon category, with §3.9's sub-check routing (`mta_sts_without_tls_rpt`, `dane_without_tls_rpt` → `email.tls_rpt`; `dane_without_dnssec` → `email.dane`); `email.mx`, `.mta_sts`, `.tls_rpt`, `.bimi`, `.dane`, `.dnsbl`, `.fcrdns` are `not_applicable` for no MX or Null MX; `email.spf`, `.dkim`, `.dmarc` always measured.
7. **TLS over every address.** The TLS module emits all 21 `tls.*` IDs in every run, each measured on every address tlsight inspected for port 443 and reported as the worst over tested addresses (addresses not tested from here excluded; all excluded → `not_tested`); when `tls.tls_reachable` fails, every other TLS check is `not_applicable` "no TLS handshake"; `tls.aia_reachability` is `not_applicable` when the chain is complete; `tls.consistency` and `tls.alpn_consistency` with one address; `tls.caa_compliant` and `tls.ech_advertised` read CAA and the HTTPS RR from `Facts` when present, else from tlsight's own lookup.
8. **One emitter per check.** A test runs every module's golden and fails when a module emits an ID outside its catalogue protocol, a catalogue ID is missing from a run, or two modules emit one ID.
9. **lens V1 output with v3 checks.** lens keeps its JSON, SSE and snapshot shapes; each section's check list holds the v3 checks (the name part of the ID); titles, fix hints, fix owners and guide URLs come from the catalogue (one source, replacing `fix_for`, `guide_url_for` and the snapshot copies); `crates/lens/frontend/src/lib/checkMeta.ts` lists the 54 names and a repo check fails when they differ from `checks.toml`. The lens result goldens and full-output goldens are regenerated (`ADLC-Test-Change` per file).
10. **IP module log.** The IP module builds its enrichment context without the User-Agent parser and logs no warning about it.
11. **Review before the release.** A table of the 54 catalogue entries (title, fix owner, severity, short, weight) and the grade shift of profile v3 against V1 on the baseline aggregates (`specs/baseline/2026-10-09-v1-snapshots.json`, planning repo) is handed to the operator before `just release 0.25.0`.

## Phase 1 — Catalogue

**Depends on:** none
**Requirements:** 1

### Test Scenarios

- GIVEN `checks.toml` WHEN loaded THEN 54 entries, every field present, `fix_owner` and `severity` valid, every `short` ≤ 8 characters, IDs unique, 10/21/12/10/1 per protocol.
- GIVEN a catalogue copy with a `short` of 9 characters, a missing `impact`, or a duplicate ID WHEN loaded THEN refused naming the entry.

## Phase 2 — Profile v3 and scoring

**Depends on:** 1
**Requirements:** 2, 3

### Test Scenarios

- GIVEN `profiles/v3.toml` WHEN loaded THEN every catalogue ID has a weight ≥ 1 and every weight a catalogue ID; section weights 20/35/20/15/10; the split weights of §3.9.
- GIVEN the lock file WHEN a byte of `v3.toml` changes THEN the lock test fails.
- GIVEN a section of one weight-1 check that warns WHEN scored THEN 0.5 of 1.
- GIVEN V1's scoring cases (N/A section excluded, no possible points → incomplete, hard fail → F, thresholds) WHEN scored by the engine THEN the same results as lens's V1 engine.

## Phase 3 — DNS and IP IDs

**Depends on:** 1
**Requirements:** 4, 10

### Test Scenarios

- GIVEN `prism.sse` WHEN the DNS module translates THEN exactly the 10 `dns.*` IDs, no `dns.infrastructure`.
- GIVEN the IP module WHEN loaded THEN no "user_agent_regexes" log line; `ip.reputation` as before.

## Phase 4 — HTTP split

**Depends on:** 1
**Requirements:** 5

### Test Scenarios

- GIVEN `spectra-inspect.json` WHEN translated THEN 12 `http.*` IDs, each with spectra's status for its check; `http.deprecated_headers`, `.info_leakage`, `.caching` pass on the healthy golden (no `skip` roll-up).
- GIVEN a response with `redirect_limit_reached` WHEN translated THEN `http.https_redirect` carries the redirect-limit finding and no `redirect_limit` check exists.

## Phase 5 — Email split

**Depends on:** 1
**Requirements:** 6

### Test Scenarios

- GIVEN `beacon.sse` WHEN translated THEN 10 `email.*` IDs with beacon's category verdicts and the routed sub-checks' findings.
- GIVEN a `dane_without_tls_rpt` sub-check WHEN translated THEN its finding sits on `email.tls_rpt`, not on `email.dane`.
- GIVEN `beacon-null-mx.sse` and `beacon-no-mx.sse` WHEN translated THEN the seven MX-dependent IDs are `not_applicable` and `spf`, `dkim`, `dmarc` measured.

## Phase 6 — TLS over every address

**Depends on:** 1
**Requirements:** 7

### Test Scenarios

- GIVEN two addresses, one valid and one with an expired certificate WHEN translated THEN `tls.not_expired` fails.
- GIVEN no address completing a handshake WHEN translated THEN `tls.tls_reachable` fails and the other 20 IDs are `not_applicable` "no TLS handshake".
- GIVEN one address WHEN translated THEN `tls.consistency` and `tls.alpn_consistency` are `not_applicable`.
- GIVEN Facts with CAA records WHEN the TLS module runs THEN `tls.caa_compliant` uses them (no own CAA lookup); GIVEN empty Facts THEN it looks up itself.

## Phase 7 — lens with v3 checks

**Depends on:** 2, 3, 4, 5, 6
**Requirements:** 8, 9, 11

### Test Scenarios

- GIVEN every module's golden WHEN run THEN each emits exactly its protocol's catalogue IDs and no ID twice across modules.
- GIVEN lens WHEN it renders a section THEN the check list holds the v3 names, and titles, fix hints, owners and guide URLs equal the catalogue's.
- GIVEN `checkMeta.ts` with a name missing from `checks.toml` WHEN the repo check runs THEN it fails.
- GIVEN the regenerated lens goldens WHEN reviewed THEN each grade change is explained by the split weights, the exact half-warn or the hygiene fix (listed in the report).
- GIVEN the review table and the grade shift WHEN handed to the operator THEN `just release 0.25.0` waits for the go.

## Decision log

- Phase 1b as three features: `v2-catalogue`, `v2-unmeasured`, `v2-evidence` (operator, 2026-10-10).
- Profile v3 takes §3.9's mechanical split now; the operator reviews weights and the grade shift against the baseline before the release (operator, 2026-10-10).
- lens keeps its V1 output shape with the v3 checks; external API users see new check names, stated in the release notes (operator, 2026-10-10).
- Catalogue texts drafted from lens's current copies, owners mapped to the six values, severity from weight and hard fail; reviewed as a table (operator, 2026-10-10).
- Scoring moves into the engine with loadable profile versions (operator, 2026-10-10).
- A warn earns exactly half its weight (operator, 2026-10-10).
- TLS reads CAA and the HTTPS RR from Facts when present, else looks up itself; it does not wait for the resolve (operator, 2026-10-10).
- `checkMeta.ts` follows the catalogue by hand until Phase 4, guarded by a repo check (operator, 2026-10-10).
- The services' own APIs keep their checks; only the modules emit the catalogue (default, not interviewed: the SDD's "removed from DNS/TLS" speaks of the modules).

## Open decisions

None.

## Out of scope

- The planning repo's SDD amendments: 42 weighted V1 checks (not 41), 14 `fix_owner` values (not 13), the `hygiene` skip defect.
