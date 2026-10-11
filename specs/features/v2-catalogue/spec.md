# Spec: v2 catalogue

Status: Done
Created: 2026-10-10
Finished: 2026-10-11

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
- beacon's `info` verdict (e.g. no DKIM keys found) was neutral inside V1's buckets (`crates/email/src/module.rs:531`); `beacon-no-mx.sse` and `beacon-null-mx.sse` carry dkim `info`.
- `tls.tls_reachable` fails only when every address failed with a target-side error (`crates/tls/src/quality/checks.rs:251`; SDD §3.9).
- A module that does not wait for the resolve stage runs with empty `Facts` (`crates/engine/src/lib.rs:218`).
- Stored `/r/` snapshots carry V1 check names and no texts; the renderer looks labels up by name (`crates/lens/src/snapshot/render.rs:106`); `earned` is an integer (`snapshot/types.rs:190`).
- The HTTP contract golden carries only hsts, csp, x_frame_options, cors and info_leakage (`crates/http/tests/contract_golden.rs:126`).
- Grades change in this release (0.25.0); the release notes state it.

## Requirements

1. **Catalogue.** `crates/engine/catalogue/checks.toml` (embedded) holds the 54 checks of §3.9, one entry per ID with `id`, `protocol`, `title`, `finding`, `impact`, `fix_hint`, `fix_owner` (one of `dns_provider`, `registrar`, `certificate_provider`, `web_server`, `mail_provider`, `hosting`), `severity` (`critical`, `high`, `medium`, `low`), `source`, `guide`, `short` (≤ 8 characters, newly drafted). `netray_engine::catalogue()` loads it; a test fails on a missing field, an unknown enum value, a `short` over 8 characters, a duplicate ID or a count other than 10/21/12/10/1. Texts come from lens's `routes.rs` copies where copies disagree (their guide pages exist).
2. **Profile v3 and scoring in the engine.** `crates/engine/profiles/v3.toml` (embedded) weights exactly the catalogue's IDs (each ≥ 1) with §3.9's section weights, unchanged checks' weights and its mechanical split; hard fail `tls.chain_trusted`, `tls.not_expired`, `tls.tls_reachable`; V1's thresholds. `Profiles::get(n)` / `Profiles::current()`; a lock file holds each profile's SHA-256 and a test fails when a released profile changes; a test fails when a catalogue ID lacks a weight or a weight names no catalogue ID. `netray_engine::score(&Profile, sections) -> Score` carries lens's V1 rules (N/A sections excluded, a section without possible points incomplete, hard fail → F, thresholds) except that a `warn` earns exactly half its weight. lens is not changed in this phase.
3. **The switch to v3.** In one phase, with the module work as parallel groups:
   - (a) DNS emits exactly the 10 `dns.*` IDs (no `infrastructure`); a check prism reported nothing for is `not_applicable` with finding "not reported". IP emits `ip.reputation` and builds its context without the User-Agent parser, logging nothing about it.
   - (b) HTTP emits the 12 `http.*` IDs: `https_redirect` from `http_upgrade` with `redirect_limit` as its finding, the other 11 one per spectra quality check; a check spectra did not emit is `not_applicable` "not reported". HTTP keeps measuring one address.
   - (c) Email emits the 10 `email.*` IDs, one per beacon category, with §3.9's routing (`mta_sts_without_tls_rpt`, `dane_without_tls_rpt` → `email.tls_rpt`; `dane_without_dnssec` → `email.dane`; `sends_no_mail` stays with `email.spf` and still suppresses `reject_no_dkim` and `spf_mx_coverage`); beacon `info` and `skip` give `not_applicable` with the message as finding; the seven MX-dependent IDs are `not_applicable` for no MX or Null MX; `email.spf` and `email.dmarc` are always measured.
   - (d) TLS emits all 21 `tls.*` IDs in every run. `tls.tls_reachable` keeps its rule (fail only when every address failed with a target-side error). Every other per-address check is measured on each address of port 443 that completed a handshake and reports the worst of them, naming the failing address in its finding; `tls.caa_compliant`, `tls.dane_valid` and `tls.ocsp_stapled` stay once per run. When `tls.tls_reachable` fails, the other 20 are `not_applicable` "no TLS handshake"; `tls.aia_reachability` is `not_applicable` when the chain is complete; `tls.consistency` and `tls.alpn_consistency` with one address. `Module::wants_facts()` (new, default false; true for TLS): the engine lets such a module wait for the resolve inside its own window, and on a resolve error runs it with empty `Facts`; TLS takes CAA and the HTTPS RR from `Facts` and looks up itself only when they are empty.
   - (e) No module emits an ID outside its protocol, an ID twice, or misses a catalogue ID; no ID has two emitters (a test over every module's golden).
   - (f) lens scores through `netray_engine::score` with profile v3; `crates/lens/src/scoring/` and the tests importing it move to the engine's API (`ADLC-Test-Change`). lens keeps its JSON, SSE and snapshot shapes, each section's check list holding the v3 names; titles, fix hints, owners and guide URLs come from the catalogue; `not_applicable` findings show as the check's message; snapshot `earned` becomes a decimal (old entries still read); old snapshots' V1 names render through a V1 label table until they expire. `crates/lens/frontend/src/lib/checkMeta.ts` lists the 54 names and titles, and a repo check fails when they differ from `checks.toml`. The lens result goldens and full-output goldens are regenerated (`ADLC-Test-Change` per file), and the report explains each grade change.
4. **Review before the release.** A table of all 54 catalogue entries (every field and the v3 weight) and the grade shift of profile v3 against V1 on the baseline aggregates (`specs/baseline/2026-10-09-v1-snapshots.json`, planning repo) is handed to the operator before `just release 0.25.0`; the release notes state the grade changes and the new check names.

## Phase 1 — Catalogue

**Depends on:** none
**Requirements:** 1

### Test Scenarios

- GIVEN `checks.toml` WHEN loaded THEN 54 entries, every field present, `fix_owner` and `severity` valid, every `short` ≤ 8 characters, IDs unique, 10/21/12/10/1 per protocol.
- GIVEN a catalogue copy with a `short` of 9 characters, a missing `impact`, or a duplicate ID WHEN loaded THEN refused naming the entry.

## Phase 2 — Profile v3 and scoring

**Depends on:** 1
**Requirements:** 2

### Test Scenarios

- GIVEN `profiles/v3.toml` WHEN loaded THEN every catalogue ID has a weight ≥ 1 and every weight a catalogue ID; section weights 20/35/20/15/10; §3.9's split weights.
- GIVEN the lock file WHEN a byte of `v3.toml` changes THEN the lock test fails.
- GIVEN a section of one weight-1 check that warns WHEN scored THEN 0.5 of 1.
- GIVEN V1's scoring cases (N/A section excluded, no possible points → incomplete, hard fail → F, thresholds) WHEN scored by the engine THEN V1's results.

## Phase 3 — Switch to v3

**Depends on:** 2
**Requirements:** 3

### Test Scenarios

- GIVEN `prism.sse` WHEN the DNS module translates THEN 10 `dns.*` IDs; those prism did not report are `not_applicable` "not reported"; no `dns.infrastructure`.
- GIVEN the IP module WHEN loaded THEN no "user_agent_regexes" log line.
- GIVEN `spectra-inspect.json` WHEN translated THEN 12 `http.*` IDs; the golden's five checks carry spectra's status, the others are `not_applicable` "not reported"; GIVEN `redirect_limit_reached` THEN `http.https_redirect` carries the finding and no `redirect_limit` check exists.
- GIVEN `beacon.sse` WHEN translated THEN 10 `email.*` IDs; GIVEN `dane_without_tls_rpt` THEN its finding sits on `email.tls_rpt`; GIVEN `beacon-null-mx.sse`, `beacon-no-mx.sse` THEN the seven MX-dependent IDs are `not_applicable` and `email.spf`, `email.dmarc` measured; dkim `info` → `not_applicable` with its message.
- GIVEN a valid and an expired-certificate address that both completed a handshake WHEN translated THEN `tls.not_expired` fails naming the expired address; GIVEN one address with a handshake and one with `HANDSHAKE_FAILED` THEN `tls.tls_reachable` passes and the others are measured on the reachable one; GIVEN none completing THEN `tls.tls_reachable` fails and the other 20 are `not_applicable` "no TLS handshake".
- GIVEN a stub module with `wants_facts()` WHEN the run's resolve succeeds THEN it sees the Facts; WHEN the resolve fails THEN it runs with empty Facts (not incomplete).
- GIVEN every module's golden WHEN run THEN each emits exactly its protocol's catalogue IDs, none twice, none by two modules.
- GIVEN lens WHEN it renders a section THEN the check list holds the v3 names with the catalogue's texts; GIVEN a 0.24.0 snapshot with `email_authentication` WHEN rendered THEN it shows its V1 label and guide link.
- GIVEN `checkMeta.ts` with a name or title differing from `checks.toml` WHEN the repo check runs THEN it fails.
- GIVEN the regenerated lens goldens WHEN reviewed THEN the report explains each grade change (split weights, exact half-warn, the hygiene fix, N/A by design).

## Decision log

- One switch phase after the catalogue and the engine profile, over golden regeneration per module phase: the gate stays green per phase (operator, 2026-10-10, after the independent reading).
- beacon `info` → `not_applicable` with its message, as V1's buckets treated it; `email.dkim` follows beacon's verdict (operator, 2026-10-10).
- `wants_facts`: TLS waits for the resolve and falls back on a failed one, over looking up in every run; revises the earlier fallback-only choice (operator, 2026-10-10).
- Defaults accepted by the operator (2026-10-10): N/A reasons as findings; DNS "not reported" N/A; `sends_no_mail` stays with spf; caa/dane/ocsp once per run; failing address named in the finding; HTTP one address; new `short`s; routes.rs texts win; V1 label table for old snapshots; decimal `earned`; R3(e) checks duplicates; the checkMeta check compares names and titles.
- `tls.tls_reachable` keeps its all-addresses rule; other checks run on addresses that completed a handshake (SDD §3.9; independent reading).

- Phase 1b as three features: `v2-catalogue`, `v2-unmeasured`, `v2-evidence` (operator, 2026-10-10).
- Profile v3 takes §3.9's mechanical split now; the operator reviews weights and the grade shift against the baseline before the release (operator, 2026-10-10).
- lens keeps its V1 output shape with the v3 checks; external API users see new check names, stated in the release notes (operator, 2026-10-10).
- Catalogue texts drafted from lens's current copies, owners mapped to the six values, severity from weight and hard fail; reviewed as a table (operator, 2026-10-10).
- Scoring moves into the engine with loadable profile versions (operator, 2026-10-10).
- A warn earns exactly half its weight (operator, 2026-10-10).
- TLS reads CAA and the HTTPS RR from Facts when present, else looks up itself; it does not wait for the resolve (operator, 2026-10-10).
- `checkMeta.ts` follows the catalogue by hand until Phase 4, guarded by a repo check (operator, 2026-10-10).
- The services' own APIs keep their checks; only the modules emit the catalogue (the SDD's "removed from DNS/TLS" speaks of the modules; operator, 2026-10-10, in the spec question).

## Open decisions

None.

## Out of scope

- The planning repo's SDD amendments: 42 weighted V1 checks (not 41), 14 `fix_owner` values (not 13), the `hygiene` skip defect.
