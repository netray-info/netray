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
| Startup rejects (`validate()` for spectra, beacon, tlsight) | 1 | sonnet | 47,384 | 49 |

### Review

| Class | Finding | Outcome |
|---|---|---|
| BLOCKER | `ip --check` was specified as an alias of `--check-config` (D2) but keeps its config-and-data meaning and panics on a load error | spec D2 amended: `--check` is not an alias; `--check-config` is the operator check |
| AMENDMENT | ifconfig-rs's own `extract_client_ip` still trusted `CF-Connecting-IP` (rate-limit bypass) | repaired in phase: D6/R5 widened, test `trusted_proxy_cf_connecting_ip_is_ignored` |
| AMENDMENT | Prefix matched case-sensitively; the `config` crate matched case-insensitively before | repaired in phase: D1 amended, test `prefix_matches_case_insensitively` |
| AMENDMENT | A non-UTF-8 override is skipped silently (before: a load error in prism/tlsight/ifconfig-rs) | decided in D1: skipped |
| AMENDMENT | `specs/rules/architecture-rules.md` gives `PRISM_TELEMETRY__LEVEL` as its example (now an unknown key) and `SPECTRA_`/`BEACON_` with one underscore | prose, fixed in the spec's closing docs commit |
| AMENDMENT | `--check-config` for http/email only deserialises: spectra and beacon have no `validate()`, so a zero rate limit passes the check and fails at startup (tlsight likewise for a missing `custom_ca_dir`) | forwarded to the operator, who chose to repair it now: R3 amended, `validate()` added to spectra and beacon, tlsight checks `custom_ca_dir`; five `startup_rejects` cases in `test_check_config.sh` |
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

## Phase 2 — Header and CORS parity

### Criteria

| Id | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R6: every response of the six services carries `secure-headers` (D3), no `Server`/`X-Powered-By`; non-`/docs` responses carry `csp-tool-spa` verbatim; ACAO `*` and CORP `cross-origin` everywhere; POST preflight 2xx with D3 methods, headers, max-age 600 | green | `tests/repo/test_header_parity.sh` |
| C2 | R7: ifconfig-rs's HSTS difference is a `SecurityHeadersConfig` field; no header-rewriting code in ifconfig-rs | green | `tests/repo/test_header_parity.sh` |
| C3 | R8: acceptance asserts R6 per host and `netray site`'s set, against production and a local `netray`; mta-sts policy check replaces the TODO | green | `tests/acceptance/smoke/security-headers.spec.ts; tests/acceptance/static-site/assets.spec.ts` |
| C4 | Scenario: `/` and a JSON route per service carry the D3 set, `csp-tool-spa`, ACAO, CORP, no `server` | green | `tests/repo/test_header_parity.sh` |
| C5 | Scenario: `/docs` has the relaxed CSP allowing Scalar, other D3 headers present | green | `tests/repo/test_header_parity.sh` |
| C6 | Scenario: POST preflight → 2xx, methods `GET, POST, OPTIONS`, headers `content-type, accept`, max-age 600 | green | `tests/repo/test_header_parity.sh` |
| C7 | Scenario: ifconfig-rs HSTS from `SecurityHeadersConfig`, no own header code | green | `tests/repo/test_header_parity.sh` |
| C8 | Scenario: acceptance header spec passes against production (operator-observed) | green | `tests/acceptance/smoke/security-headers.spec.ts` |
| C9 | Scenario: acceptance mta-sts spec: `version: STSv1`, `mode: enforce` | green | `tests/acceptance/static-site/assets.spec.ts` |

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| G1 headers and CORS | 1 | sonnet | 65,514 | 182 |
| G2 `acceptance-local` (fixed ports, collided with foreign dev servers on 8000/8080) | 3 | stuck → replaced | 42,915 | 89 |
| G2 `acceptance-local` on free ports via `LOCAL_<NAME>_URL` | 2 | sonnet | 34,762 | 62 |

