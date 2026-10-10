# Report: v2 modules

## Phase 0 — Baseline

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: lens's full sync output (durations and IDs removed) committed per lens fixture and for beacon-timeout, beacon-partial, beacon-sending-no-dkim as `lens-full-*.json`, written by `UPDATE_GOLDEN=1`; a test compares | green | crates/lens/tests/lens_full_output.rs |
| C2 | each fixture plus the three beacon-only goldens: `UPDATE_GOLDEN=1` writes, without it the test compares and passes | green | crates/lens/tests/lens_full_output.rs |
| C3 | a one-word headline change makes the test fail naming the fixture and field | green | crates/lens/tests/lens_full_output.rs |

RED: `lens_full_output_matches_goldens` failed on the ten missing goldens; the comparator and stripper unit tests passed (they are the C3 tooling).

### Behavioural verification

`UPDATE_GOLDEN=1 cargo test -p lens --test lens_full_output` wrote ten `lens-full-*.json` (79 kB) from the 0.23.1 code; two further runs without it passed; no stub port or timestamp remains (`grep -E '127\.0\.0\.1:[0-9]+|20[0-9]{2}-..-..T'` empty). Sections carry checks with fix hints, headlines, detail URLs and the HTTP and IP extras.

## Phase 1 — HTTP module

### API contract (fixed before the test writers)

