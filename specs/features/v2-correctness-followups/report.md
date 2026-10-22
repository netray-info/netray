# Report: v2-correctness-followups

## Phase 1 — TLS: legacy-version alerts refuse

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: a legacy-version probe (Ssl3, Tls10, Tls11, Tls12) answered with close_notify (warning or fatal) or a fatal 10, 40, 47, 50, 70, 71, 86 or 112 is Refused with answer alert name; fatal 80, 90, other or unregistered alerts, a timeout, garbage and a partial answer stay Unanswered; other warning alerts are skipped | green | crates/tls/tests/it/probe_version_alerts.rs |
| C2 | R2: a suite-group probe refuses only on a fatal 40, 70 or 71; close_notify and other alerts leave it unanswered | already_implemented | crates/tls/tests/it/probe_suite_alerts.rs |
| C3 | GIVEN an SSLv3 probe answered with fatal unrecognized_name WHEN classified THEN Refused with answer alert unrecognized_name | green | crates/tls/tests/it/probe_version_alerts.rs |
| C4 | GIVEN a TLS 1.0, 1.1 or 1.2 probe answered with a fatal alert 10, 40, 47, 50, 70, 71, 86 or 112 WHEN classified THEN Refused | green | crates/tls/tests/it/probe_version_alerts.rs |
| C5 | GIVEN a TLS 1.0 probe answered with close_notify at warning or fatal level WHEN classified THEN Refused with answer alert close_notify | green | crates/tls/tests/it/probe_version_alerts.rs |
| C6 | GIVEN a TLS 1.0 probe answered with fatal internal_error, user_canceled or an unregistered alert (200) WHEN classified THEN Unanswered | already_implemented | crates/tls/tests/it/probe_version_alerts.rs |
| C7 | GIVEN a TLS 1.0 probe whose read timed out with nothing received WHEN classified THEN Unanswered timed out | already_implemented | crates/tls/tests/it/probe_version_alerts.rs |
| C8 | GIVEN a TLS 1.0 probe answered with a warning unrecognized_name then a ServerHello at TLS 1.0 WHEN classified THEN Accepted | already_implemented | crates/tls/tests/it/probe_version_alerts.rs |
| C9 | GIVEN a suite-group probe answered with fatal unrecognized_name, decode_error or close_notify WHEN classified THEN Unanswered, and with fatal handshake_failure THEN Refused | already_implemented | crates/tls/tests/it/probe_suite_alerts.rs |

C2, C6, C7, C8 and C9 held before the change; their tests are kept as protection (suite groups and
the unanswered cases must not move with the version probes).

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 0 | implementer | 0 | 0 |

G1 was a ten-line change in one function, written by the implementer without a coder.

### Review

- NIT (fixed): close_notify refused at any level byte; now only at warning or fatal level.
- NIT (fixed): S44 justified unrecognized_name with "the same SNI", but the SSLv3 probe carries no
  extensions; S44 now states the residual (a server refusing every SNI-less hello reads as
  refusing SSLv3).
- NIT (fixed): S42 still listed a warning alert as deciding nothing; it now excepts a version
  probe's close_notify.

### Behavioural verification

skipped: the probes need a live TLS server; the live check of nginx.org runs once after phase 3
with the build copied to main's binary path (network), and is recorded there.

## Phase 2 — DNS: domain runs pay no per-target limit

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R3: a domain run charges no per-target limit of the DNS module; the registry's DNS module and the facts stage are admitted by lens, so a per_target_burst below 19 never makes the DNS section incomplete with a rate-limit reason | green | crates/netray/src/main.rs |
| C2 | R4: a direct DNS-inspector request keeps its per-target limit from [api.dns] and is refused past its burst | already_implemented | crates/web/tests/it/api_dns_limits.rs |
| C3 | GIVEN a lens config with [modules.dns.limits] per_target_burst = 1 WHEN lens_registry builds the modules and the registry's DNS module runs example.com twice THEN neither outcome is incomplete with a rate-limit reason | green | crates/netray/src/main.rs |
| C4 | GIVEN that config WHEN the registry's facts provider resolves example.com twice THEN neither error names a rate limit | green | crates/netray/src/main.rs |
| C5 | GIVEN an [api.dns] per-target burst spent for a target WHEN a DNS query job for it starts THEN it is refused with scope api_v1_dns_target as before | already_implemented | crates/web/tests/it/api_dns_limits.rs |

