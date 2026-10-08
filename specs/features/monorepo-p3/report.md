# Report: contract deliverables

## Phase 1 — Config loader and check

### Criteria

| Id | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R1: `netray_common::config::load::<T>(path, prefix)` builds from optional TOML + env under D1 naming; the only loader; non-UTF-8 env does not panic | green | `crates/common/tests/config_loader.rs` |
| C2 | R2: every config struct (incl. common's shared ones) rejects unknown keys; a convention test enforces `deny_unknown_fields` | green | `tests/repo/test_config_strict.sh` |
| C3 | R3: `netray <sub> --check-config <path>` for lens dns tls http email ip: 0 + `config ok: <path>`; 1 for unknown key, missing file, rejected value; no listener | green | `tests/repo/test_check_config.sh` |
| C4 | R4: `tests/fixtures/<service>.production.toml` for all six, mirroring the argus templates; a loading test per service | green | `tests/repo/test_check_config.sh` |
| C5 | R5: `IpExtractor` ignores `CF-Connecting-IP` | green | `crates/common/tests/ip_extract_no_cf.rs; crates/ifconfig-rs/src/extractors.rs` |
| C6 | Scenario: unknown key in file → error naming the key | green | `crates/common/tests/config_loader.rs` |
| C7 | Scenario: `PRISM_CONFIG` + `PRISM_LIMITS__…` → value set, `config` not unknown; same for `SPECTRA__…`, `BEACON__…` | green | `crates/common/tests/config_loader.rs` |
| C8 | Scenario: non-UTF-8 env entry → no panic | green | `crates/common/tests/config_loader.rs` |
| C9 | Scenario: convention test over every `config.rs` | green | `tests/repo/test_config_strict.sh` |
| C10 | Scenario: `--check-config` fixture → 0; fixture + unknown key → 1; missing path → 1 | green | `tests/repo/test_check_config.sh` |
| C11 | Scenario: each service's fixture loads through its loader | green | `tests/repo/test_check_config.sh` |
| C12 | Scenario: trusted proxy, `CF-Connecting-IP` + `X-Real-IP` → `X-Real-IP` wins | green | `crates/common/tests/ip_extract_no_cf.rs; crates/ifconfig-rs/src/extractors.rs` |

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 34,985 | 112 |
| G2 | 2 | sonnet | 72,458 | 218 |
| G3 | 1 | sonnet | 33,834 | 36 |
| Review repairs (prefix case, ifconfig-rs CF) | 1 | sonnet | 27,894 | 44 |

### Review

| Class | Finding | Outcome |
|---|---|---|
| BLOCKER | `ip --check` was specified as an alias of `--check-config` (D2) but keeps its config-and-data meaning and panics on a load error | spec D2 amended: `--check` is not an alias; `--check-config` is the operator check |
| AMENDMENT | ifconfig-rs's own `extract_client_ip` still trusted `CF-Connecting-IP` (rate-limit bypass) | repaired in phase: D6/R5 widened, test `trusted_proxy_cf_connecting_ip_is_ignored` |
| AMENDMENT | Prefix matched case-sensitively; the `config` crate matched case-insensitively before | repaired in phase: D1 amended, test `prefix_matches_case_insensitively` |
| AMENDMENT | A non-UTF-8 override is skipped silently (before: a load error in prism/tlsight/ifconfig-rs) | decided in D1: skipped |
| AMENDMENT | `specs/rules/architecture-rules.md` gives `PRISM_TELEMETRY__LEVEL` as its example (now an unknown key) and `SPECTRA_`/`BEACON_` with one underscore | prose, fixed in the spec's closing docs commit |
| AMENDMENT | `--check-config` for http/email only deserialises: spectra and beacon have no `validate()`, so a zero rate limit passes the check and fails at startup (tlsight likewise for a missing `custom_ca_dir`) | not repaired: forwarded to the operator |
| NIT | Changed unit tests need `ADLC-Test-Change` trailers | carried in this commit |
| — | `test_config_strict.sh` passed on an empty scan | repaired: the scan must report the files it read |

Unit tests changed in production files: `ip_extract.rs` `trusted_peer_uses_cf_connecting_ip` → `trusted_peer_ignores_cf_connecting_ip` (expects the peer), `cf_connecting_ip_with_whitespace` removed, `cf_connecting_ip_takes_priority_over_x_real_ip` → `x_real_ip_wins_over_cf_connecting_ip`, `cf_connecting_ip_invalid_falls_through` → `cf_connecting_ip_is_ignored_in_favour_of_x_real_ip`; lens `non_config_env_vars_are_not_config_keys`; spectra `env_overrides_apply`, `unknown_env_key_is_rejected` (env pairs, same assertions).

Fixtures: the four new ones mirror `argus-oci/ansible/roles/app_stack/templates/*.toml.j2`; beacon's fixture was aligned to its template on the operator's decision (`per_ip` 10/min, no `sample_rate`). No template key is unknown to the code.

### Behavioural verification

```
$ netray dns --check-config crates/mhost-prism/tests/fixtures/prism.production.toml
config ok: crates/mhost-prism/tests/fixtures/prism.production.toml        (exit 0)
$ netray email --check-config crates/beacon/tests/fixtures/beacon.production.toml
config ok: crates/beacon/tests/fixtures/beacon.production.toml            (exit 0)
$ netray tls --check-config bad.toml    # tlsight fixture with `bogus = 1` on top
config error: …/bad.toml: unknown field `bogus`, expected one of `site_name`, `server`, … (exit 1)
$ netray ip --check-config /nonexistent.toml
config error: /nonexistent.toml: configuration file "/nonexistent.toml" not found (exit 1)
```

`adlc verify`: passed, 24 tests collected.