- `netray_engine`: `RunContext { deadline: Instant, domain: Domain, options: RunOptions }`; `#[derive(Debug, Clone, Default, PartialEq)] RunOptions { dkim_selectors: Option<Vec<String>> }`; `SectionOutcome::Measured { checks: Vec<CheckResult>, presentation: serde_json::Value }`; `Registry::new()`, `.with(Box<dyn Module>) -> Registry`, `.with_facts(Box<dyn FactsProvider>) -> Registry`, `.module(Protocol) -> Option<&dyn Module>`, `.facts() -> Option<&dyn FactsProvider>`.
- `netray_http`: `ModuleConfig` (`deny_unknown_fields`; sections `inspect`, `enrichment` as in spectra's config), `HttpModule::new(ModuleConfig) -> Result<HttpModule, _>` implementing `Module`; `translate(&InspectResponse) -> SectionOutcome` (pure, moved from `crates/lens/src/backends/http.rs`); feature `testing`: `testing::golden_module(contract_json: &str) -> Box<dyn Module>`.
- lens: `AppState::with_registry(Config, Registry)`, `lens::run_with(Option<String>, Registry)`; `Config.modules: BTreeMap<String, toml::Table>`; `[backends.http] url` refused.
- `crates/netray`: builds `HttpModule` from `[modules.http]` and calls `lens::run_with`; `netray lens --check-config` builds every module from its table.

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R2: RunContext domain/options, Measured{checks,presentation}, Registry, run_with, with_registry | green | crates/engine/tests/traits.rs, crates/engine/tests/registry.rs |
| C2 | R3: crates/http is netray-http, NETRAY_HTTP_ / NETRAY_HTTP_CONFIG, SPECTRA_ refused naming the new prefix; paths, checks, release smoke, docs follow | green | tests/repo/test_env_prefixes.sh, crates/http/src/config.rs (unit) |
| C3 | R4: ModuleConfig, Module with http.<v1> IDs, in-process run on Facts, pure translate moved from lens, golden_module behind testing | green | crates/http/tests/module.rs |
| C4 | R5: lens takes HTTP from the registry; backends/http.rs and [backends.http] url gone (url refused), timeout_ms the deadline; lens tests on golden_module; goldens unchanged | green | crates/lens/tests/*.rs |
| C5 | stub Module reads domain and DKIM selectors from RunContext; Measured{checks,presentation} reaches lens | green | crates/engine/tests/registry.rs |
| C6 | no tracked path names crates/spectra | green | tests/repo/test_env_prefixes.sh |
| C7 | NETRAY_HTTP_SERVER__BIND binds; SPECTRA__SERVER__BIND refused naming NETRAY_HTTP_ | green | tests/repo/test_env_prefixes.sh |
| C8 | spectra-inspect.json translated equals the HTTP section of lens-full-healthy.json | green | crates/http/tests/module.rs |
| C9 | [modules.http] unknown key, or [backends.http] url, makes `netray lens --check-config` exit 1 naming it | green | tests/repo/test_check_config.sh |
| C10 | lens with golden modules: full-output and lens_golden unchanged | green | crates/lens/tests/lens_full_output.rs, lens_golden.rs |

RED (08dc0c4): every new and rewired test failed to compile against the missing API; `test_env_prefixes.sh` failed on the `SPECTRA__` loader.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| rename | 1 (general-purpose, mechanical) | sonnet | 73128 | 263 |
| G1 engine | 1 | sonnet | 25626 | 27 |
| G2 netray-http | 1 | sonnet | 99250 | 196 |
| G3 lens + binary | 1 (+ orchestrator: `modules` in seven test Config literals) | sonnet | 100459 | 215 |
| reader amendments | 1 | sonnet | 44685 | 129 |

### Reader

| class | at | finding | outcome |
|---|---|---|---|
| AMENDMENT | crates/netray/src/main.rs:101 | without `[modules.http.enrichment] ip_url` the HTTP section loses server org and network type, silently | repaired: lens dev, example and production fixture carry `ip_url`; startup warns when it is missing; argus renders it (K item) |
| AMENDMENT | crates/http/src/module.rs:69 | in-process the HTTP section skipped spectra's per-target limit (30/min, burst 10); lens's cache bypass with `dkim_selectors` made one target reachable at lens's per-IP rate per client | repaired: the module keeps the per-target limiter (`[modules.http.limits]`), checked before target validation; `module_target_limit.rs` |
| AMENDMENT | crates/lens/src/modules.rs:103 | an HTTP failure or timeout left no log line | repaired: WARN "backend call failed" with `service = "http"` and the reason |
| AMENDMENT | spec R4 | with `Facts` empty until the engine run, the module resolves as spectra does (`validate_target`); with `Facts` it picks from them | in this phase; Phase 6 supplies `Facts` |
| NIT | crates/lens/src/state.rs:123 | `[backends.http]` without `url` now enables the section | documented in lens.example.toml and README; argus renders the table |
| DEFERRED | crates/lens/tests/unknown_verdicts.rs | `lens_unknown_verdict_total{section="http"}` can no longer increment: in-process the status is typed | by design |

### Behavioural verification

```
$ netray lens --check-config crates/lens/lens.dev.toml
config ok: crates/lens/lens.dev.toml
$ netray lens --check-config <dev + [modules.http] bogus = 1>
config error: …: modules.http: unknown field `bogus`, expected one of `inspect`, `enrichment`, `limits`   (exit 1)
$ SPECTRA__SERVER__BIND=127.0.0.1:1 netray http crates/http/spectra.dev.toml
SPECTRA__SERVER__BIND is set: these variables are now NETRAY_HTTP_* (config file: NETRAY_HTTP_CONFIG)
```
`just adlc-verify` green; `tests/fixtures/contracts/` unchanged.

## Phase 2 — Email module

### API contract (fixed before the test writers)

- `netray_engine::SectionOutcome` gains `TimedOut` (a module whose own deadline fired; lens renders it as its V1 timeout). Phase 6's engine uses it for overruns too.
- `netray_email`: `ModuleConfig` (`deny_unknown_fields`; sections `dns`, `dnsbl`, `http`, `dkim`, `backends` (the enrichment `ip_url`) and `inspections` (`max_concurrent`) with beacon's key names and defaults); `EmailModule::new(ModuleConfig) -> Result<EmailModule, _>` is `async` (beacon's resolvers); `impl Module` with ids `email.<v1 name>`; `translate(&[SseEvent]) -> SectionOutcome` (pure, moved from `crates/lens/src/backends/email.rs`: a `skipped` summary → `TimedOut`, otherwise `Measured { checks, presentation }`); feature `testing`: `testing::golden_module(contract_sse: &str) -> Box<dyn Module>` (parses the `data:` lines of a `beacon-*.sse` golden).
- The module passes `ctx.options.dkim_selectors` to `run_all_checks`.
- lens: the adapter becomes generic over the protocol (`ModuleSection` for http and email, `BackendExtra::Email` from presentation); `[backends.email] url` refused; `TimedOut` → lens's timeout.
- `crates/netray`: `lens_registry` builds `EmailModule` from `[modules.email]` too; unknown tables still refused.

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R6: crates/email is netray-email, NETRAY_EMAIL_ / NETRAY_EMAIL_CONFIG, BEACON_ refused naming the new prefix; paths, checks, docs follow | green | tests/repo/test_env_prefixes.sh |
| C2 | R7: ModuleConfig, Module (email.<v1>) running run_all_checks in-process with the request's selectors, translate moved from lens, golden_module, inspection semaphore kept | green | crates/email/tests/module.rs |
| C3 | R8: lens takes email from the registry; backends/email.rs and [backends.email] url gone; full-output goldens unchanged incl. the three beacon-only | green | crates/lens/tests/*.rs |
| C4 | BEACON__SERVER__BIND or BEACON_CONFIG set → netray email refused naming NETRAY_EMAIL_ | green | tests/repo/test_env_prefixes.sh |
| C5 | each beacon-*.sse translated equals the email section of its full-output golden (Null MX, no MX, timeout, partial, cross-validation) | green | crates/email/tests/module.rs |
| C6 | DKIM selectors on a lens request reach the email module | green | crates/lens/tests/email_in_process.rs |
| C7 | full-output and lens_golden unchanged | green | crates/lens/tests/lens_full_output.rs, lens_golden.rs |

RED (ea02272): the new and rewired tests failed against the missing module API; `test_env_prefixes.sh` failed on the `BEACON__` loader.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| rename | 1 (general-purpose, mechanical) | sonnet | 67056 | 321 |
| G1 engine `TimedOut` + netray-email | 3 (needed one lens match arm; a lost space in a test row, fixed by the orchestrator) | sonnet | 114911 | 327 |
| G2 lens + binary | 2 (one test read a non-existent `sections` key; fixed by the orchestrator) | sonnet | 99582 | 272 |
| reader repairs | 1 | sonnet | 41478 | 45 |

### Reader

| class | at | finding | outcome |
|---|---|---|---|
| BLOCKER | crates/email/src/module.rs:125 | the module passed lens's up to 10 DKIM selectors without beacon's `max_user_selectors` cap (route-only): a key at the 6th selector was never queried (release) or the check panicked (debug) | repaired: the module validates selectors as the route does (shared `input::validate_selectors`), refusal → `Incomplete` with beacon's text; `module_input.rs` |
| AMENDMENT | crates/email/src/module.rs:124 | `parse_domain` was route-only: `localhost`, `foo_bar.example.com` were inspected and scored | repaired in phase: the module parses the domain first |
| NIT | crates/email/src/module.rs:87 | `inspections.max_concurrent` 0 made every run "busy"; above `MAX_PERMITS` panicked | repaired: refused at parse time |
| NIT | crates/email/src/lib.rs:74 | a missing default `beacon.toml` now runs on built-in defaults (the image ships one) | accepted: matches `netray http`; `test_env_prefixes.sh` needs it |
| DEFERRED | — | `lens_unknown_verdict_total{section="email"}` cannot increment in-process (typed events) | by design, as for HTTP |

### Behavioural verification

`just adlc-verify` green; `tests/fixtures/contracts/` unchanged; `test_env_prefixes.sh` refuses `BEACON__SERVER__BIND` and `BEACON_CONFIG` naming `NETRAY_EMAIL_`; `email_in_process.rs` sees the request's DKIM selectors in the module.