C2 and C5 held before the change; `a_spent_per_target_burst_refuses_another_caller_with_retry_after`
in `crates/web/tests/it/api_dns_limits.rs` already pins them and stays green.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 0 | implementer | 0 | 0 |

G1 was one builder call in `lens_registry` plus doc comments, written by the implementer.

### Review

- AMENDMENT (repaired in phase 3's prose commit): CLAUDE.md's in-process bullet still names
  per-target limits for every module; requirement 9 puts it in the final prose commit.
- AMENDMENT (fixed): SDD §3.6 still said the facts stage charges 6 on top of 19.
- AMENDMENT (fixed): `crates/lens/lens.example.toml` still said a check charges 25 per target; it
  now says the keys are accepted and not read.
- AMENDMENT (fixed): the spec's scenario named 192.0.2.53, which the server policy refuses before
  any charge (the test would pass either way); the scenario and the test use 9.9.9.9 as
  `module_admitted.rs` does. Offline the run times out; it never says "rate limit".
- DEFERRED (operator proposal): badge and OG runs skip the per-IP, global and budget limits, so the
  DNS per-target limiter was their only per-minute ceiling on resolver load; now the 8 domain-run
  slots and the per-domain throttle bound them. S44 and the `lens_registry` comment now say so.
- NIT: `module_admitted.rs`'s `the_registry_module_refuses_over_its_per_target_limit` now tests a
  bare `DnsModule::new`, which no production path builds unadmitted; kept (protected), name stale.

### Behavioural verification

`cargo test -p netray --bin netray`: `tests::a_domain_run_charges_no_dns_per_target_limit ... ok`
(red before: `Incomplete { reason: "rate limited (per_target)" }`). The live run follows phase 3.

## Phase 3 — DNS: a missing HTTPS record warns, profile v5

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R5: dns.https_svcb is warn with finding Not found when HTTPS and SVCB answer no record; mixed modes and several alias records stay warn, well-formed pass, a failed HTTPS or SVCB batch unmeasured; no other check's NotFound changes | green | crates/dns/tests/it/https_svcb_absent.rs |
| C2 | R6: profile v5 carries v4's section weights, check weights, hard fails and thresholds; Profiles::current() is v5; v3 and v4 stay loadable with unchanged lock entries | green | crates/engine/tests/it/profile_v5.rs |
| C3 | R7: the catalogue texts of dns.https_svcb describe every branch as a warning; title and short label stay, checkMeta keeps its names and titles | green | crates/engine/tests/it/profile_v5.rs |
| C4 | R8: history fixtures, lens goldens and design fixtures are regenerated from evidence and hold profile_version 5 and the recomputed statuses and grades | green | crates/lens/tests/it/history_fixtures_consistent.rs |
| C5 | GIVEN HTTPS and SVCB batches answering NODATA and a lint event https_svcb NotFound WHEN translated THEN dns.https_svcb is warn with finding Not found | green | crates/dns/tests/it/https_svcb_absent.rs |
| C6 | GIVEN a lint event https_svcb with a Warning for mixed modes WHEN translated THEN warn with that finding | already_implemented | crates/dns/tests/it/https_svcb_absent.rs |
| C7 | GIVEN a lint event https_svcb with Ok WHEN translated THEN pass | already_implemented | crates/dns/tests/it/https_svcb_absent.rs |
| C8 | GIVEN an HTTPS batch whose every lookup failed (SERVFAIL) WHEN translated THEN dns.https_svcb is unmeasured | already_implemented | crates/dns/tests/it/https_svcb_absent.rs |
| C9 | GIVEN a lint event caa with NotFound WHEN translated THEN dns.caa is still fail | already_implemented | crates/dns/tests/it/https_svcb_absent.rs |
| C10 | GIVEN Profiles::current() THEN version 5 with v4's numbers; get(4) and get(3) load; the lock matches v3, v4 and v5 | green | crates/engine/tests/it/profile_v5.rs |
| C11 | GIVEN a stored v4 run with dns.https_svcb fail and a v5 run with it warn WHEN diffed THEN the diff carries a profile change and the grade change carries the v4 run re-scored under v5 | green | crates/engine/tests/it/profile_v5.rs |
| C12 | GIVEN the catalogue WHEN dns.https_svcb is read THEN its finding names a missing record, mixed modes and several alias records, and test_checkmeta_catalogue passes | green | crates/engine/tests/it/profile_v5.rs |
| C13 | GIVEN the committed history fixtures WHEN every_history_fixture_is_its_recomputed_run runs THEN it passes with profile_version 5 | green | crates/lens/tests/it/history_fixtures_consistent.rs |

C6 to C9 held before the change and pin that only the absent case moves. C3 and C12's substring
test passed against the old texts too; the reworded texts keep it green. The history fixtures all
pass `dns.https_svcb`, so only their `profile_version` moved (4 to 5) and no fixture grade
changed; the lens full-output goldens changed only in the catalogue `fix_hint`.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 0 | implementer | 0 | 0 |
| G2 | 0 | implementer | 0 | 0 |
| G3 | 0 | implementer | 0 | 0 |

### Grade impact (live, 2026-10-22 rerun evidence, rescored offline)

| domain | dns.https_svcb | DNS before | DNS after | overall before | overall after |
|---|---|---|---|---|---|
| amazon.com | fail to warn | 76.67 | 80.00 | B 84.84 | B 85.58 |
| docs.github.com | fail to warn | 81.67 | 85.00 | C 74.78 | B 75.52 |
| github.com | fail to warn | 85.00 | 88.33 | B 89.00 | B 89.74 |
| lwn.net | fail to warn | 76.67 | 80.00 | B 89.62 | A 90.36 |
| nginx.org | fail to warn | 76.67 | 80.00 | incomplete (49.37) | incomplete (50.58), complete once phase 1's SSLv3 refusal lands |
| ietf.org, www.ietf.org, netray.info, posteo.de, www.isc.org | pass | unchanged | | unchanged | |

The rescoring mirrors `scoring.rs` over the stored statuses of
`evidence/correctness/1022/fixed/` (private) and reproduces every stored grade before the change.

### Behavioural verification

Live, all three phases: the worktree build copied to main's binary path (backed up, `rm`, `cp`,
restored byte-identically, sha256 `f3a48a45…` before and after), `netray lens` on 127.0.0.1:18185
with `crates/lens/lens.dev.toml`'s values and `[modules.dns.limits] per_target_burst = 1`, then
`POST /api/v1/domains/<d>/runs` one after another:

