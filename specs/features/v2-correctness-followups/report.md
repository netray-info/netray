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