### Review

| Class | Finding | Outcome |
|---|---|---|
| BLOCKER | beacon's relaxed `/docs` CSP source `…/api-reference@1.44.25/` ends in `/` and does not match the page's script `…/api-reference@1.44.25`; Scalar would render blank after cutover (verified in headless Chromium by the reader) | test sharpened (`16dee7f`: scripts must match `script-src` by CSP source rules), source fixed without the slash |
| NIT | the `/docs` check only grepped for the host | repaired with the BLOCKER |
| AMENDMENT | C7/R7 named a `font-src` field and "no header-rewriting code", but D3 drops `font-src` and ifconfig-rs keeps `Vary`/`Cache-Control` | repaired in phase (`8f13662`) |

Unit tests changed in production files: common `sets_all_base_headers` (exact CSP, preload HSTS, COOP, CORP), `no_permissions_policy_by_default` → `permissions_policy_always_set`, `includes_permissions_policy_when_configured` → `hsts_taken_from_config`; prism and tlsight `sets_strict_transport_security` (preload).

Not run locally: ifconfig-rs integration tests (`ok_handlers.rs`), which need GeoIP data; their header asserts (2-year HSTS, CDN in `/docs` CSP, preflight 2xx) hold by reading. CI runs them.

### Behavioural verification

```
$ just acceptance-local            # all seven subcommands on free ports, no proxy
  29 passed (3.3s)
$ cd tests/acceptance && TEST_ENV=production npx playwright test --no-deps smoke/security-headers.spec.ts static-site/assets.spec.ts
  29 passed (1.5s)
$ bash tests/repo/test_header_parity.sh
PASS: test_header_parity
```

`adlc verify`: passed.

## Phase 3 — lens snapshot 404

### Criteria

| Id | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R9: `/r/<id>` answers 404 with the "expired or unknown" HTML page for an unknown, an expired and a malformed id | green | `crates/lens/tests/snapshot_routes.rs` |
| C2 | Scenario: unknown id `abcdefgh` → 404, HTML says expired or unknown | green | `crates/lens/tests/snapshot_routes.rs` |
| C3 | Scenario: snapshot past its expiry → 404, same page | green | `crates/lens/tests/snapshot_routes.rs` |
| C4 | Scenario: malformed id (`/r/x`, `/r/not-a-valid-id!`) → 404, same page, not 400 JSON | green | `crates/lens/tests/snapshot_routes.rs` |

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| G1 malformed id → 404 page | 1 | sonnet | 25,979 | 48 |
| G1 repair: path rejection and unmatched `/r/` paths | 2 | sonnet | 29,765 | 45 |

### Review

| Class | Finding | Outcome |
|---|---|---|
| BLOCKER | an id that percent-decodes to invalid UTF-8 (`/r/%C0`) was rejected by `Path<String>` with 400 text/plain before the handler | repaired: the handler takes `Result<Path<String>, PathRejection>`; test rows added (`ce09f5e`) |
| DEFERRED → repaired | `/r/`, `/r/a/b`, `/r/AAAAAAAA/` fell through to the SPA with 200 | repaired: `/r/`, `/r/{shortid}/`, `/r/{shortid}/{*rest}` answer the 404 page (axum 0.8 rejects `/r/{*rest}` beside `/r/{shortid}`) |

Unchanged and checked by the reader: no caller used the 400 JSON `INVALID_SHORTID`; the 404 page carries the security headers and request id; HEAD works; the id is never reflected into the page.

### Behavioural verification

```
$ netray lens lens.dev.toml   # snapshots enabled, empty store
/r/abcdefgh    -> 404 text/html   "This snapshot has expired or the ID is unknown."
/r/x           -> 404 text/html
/r/%C0         -> 404 text/html
/r/a/b         -> 404 text/html
/r/AAAAAAAA/   -> 404 text/html
/r/            -> 404 text/html
/              -> 200 text/html
```

`adlc verify`: passed.
