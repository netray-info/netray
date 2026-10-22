# Spec: v2-correctness-followups

Status: Ready for Implementation
Created: 2026-10-22

## Goal

Three operator decisions of 2026-10-22, taken after `v2-correctness` (#56) on the rows it left
open (private: `netray.info/specs/inventory/sections/10-correctness.md`, rows 10.19, 10.20 and the
rerun's nginx.org observation):

1. A domain that publishes no HTTPS or SVCB record warns on `dns.https_svcb` instead of failing
   it (10.19). A present record keeps its verdict. Scoring profile `v5` marks the grade change.
2. A domain run no longer charges the DNS module's per-target limiter (10.20); lens admission
   bounds domain runs. Direct DNS-inspector requests keep their per-target limit.
3. A legacy-version probe answered with a TLS alert that refuses the ClientHello (nginx.org answers
   SSLv3 with a fatal `unrecognized_name`) counts as refused, as a close does (S42).

## Non-goals

- No change to the suite-group probes (`SuitesInsecure`, `SuitesCbc`, `SuitesStaticRsa`): the
  decision covers legacy-version ClientHellos only.
- No weight, hard-fail, threshold or catalogue-id change: `v5` carries `v4`'s numbers.
- No template, stylesheet or island change (lane A owns them); no V1 frontend change.
- The `[api.dns]` limiter of API v1 and the inspector page is unchanged, and so is the standalone
  `netray dns` service.
- No removal of `[modules.dns.limits] per_target_*`: production's config carries them and every
  config struct denies unknown keys.

## Context and constraints

- `dns.https_svcb` comes from mhost's `check_https_svcb_mode` (`mhost` 0.12.1): no HTTPS and no
  SVCB record is `NotFound`; mixed alias and service mode or several alias records are `Warning`;
  otherwise `Ok`. `netray_dns::module::translate` maps `NotFound` to `fail`
  (`impl From<Verdict> for Status`). A failed HTTPS or SVCB batch already makes the check
  `unmeasured` (Rule M, `CHECK_INPUTS`); that stays.
- Profiles (SDD §3.8): a released profile is never edited; a change is a new file with a lock
  entry; `Profiles::current()` is the highest version; the fingerprint includes the profile
  version, so the first `v5` run of every domain is a new history entry; a stored `v4` run is
  re-scored with `v5`'s weights from its stored statuses, so the grade split cannot carry a
  verdict change (the S41 caveat).
- History fixtures (`tests/acceptance/fixtures/history/`) are recomputed, never hand-set:
  `UPDATE_FIXTURES=1 cargo test -p lens --test it every_history_fixture`. lens goldens
  (`lens-*.json`) and the design fixtures' stored grades follow `score_run`.
- The binary (`crates/netray/src/main.rs`, `lens_registry`) builds two `DnsModule`s from
  `[modules.dns]`: the registry's (domain runs and the facts stage, charging `per_target`
  `CHECK_TOTAL_STEPS` = 19 plus 6 for the facts per resolver key) and the inspector band's
  (`admitted_by_caller`, no per-target charge). Direct DNS jobs run on `dns.jobs()` under the
  `[api.dns]` `RateLimitState` (`api_v1_dns_target`), which never touches the module's limiter.
- CLAUDE.md: "A service moved in-process keeps every check its route applied to lens's calls
  (input validation, per-target limits, policy); only per-client limits go." SDD §3.6: "Modules
  keep every guard their service route applied (input validation, per-target limits, …)". Item 2
  amends both for the DNS module's per-target limit of domain runs.
- TLS probes (`crates/tls/src/tls/probe.rs`): `decide` returns `Refused` for a fatal alert in
  `REFUSING_ALERTS` (40, 70, 71) and `Unanswered` for any other alert; a warning alert other than
  close_notify is skipped. S42 counts a FIN or RST with nothing received as refused and lists
  "an alert other than a fatal handshake_failure, protocol_version or insufficient_security" as
  unanswered. A timeout stays unanswered (Rule M).
- Tests run offline with example.com-family names and documentation addresses.

## Requirements

1. A legacy-version probe (`Ssl3`, `Tls10`, `Tls11`, `Tls12`) whose first answer is an alert that
   refuses the ClientHello is `Refused`, its answer `alert <name>`. Refusing alerts: close_notify
   (0, warning or fatal) and the fatal alerts unexpected_message (10), handshake_failure (40),
   illegal_parameter (47), decode_error (50), protocol_version (70), insufficient_security (71),
   inappropriate_fallback (86) and unrecognized_name (112). A fatal internal_error (80) or
   user_canceled (90), any other or unregistered alert, a timeout, garbage and a partial answer
   stay `Unanswered`; a warning alert other than close_notify is still skipped.
2. A suite-group probe keeps today's rule: only a fatal 40, 70 or 71 refuses, close_notify and
   every other alert leave it unanswered.
3. A domain run charges no per-target limit of the DNS module: the registry's DNS module and the
   facts stage are admitted by lens (`admitted_by_caller`), so a `[modules.dns.limits]`
   `per_target_burst` below 19 never makes the DNS section `incomplete` with a rate-limit reason.
4. A direct DNS-inspector request (API v1 `/api/v1/dns/*`, the inspector page's jobs) keeps its
   per-target limit from `[api.dns]`: past its burst it is refused as today.
5. `dns.https_svcb` is `warn` with the finding "Not found" when the HTTPS and SVCB lookups answer
   no record. Mixed modes and several alias records stay `warn`, well-formed records `pass`, a
   failed HTTPS or SVCB batch `unmeasured`. No other check's `NotFound` changes.
6. Scoring profile `v5` (`crates/engine/profiles/v5.toml`, new lock entry) carries `v4`'s section
   weights, check weights, hard fails and thresholds; `Profiles::current()` is `v5`; `v3` and `v4`
   stay loadable with unchanged lock entries.
7. The catalogue texts of `dns.https_svcb` describe every branch as a warning: `finding` names a
   missing record and the two malformed cases; `fix_hint` tells how to publish a record. Title and
   short label stay, so `checkMeta.ts` keeps its names and titles.
8. History fixtures, lens goldens and design fixtures are regenerated from evidence and hold
   `profile_version` 5 and the recomputed `dns.https_svcb` statuses and grades.
9. SDD `specs/sdd/v2.md` records the three decisions in one decision row (S44), amends S42 and
   §3.7's probe text for requirement 1, §3.6's guard bullet for requirement 3, §3.9's
   `dns.https_svcb` row and a profile `v5` paragraph for requirements 5 and 6, each in the commit
   of its code; CLAUDE.md's in-process bullet names the exception; `CHANGELOG.md` `[Unreleased]`
   names the grade change.

## Phase 1 — TLS: legacy-version alerts refuse

**Depends on:** none
**Requirements:** 1, 2, 9 (S42 and §3.7)

`crates/tls/src/tls/probe.rs` (`decide`, `REFUSING_ALERTS`); tests in `crates/tls/tests/it/`
over `classify`.

### Test Scenarios

GIVEN an SSLv3 probe answered with the fatal alert unrecognized_name (112) WHEN classified THEN it is Refused with the answer "alert unrecognized_name".
GIVEN a TLS 1.0, TLS 1.1 or TLS 1.2 probe answered with a fatal alert 10, 40, 47, 50, 70, 71, 86 or 112 WHEN classified THEN it is Refused.
GIVEN a TLS 1.0 probe answered with close_notify at warning or fatal level WHEN classified THEN it is Refused with the answer "alert close_notify".
GIVEN a TLS 1.0 probe answered with a fatal internal_error (80), user_canceled (90) or unregistered alert (200) WHEN classified THEN it is Unanswered.
GIVEN a TLS 1.0 probe whose read timed out with nothing received WHEN classified THEN it is Unanswered "timed out".
GIVEN a TLS 1.0 probe answered with a warning unrecognized_name followed by a ServerHello at TLS 1.0 WHEN classified THEN it is Accepted.
GIVEN a suite-group probe answered with a fatal unrecognized_name, decode_error or close_notify WHEN classified THEN it is Unanswered, and with a fatal handshake_failure THEN Refused.

## Phase 2 — DNS: domain runs pay no per-target limit

**Depends on:** none
**Requirements:** 3, 4, 9 (§3.6, CLAUDE.md)

`crates/netray/src/main.rs` (`lens_registry`), `crates/dns/src/module.rs` (doc comments); a unit
test in `crates/netray/src/main.rs` over `lens_registry`; the API limiter test in
`crates/web/tests/it/`.

### Test Scenarios

GIVEN a lens config whose `[modules.dns.limits]` sets `per_target_burst = 1` and `[modules.dns.dns]` allows the server 192.0.2.53 WHEN `lens_registry` builds the modules and the registry's DNS module runs example.com twice under a short deadline THEN neither outcome is incomplete with a rate-limit reason.
GIVEN that config WHEN the facts provider of the registry resolves example.com twice THEN neither error names a rate limit.
GIVEN an API state whose `[api.dns]` per-target burst is exhausted for a target WHEN a DNS query job for that target starts THEN it is refused with scope `api_v1_dns_target`, as before.

## Phase 3 — DNS: a missing HTTPS record warns, profile v5

**Depends on:** none
**Requirements:** 5, 6, 7, 8, 9 (S44, §3.9, CHANGELOG)

`crates/dns/src/module.rs` (`translate`), `crates/engine/profiles/v5.toml`, `lock.toml`,
`crates/engine/src/profile.rs`, `crates/engine/catalogue/checks.toml`; tests in
`crates/dns/tests/it/`, `crates/engine/tests/it/`; regenerated fixtures and goldens.

### Test Scenarios

GIVEN HTTPS and SVCB batches for example.com that answer NODATA and a lint event `https_svcb` with `NotFound` WHEN translated THEN `dns.https_svcb` is `warn` with the finding "Not found".
GIVEN a lint event `https_svcb` with a Warning for mixed modes WHEN translated THEN it is `warn` with that finding.
GIVEN a lint event `https_svcb` with Ok WHEN translated THEN it is `pass`.
GIVEN an HTTPS batch whose every lookup timed out WHEN translated THEN `dns.https_svcb` is `unmeasured`.
GIVEN a lint event `caa` with `NotFound` WHEN translated THEN `dns.caa` is still `fail`.
GIVEN `Profiles::current()` WHEN read THEN it is version 5 with v4's section weights, check weights, hard fails and thresholds; GIVEN `Profiles::get(4)` and `get(3)` THEN they load; GIVEN the lock THEN v3, v4 and v5 hashes match their files.
GIVEN a stored v4 run of example.com with `dns.https_svcb` fail and a current v5 run with it warn WHEN diffed THEN the diff carries a profile change and the grade change carries the v4 run re-scored under v5.
GIVEN the catalogue WHEN `dns.https_svcb` is read THEN its finding names a missing record, mixed modes and several alias records, and `tests/repo/test_checkmeta_catalogue.sh` passes.
GIVEN the committed history fixtures WHEN `every_history_fixture_is_its_recomputed_run` runs THEN it passes with `profile_version` 5.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate (operator,
  2026-10-19, extended 2026-10-22). The three outcomes are the operator's decisions of
  2026-10-22; the details below are the implementer's.
- Independent reading of the Draft skipped: the review step reads the whole range against the spec
  (implementer under the operator's autonomy mandate, 2026-10-22).
- Refusing alerts for legacy-version probes: an alert in place of a ServerHello means the server
  did not negotiate the offered version on an address and port where the main handshake, with the
  same SNI, just succeeded. The alerts that reject a ClientHello's content or version (10, 40, 47,
  50, 70, 71, 86, 112) and close_notify (a TLS-level close, as a FIN is under S42) refuse.
  internal_error and user_canceled are the server's own fault or choice and say nothing about the
  version, like SERVFAIL (Rule M); certificate and record-layer alerts cannot answer a ClientHello
  sensibly and an unregistered code is garbage. Suite-group probes keep the narrow rule: the
  decision covers version probes, and a suite offer that a server cannot parse is less clearly a
  refusal of the suites (implementer under the operator's autonomy mandate, 2026-10-22).
- Domain runs are admitted by lens: the registry's DNS module becomes `admitted_by_caller`, the
  instance stays separate from the band's (own circuit breakers and query semaphore, as today).
  `[modules.dns.limits] per_target_*` stay accepted though lens no longer reads them, because
  production's config carries them and `deny_unknown_fields` would refuse it; their removal belongs
  to the cutover's config merge (S10) (implementer under the operator's autonomy mandate,
  2026-10-22).
- `dns.https_svcb` never fails after this change: mhost reports both malformed cases as warnings,
  so absence joins them. The mapping lives in `translate` for this check only, not in mhost and not
  in `lint_status` (the V1 wire keeps `not_found`) (implementer under the operator's autonomy
  mandate, 2026-10-22).
- Profile `v5` repeats `v4`'s numbers: the operator asked for a new profile to mark the grade
  change (SDD §3.8: a change is a new version the diff shows as a profile change). As with S41,
  the grade split cannot carry the effect: a stored `v4` run keeps its stored `fail` and re-scores
  under `v5` to its old grade (implementer under the operator's autonomy mandate, 2026-10-22).

## Open decisions

None.

## Out of scope

- Suite-group probe alerts; the standalone `netray dns` limiter; removing the dead
  `[modules.dns.limits] per_target_*` keys (cutover config merge).
