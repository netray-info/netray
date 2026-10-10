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