```
GET /api/v1/meta -> "current_profile":5,"profiles":[3,4,5]
nginx.org        C complete  dns measured  dns.https_svcb warn ["Not found"]  tls.legacy_versions pass
  tls.probes@52.58.199.22:443 ssl3 disabled, ssl3_answer "alert unrecognized_name"
lwn.net          A complete  dns measured  dns.https_svcb warn ["Not found"]
docs.github.com  B complete  dns measured  dns.https_svcb warn ["Not found"]
```

Each DNS section stayed measured under a per-target burst of 1 (before: `rate limited
(per_target)` from the first run); nginx.org, incomplete on the 2026-10-22 rerun, is graded; the
letters match the offline rescoring above (lwn.net B to A, docs.github.com C to B).

### Review

- BLOCKER (repaired in the prose commit): `CHANGELOG.md` `[Unreleased]` did not name the v5 grade
  change or the TLS alert change; requirement 9's prose lands in the spec's final `docs` commit.
- AMENDMENT (repaired in the prose commit): CLAUDE.md's in-process bullet (phase 2).
- NIT (fixed): the SDD's overall gain now reads +0.67 or +0.57 with all five sections.
- NIT (fixed): the catalogue text test also requires "warning" in the finding, so it fails on the
  old text.
- NIT (fixed): the C8 scenario names SERVFAIL, as the test does (the batch-error arm is shared
  with timeouts, which `unmeasured_inputs.rs` covers).
- NIT (fixed): `lens.example.toml` and `crates/lens/CLAUDE.md` named profile v3.
- NIT (kept): the C11 diff test cannot tell a re-score under v4 from v5 (same numbers); and
  `contract_status.rs` pins a synthetic `Failed` lint for `dns.https_svcb` that mhost never emits.
