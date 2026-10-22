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
