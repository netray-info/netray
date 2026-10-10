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

## Phase 3 — IP module

### API contract (fixed before the test writers)

- `netray_ip`: `ModuleConfig` (`deny_unknown_fields`) with ifconfig-rs's data path keys (`geoip_city_db`, `geoip_asn_db`, `user_agent_regexes`, `tor_exit_nodes`, `feodo_botnet_ips`, `cins_army_ips`, `cloud_provider_ranges`, `vpn_ranges`, `datacenter_ranges`, `bot_ranges`, `spamhaus_drop`, `asn_patterns`, `asn_info`) — same names, same load semantics (city, ASN, user-agent regexes refuse; the others warn); `IpModule::new(ModuleConfig) -> Result<IpModule, _>` is `async` (loads `EnrichmentContext`); `IpModule::reload(&self)` reloads the data (`ArcSwap`, as ifconfig-rs's SIGHUP); `impl Module` with ids `ip.<v1 name>`; `run()` samples up to four IPv4 and four IPv6 public addresses from `Facts.a`/`aaaa` (sorted, `is_allowed_target`), looks each up in-process (`get_ifconfig`, no reverse DNS), translates.
- `translate(lookups: &[(IpAddr, Result<Ifconfig, String>)], total_public: usize) -> SectionOutcome` (pure, moved from `crates/lens/src/backends/ip.rs`: reputation check, geo, headline, sampling note; a failed lookup → `Incomplete`; no public address → `NotApplicable`).
- Feature `testing`: `testing::golden_module(contract_json: &str) -> Box<dyn Module>` (every sampled address answers the decoded golden `Ifconfig`).
- The per-target limiter of ifconfig-rs's route is not carried over: it protects the ifconfig service, and an in-process lookup reaches no target.
- lens: the adapter fills `Facts.a`/`aaaa` from the DNS section's resolved addresses (wave 2 as today); IP presentation → `BackendExtra::Ip`; `[backends.ip] url` refused.
- `crates/netray`: `lens_registry` builds `IpModule` from `[modules.ip]` and keeps a handle; on SIGHUP it calls `reload()`.

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R9: crates/ip is netray-ip (data/ moved), NETRAY_IP_ / NETRAY_IP_CONFIG, IFCONFIG_ refused; data image, no-data check, paths, docs follow | green | tests/repo/test_env_prefixes.sh, test_image_data.sh |
| C2 | R10: ModuleConfig with the data paths, same load semantics, --check-config loads them with a startup_rejects row; Module samples 4+4 sorted public addresses from Facts; translate moved; golden_module; SIGHUP reload | green | crates/ip/tests/module.rs, tests/repo/test_check_config.sh |
| C3 | R11: lens takes IP from the registry with Facts from the DNS backend; backends/ip.rs and [backends.ip] url gone; goldens unchanged | green | crates/lens/tests/*.rs |
| C4 | ifconfig-json.json translated equals the IP section of its full-output golden; 9 public + 2 private → 4 IPv4 + 4 IPv6 public sorted | green | crates/ip/tests/module.rs |
| C5 | [modules.ip] geoip_city_db missing → --check-config and startup refuse; feodo_botnet_ips missing → both start with a warning | green | tests/repo/test_check_config.sh |
| C6 | IFCONFIG_CONFIG set → netray ip refused naming NETRAY_IP_CONFIG | green | tests/repo/test_env_prefixes.sh |
| C7 | the release image carries no data file | green | tests/repo/test_image_data.sh |
| C8 | data file replaced + reload → the next lookup uses the new data | green | crates/ip/tests/module.rs |
| C9 | full-output and lens_golden unchanged | green | crates/lens/tests/lens_full_output.rs, lens_golden.rs |

RED (3f339d2): the new and rewired tests failed against the missing module API; the ip row of `test_env_prefixes.sh` on the `IFCONFIG_` loader.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| rename | 1 (general-purpose, mechanical; `/meta` project name pinned to `ifconfig-rs`) | sonnet | 83409 | 358 |
| G1 netray-ip | 3 (ifconfig-rs's integration tests need GeoIP data a checkout lacks; not in the gate, which runs `--lib`) | sonnet | 109045 | 242 |
| G2 lens + binary + SIGHUP | 1 (+ sampling total counts all addresses, as 0.23.1) | sonnet | 107035 | 293 |
| reader repairs | 4 | sonnet | 59651 | 261 |

### Reader

| class | at | finding | outcome |
|---|---|---|---|
| BLOCKER | crates/netray/src/main.rs:215 | the registry (and every data-load warning) was built before lens installed its tracing subscriber; `--check-config` had none | repaired: the binary installs the subscriber first (`lens::init_telemetry`), `--check-config` a stderr one; `test_check_config.sh` greps the warnings |
| BLOCKER | crates/netray/src/main.rs:149 | an absent or data-less `[modules.ip]` built an IP module with no data: reputation passed for Tor exits and DROP-listed addresses | repaired: no `[modules.ip]` → no IP section and a startup warning; a `[modules.ip]` without both GeoIP databases refuses; `test_check_config.sh` |
| AMENDMENT | crates/netray/src/main.rs:137 | Phase 1's missing-enrichment warning was lost the same way | repaired with the first blocker |
| NIT | crates/ip/src/module.rs:56 | lens loaded the user-agent regexes it never reads; a missing file refused startup | repaired: `user_agent_regexes` leaves `ModuleConfig` and the fixture |
| NIT | crates/netray/src/main.rs:216 | no test covers lens's SIGHUP wiring | accepted: verified by `kill -HUP` ("Enrichment data reloaded successfully"); `module.rs` covers `reload()` |

### Behavioural verification

`just adlc-verify` green; `tests/fixtures/contracts/` unchanged; reader ran `netray lens` and `kill -HUP` on it ("reload triggered", "Enrichment data reloaded successfully").

## Phase 4 — TLS module

### API contract (fixed before the test writers)

- `netray_tls`: the inspection core leaves `routes::do_inspect` as a pub async function (resolve, target policy, enrichment, CAA/TLSA/ECH, per-port inspection, validation, quality) that returns `InspectResponse`; the route keeps its per-client cap-and-warn and builds its HTTP response from it unchanged.
- `ModuleConfig` (`deny_unknown_fields`) with tlsight's check sections and key names: `limits` (handshake/request timeouts, `max_ports`, `max_concurrent_handshakes`, `max_ips_per_hostname`, `per_target_per_minute`, `per_target_burst`), `dns`, `validation`, `quality`, `backends` (the enrichment `ip`); no `allow_blocked_targets`.
- `TlsModule::new(ModuleConfig) -> Result<TlsModule, _>` is `async` (tlsight's resolver); `impl Module` with ids `tls.<v1 name>`; `run()` parses the domain with `input::parse_input` as the route does (a refusal → `Incomplete` with tlsight's text), checks the per-target limit with cost ports × addresses (keyed by host, scope `per_target`; over → `Incomplete` with the limiter's text; no per-client limit), keeps the handshake semaphore, then the core and `translate`.
- `translate(&InspectResponse) -> SectionOutcome` (pure, moved from `crates/lens/src/backends/tls.rs`: first port's checks, `tls_reachable`, `NOT_TESTED_FROM_HERE`, headline).
- Feature `testing`: `testing::golden_module(contract_json: &str) -> Box<dyn Module>`.
- lens: TLS presentation → `BackendExtra::Tls`; `[backends.tls] url` refused; `[modules.tls]` in the production fixture with tlsight.production.toml's check values.

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R12: crates/tls is netray-tls, NETRAY_TLS_ / NETRAY_TLS_CONFIG, TLSIGHT_ refused naming the new prefix | green | tests/repo/test_env_prefixes.sh |
| C2 | R13: core extracted and shared; tlsight's route responses unchanged; ModuleConfig; Module (tls.<v1>) with input parsing, per-target limit and handshake semaphore; translate moved; golden_module | green | crates/tls/tests/module.rs, crates/tls/tests/*.rs |
| C3 | R14: lens takes TLS from the registry; backends/tls.rs and [backends.tls] url gone; goldens unchanged | green | crates/lens/tests/*.rs |
| C4 | tlsight-inspect/not-tested/unreachable translated equal the TLS section of their full-output goldens | green | crates/tls/tests/module.rs |
| C5 | TLSIGHT_CONFIG set → netray tls refused naming NETRAY_TLS_CONFIG | green | tests/repo/test_env_prefixes.sh |
| C6 | a third run for one host over a per-target burst of 2 → Incomplete naming the limit; an invalid domain → Incomplete with tlsight's text | green | crates/tls/tests/module_limits.rs |
| C7 | full-output and lens_golden unchanged | green | crates/lens/tests/lens_full_output.rs, lens_golden.rs |

RED (a5c39db): the new and rewired tests failed against the missing module API; the tls row of `test_env_prefixes.sh` on the `TLSIGHT_` loader.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| rename | 1 (general-purpose, mechanical) | sonnet | 92123 | 454 |
| G1 netray-tls (core extracted from `do_inspect`) | 2 | opus (chosen up front: the extraction had to keep the route byte-identical) | 150645 | 496 |
| G2 lens + binary | 3 | sonnet | 92163 | 415 |
| reader repairs | 3 | opus | 54834 | 423 |

### Reader

| class | at | finding | outcome |
|---|---|---|---|
| BLOCKER | crates/tls/src/module.rs:89 | the module charged ports × every resolved address against per_target before the target policy and never reduced, so a host with more addresses than the burst (20) was refused on every run; 0.23.1 capped a visitor at 10 (tlsight's per_ip_burst with cap-and-warn) | repaired: policy first, then the route's reduction to `limits.check_budget` (default 10) with the same warning, shared as `routes::cap_to_budget`; the limiter is charged the reduced cost (one unit when everything is blocked); in-src unit tests |
| AMENDMENT | crates/netray/src/main.rs:151 | a missing `[modules.tls]` ran on tlsight's defaults silently (`check_ct = false`, production has it on) | repaired: startup warning "modules.tls is not configured: TLS runs on tlsight's defaults (certificate transparency off)" |
| NIT | crates/tls/src/routes.rs:50 | `/api-docs/openapi.json` showed `input_mode` as a `str` schema reference | repaired: `#[schema(value_type = String)]`; the document equals HEAD's (dumped and diffed) |
| NIT | crates/tls/src/module.rs:118 | an empty request id reaches enrichment; ifconfig mints its own | accepted: log correlation only; `RunContext` carries no request id in 1a |
| DEFERRED | crates/tls/src/module.rs:59 | the per-target limiter and the handshake semaphore exist once per process: lens and `netray tls` each have their own until the cutover (S1, P13) | for the SDD |
| DEFERRED | crates/tls/src/module.rs:90 | a visitor's handshake budget now comes from lens's check limit (10/min, burst 3) × `check_budget` | for the SDD (S18 admission) |

### Behavioural verification

`just adlc-verify` green; `tests/fixtures/contracts/` unchanged; tlsight's `/api/inspect` body, `x-cert-*` headers and `/api-docs/openapi.json` unchanged (reader and coder compared them against HEAD).

## Phase 5 — DNS module

### API contract (fixed before the test writers)

- `netray_dns`: the check pipeline leaves `api/check.rs` `post_handler` as a pub async function that emits prism's check events (`batch`, `lint`, `done`) on a channel; the SSE route forwards them unchanged.
- What the route applies to lens's calls stays in the module, except the per-client limit: domain validation, `parse_server_spec` + `QueryPolicy::validate_for_check` (so `@system` stays refused when `allow_system_resolvers = false`, SC17), the per-target part of `check_query_cost` (keyed by the effective servers), circuit breakers and the query semaphore. A refusal → `Incomplete` with prism's error text.
- `ModuleConfig` (`deny_unknown_fields`): prism's check sections with its key names — `dns` (`default_servers`, `allow_system_resolvers`, `allow_arbitrary_servers`), `limits` (the timeout, `max_servers`, per-target keys), `circuit_breaker`, `backends` (ip enrichment) — plus `servers` (the list lens sent as `[backends] dns_servers`).
- `DnsModule::new(ModuleConfig) -> Result<DnsModule, _>` (`async` if needed); `impl Module` with ids `dns.<v1 name>`; `run()` runs the pipeline with `servers`, collects the events, translates.
- `translate(&[CheckEvent]) -> SectionOutcome` (pure, moved from `crates/lens/src/backends/dns.rs`: lint categories → checks, the email categories dropped, DNSSEC absent → skip, headline; presentation carries `resolved_ips` from the `batch` A/AAAA records for the IP section).
- `DnsModule` implements `FactsProvider`: A, AAAA, MX, CAA, NS and the HTTPS RR through the module's resolver group; `facts_from_lookups(&Lookups) -> Facts` is pure and tested on a golden.
- Feature `testing`: `testing::golden_module(contract_sse: &str)`.
- lens: DNS presentation → `BackendExtra::Dns` (incl. `resolved_ips`, which the IP section still reads in this phase); `[backends.dns] url` and `[backends] dns_servers` refused (→ `[modules.dns] servers`).

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R15: crates/dns is netray-dns, NETRAY_DNS_ / NETRAY_DNS_CONFIG, PRISM_ refused naming the new prefix | open | tests/repo/test_env_prefixes.sh |
| C2 | R16: pipeline extracted and shared; prism's event stream unchanged; ModuleConfig; Module (dns.<v1>) with domain/server validation, @system policy, per-target cost, circuit breakers, semaphore; translate moved; FactsProvider; golden_module | open | crates/dns/tests/module.rs, crates/dns/tests/*.rs |
| C3 | R17: lens takes DNS from the registry; backends/dns.rs, [backends.dns] url and [backends] dns_servers gone; goldens unchanged | open | crates/lens/tests/*.rs |
| C4 | prism.sse, prism-no-address.sse translated equal the DNS section of their full-output goldens | open | crates/dns/tests/module.rs |
| C5 | facts_from_lookups on the golden's lookups holds its A, AAAA, MX, CAA, NS (and HTTPS when present) | open | crates/dns/tests/module.rs |
| C6 | PRISM_CONFIG set → netray dns refused naming NETRAY_DNS_CONFIG | open | tests/repo/test_env_prefixes.sh |
| C7 | `@system` in `servers` with `allow_system_resolvers = false` → Incomplete with prism's refusal; an invalid domain → Incomplete | open | crates/dns/tests/module_input.rs |
| C8 | full-output and lens_golden unchanged; `crates/lens/src/backends/` holds no section file | open | crates/lens/tests/lens_full_output.rs, lens_golden.rs |
