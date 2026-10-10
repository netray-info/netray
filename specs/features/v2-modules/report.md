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
| C1 | R2: RunContext domain/options, Measured{checks,presentation}, Registry, run_with, with_registry | open | crates/engine/tests/traits.rs, crates/engine/tests/registry.rs |
| C2 | R3: crates/http is netray-http, NETRAY_HTTP_ / NETRAY_HTTP_CONFIG, SPECTRA_ refused naming the new prefix; paths, checks, release smoke, docs follow | open | crates/http/tests/env_prefix.rs, tests/repo/test_env_prefixes.sh |
| C3 | R4: ModuleConfig, Module with http.<v1> IDs, in-process run on Facts, pure translate moved from lens, golden_module behind testing | open | crates/http/tests/module.rs |
| C4 | R5: lens takes HTTP from the registry; backends/http.rs and [backends.http] url gone (url refused), timeout_ms the deadline; lens tests on golden_module; goldens unchanged | open | crates/lens/tests/*.rs |
| C5 | stub Module reads domain and DKIM selectors from RunContext; Measured{checks,presentation} reaches lens | open | crates/engine/tests/registry.rs |
| C6 | no tracked path names crates/spectra | open | tests/repo/test_env_prefixes.sh |
| C7 | NETRAY_HTTP_SERVER__BIND binds; SPECTRA__SERVER__BIND refused naming NETRAY_HTTP_ | open | tests/repo/test_env_prefixes.sh |
| C8 | spectra-inspect.json translated equals the HTTP section of lens-full-healthy.json | open | crates/http/tests/module.rs |
| C9 | [modules.http] unknown key, or [backends.http] url, makes `netray lens --check-config` exit 1 naming it | open | tests/repo/test_check_config.sh |
| C10 | lens with golden modules: full-output and lens_golden unchanged | open | crates/lens/tests/lens_full_output.rs, lens_golden.rs |
