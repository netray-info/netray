
## f268717d..90a63b3

### Reader

COUNTS blockers=2 majors=1 minors=1
LENSES Engineering, Security, Testing
BLOCKER | crates/netray/src/legacy.rs:140 | The legacy refusal for spectra's top-level `[enrichment]` sends the operator to `[modules.http].enrichment`. That table no longer exists: it is itself refused as a legacy key (legacy.rs:39), and http's `ModuleConfig` has no `enrichment` field. CHANGELOG.md:118 makes the same claim, which breaks CLAUDE.md's "Release notes trace to code". | A config with `[enrichment]` / `ip_url = "http://ifconfig:8080"` exits 1 saying "spectra: now `[modules.http].enrichment`". Moving the table to `[modules.http.enrichment]`, as told, exits 1 again with "removed in 0.30.0, enrichment calls the in-process ip module". The correct fate is InProcess/removed.
BLOCKER | crates/netray/src/legacy.rs:93 | The legacy refusal for ifconfig's `[rate_limit] exempt_cidrs` names `[api.ip] exempt_cidrs`. `ApiIpConfig` (crates/web/src/api/config.rs:179) has no such field, so the named destination is refused as an unknown key. The key's only purpose, exempting lens's calls to ifconfig, ended with in-process enrichment. CHANGELOG.md:96 repeats the wrong destination. | `[rate_limit]` / `exempt_cidrs = ["172.31.0.0/24"]` exits 1 naming `[api.ip] exempt_cidrs`. Writing `[api.ip]` / `exempt_cidrs = ["172.31.0.0/24"]` then fails deserialization with "unknown field `exempt_cidrs`". The correct fate is "removed in 0.30.0".
MAJOR | crates/netray/src/config.rs:254 | `module_tables()` strips `deadline_ms` and then drops tables left empty, before `module_registry` runs its "unknown module" refusal (registry.rs:143). A misspelt module table that holds only a section window therefore passes `--check-config` and `serve`, and the window it meant to set is silently ignored. | `[modules.emial]` / `deadline_ms = 8000` (or `NETRAY_MODULES__EMIAL__DEADLINE_MS=8000`): `legacy::refuse` passes it, `validate()` only reads the five known names, and `module_tables()` removes the table. `netray serve --check-config` prints `config ok`, and the email section runs on the 2000 ms default. Any other key in the same table, e.g. `[modules.emial] bogus = 1`, is refused as "unknown module". No test covers the unknown-module refusal.
MINOR | crates/netray/netray.example.toml:1 | The example says every key is shown at its default (requirement 12), but `[inspect.email]` is missing. That table exists with its own default deadline of 35000 ms (crates/web/src/pages/inspect.rs:280) and is accepted by the config. | The file lists `[inspect.dns]`, `[inspect.tls]`, `[inspect.http]` and `[inspect.ip]` (ending at line 93) and no `[inspect.email]`. An operator reading it cannot find the email inspector's deadline and concurrency keys. lens's example listed all five.

```quote crates/netray/src/legacy.rs:140
    ("enrichment", "spectra", Now("[modules.http].enrichment")),
```

```quote crates/netray/src/legacy.rs:39
    ("modules.http.enrichment", "lens", InProcess),
```

```quote CHANGELOG.md:118
| `enrichment` | spectra | `[modules.http].enrichment` |
```

```quote crates/netray/src/legacy.rs:93
        Now("[api.ip] exempt_cidrs"),
```

```quote crates/web/src/api/config.rs:179
pub struct ApiIpConfig {
```

```quote CHANGELOG.md:96
| `rate_limit.exempt_cidrs` | ifconfig | `[api.ip] exempt_cidrs` |
```

```quote crates/netray/src/config.rs:254
        modules.retain(|_, table| !table.is_empty());
```

```quote crates/netray/src/registry.rs:143
        .find(|k| !matches!(k.as_str(), "http" | "email" | "ip" | "tls" | "dns"))
```

```quote crates/netray/netray.example.toml:1
# netray.toml: the one configuration of `netray serve`. Every key is shown at its default,
```

```quote crates/web/src/pages/inspect.rs:280
            email: InspectLimits::with_deadline(35_000),
```

### Spec reader

COUNTS blockers=1 majors=7 minors=3
REQUIREMENTS met=19 partial=7 missed=0 contradicted=1

BLOCKER | crates/netray/src/legacy.rs:140 | The legacy row for spectra's top-level `enrichment` sends the operator to `[modules.http].enrichment`, but no module config has that field any more, and the same table refuses that key path as removed (line 39). | Req 11, `enrichment` row. Input: a file holding `[enrichment] ip_url = "http://x"`. `walk` matches pattern `enrichment` and fails with "spectra: now `[modules.http].enrichment`". Moving the key there matches `modules.http.enrichment` (InProcess) and is refused again. `netray_http::ModuleConfig` (crates/http/src/config.rs:8) has only `inspect` and `limits`, so the words call for the CHANGELOG pointer here.

```quote specs/features/v2-cutover/spec.md:191
   - `limits`, `circuit_breaker`, `trace`, `validation`, `quality`, `enrichment`, `dns`, `dnsbl`, `http`, `dkim`, `batch`, `filtered_headers`, `internal_mode`, `watch_data_files`, `dns_base_url`, `tls_base_url`, and every path key of ifconfig's `Config`: `geoip_city_db`, `geoip_asn_db`, `user_agent_regexes`, `tor_exit_nodes`, `cloud_provider_ranges`, `feodo_botnet_ips`, `cins_army_ips`, `vpn_ranges`, `datacenter_ranges`, `bot_ranges`, `spamhaus_drop`, `asn_patterns`, `asn_info`. The message names `[modules.<m>].<key>` when exactly one module's config has a field of that name, otherwise points to the CHANGELOG table, which gives each key's fate (`user_agent_regexes`, absent from ip's `ModuleConfig`, included).
```
```quote crates/netray/src/legacy.rs:140
    ("enrichment", "spectra", Now("[modules.http].enrichment")),
```
```quote crates/netray/src/legacy.rs:39
    ("modules.http.enrichment", "lens", InProcess),
```

MAJOR | tests/fixtures/contracts/lens-healthy.json:1 | The `tests/fixtures/contracts/lens-*.json` goldens (17 files) are still tracked and are now read by the ported `crates/web/tests/it/run_golden.rs`, although requirement 6 lists them as gone. No Decision log entry keeps them. | Req 6. `git ls-files tests/fixtures/contracts/lens-healthy.json` prints the path at END.

```quote specs/features/v2-cutover/spec.md:137
   - all of `crates/lens/`, including its snapshot store, moka cache, SPA, OG and badge, `migrations/`, tests and `tests/fixtures/contracts/lens-*.json`;
```

MAJOR | crates/web/src/entries/routes.rs:126 | The V1 snapshot step is still in `EntriesDeps`/`EvidenceDeps` (`v1: Option<V1Lookup>`, documented as "V1 snapshot lookup handed in by lens"), and `entries_router.rs:306` still tests it. Only `serve` passing `None` produces the 410. | Req 23, with 13j and the Phase 3 Decision log that deferred "deleting `EntriesDeps`' V1 lookup to Phase 7 with requirement 23". Input: `/r/<id>` for a non-entry with `v1: Some(..)`. The handler returns the V1 response instead of 410.

```quote specs/features/v2-cutover/spec.md:210
    - j. Apex `/r/<id>`, `/r/<id>/og.png` and `/r/<id>/<protocol>` for a well-formed ID that is no stored entry → 410. The page routes answer with the §4.6 page, which names no domain, and `og.png` with an empty body (as `v2-entries` does). A V1 snapshot ID is such an ID. `v2-entries`' V1-snapshot step is removed. A malformed ID still answers 404.
```
```quote crates/web/src/entries/routes.rs:126
            if let Some(v1) = &deps.v1
```

MAJOR | tests/acceptance/agreement/prod.spec.ts:51 | The prod agreement spec relies on markup that does not exist. Its entry-ID regex needs `og:url` with `/r/<id>`, which only `pages/entry.html` renders, not `/{domain}` or `/{domain}/{protocol}`. Its issues regex needs a `check-row__tone` span, which `components/check_row.html` never emits. The `TODO(v2-result-page): confirm selector` notes were never resolved. | Req 22. Input: `GET /netray.info` after the API run. `viewEntryId` finds no match, so `expect(match).not.toBeNull()` fails every time. `viewIssues` always returns `[]`, so the "open issues equal API warn/fail" assertion fails whenever the API reports one, and is never really checked.

```quote specs/features/v2-cutover/spec.md:265
    - the page grade equals the API grade equals the badge letter;
    - the open issues equal the API's warn and fail checks;
    - every evidence view names the same entry.
```
```quote tests/acceptance/agreement/prod.spec.ts:38
      /<span class="check-row__code">([^<]+)<\/span>\s*<span class="check-row__tone">([^<]+)<\/span>/g,
```
```quote tests/acceptance/agreement/prod.spec.ts:51
  const match = html.match(/<meta property="og:url" content="[^"]*\/r\/([0-9A-Za-z]{8})"/);
```

MAJOR | crates/netray/netray.example.toml:121 | The example config lists only `deadline_ms` (plus the two GeoIP paths for ip) under `[modules.*]`. It leaves out every other module key: `[modules.dns] servers`/`limits`/`circuit_breaker`, `[modules.tls] validation`/`quality`, `[modules.http.inspect]`, `[modules.email] dkim`/`dnsbl`/`inspections`, and ip's other eleven data keys. | Req 12. Input: look up `tor_exit_nodes` or `max_redirects` in the file. Neither is there. `example_config_holds_every_table_and_key` only checks `deadline_ms` for the module tables.

```quote specs/features/v2-cutover/spec.md:196
12. `crates/netray/netray.example.toml` holds every top-level table with every key at its default, commented. `crates/netray/netray.dev.toml` is the dev config. `--check-config` exits 0 on both. Startup refusals keep their tests: an unopenable `[store] db_path` and missing GeoIP files exit non-zero at startup while `--check-config` on the same file exits 0. They become `startup_rejects` rows of `test_check_config.sh` for `serve`.
```
```quote crates/netray/netray.example.toml:121
# [modules.ip]
```

MAJOR | tests/repo/test_check_config.sh:87 | The rewrite dropped the startup_rejects rows of keys whose validate rules still exist in `crates/web/src/api/config.rs`. Rows gone: `[api.dns]` `per_ip_per_minute`, `per_target_per_minute`, `per_target_burst`, `global_per_minute`; `[api.tls]` `per_ip_burst`, `per_target_per_minute`; `[api.http]` `per_ip_per_minute`, `per_target_burst`; `[api.email]` `per_ip_burst`; `[api.ip]` `per_ip_burst = 0`, `per_target_per_minute = 601`, `per_target_burst = 0`, `batch_addresses_per_minute = 0`, `batch_addresses_burst = 101`. All were rows at f268717d. | Req 9. Input: delete the zero check for `api.dns.per_ip_per_minute`. No startup_rejects row fails.

```quote specs/features/v2-cutover/spec.md:171
   Every key keeps its default and its `validate()` rule and `startup_rejects` row.
```
```quote tests/repo/test_check_config.sh:87
    "api.dns|per_ip_burst = 17|api.dns.per_ip_burst"
```

MAJOR | tests/repo/test_v1_route_coverage.sh:50 | The coverage list leaves out lens's `/r/{shortid}` with its sub-paths and `/og/{domain_png}`, both V1 routes listed in Context, so the check never requires a row for them. | Req 15. Input: delete `host_redirects.rs` lines 56-57 (the only `/r/` rows in the table). `test_v1_route_coverage.sh` still prints "ok: every V1 route has a row".

```quote specs/features/v2-cutover/spec.md:228
15. The tests of requirements 13–14 are table-driven. A repo test fails when a route of the deleted V1 routers listed in Context (the §7.2/§7.3 sources), or a `site/` path of rows l–n, has no row. The list is kept in the test, because the source files are deleted.
```
```quote specs/features/v2-cutover/spec.md:76
  - lens: `/api/check`, `/api/check/{domain}`, `/api/meta`, `/docs`, `/api-docs/openapi.json` (`crates/lens/src/lib.rs:155-161`), `/og/{domain_png}`, `/r/{shortid}` and its sub-paths (`crates/lens/src/routes.rs:1570-1573`). V1 short IDs are 8 characters of `[0-9A-Za-z]` (`crates/lens/src/snapshot/shortid.rs:8`), the same shape as history entry IDs (`v2-history` R8). So a V1 ID can only be told apart by not being an entry.
```
```quote tests/repo/test_v1_route_coverage.sh:50
    for n in /api/check '/api/check/*' /api/meta /docs /api-docs/openapi.json; do
```

MAJOR | CHANGELOG.md:118 | Two 0.30.0 claims do not match the code, yet report.md's claim table cites code for them. First, "`netray.example.toml` lists every key at its default" (line 65) is false (see the req 12 finding). Second, the `enrichment` row names a destination the code refuses. | Req 24. Check line 118 against `legacy.rs:39`, and line 65 against the example file's `[modules.*]` sections.

```quote specs/features/v2-cutover/spec.md:283
    Each claim is traced to code at the release commit, not to specs or the SDD (CLAUDE.md "Release notes trace to code"), and `report.md` records it as a claim → path table. Only `netray.info` and `example.com` appear.
```
```quote CHANGELOG.md:118
| `enrichment` | spectra | `[modules.http].enrichment` |
```

MINOR | crates/web/src/host/legacy.rs:183 | The apex answers extensionless `/api/index`, `/api/dns` … `/api/lens` and bare `/api` with a 301 to `/api/v1/docs`. Row i gives every other `/api/<x>` a 410, and row m covers only `/api/` and `/api/<name>.html`. The change is recorded only as a report.md review amendment, not in the spec. | Row 13i. Input: `GET /api/lens` on the apex. The code returns 301; the spec's words call for a 410 naming `/api/v1`.

```quote specs/features/v2-cutover/spec.md:209
    - i. Apex `/api/check` and `/api/check/<domain>` → 410 naming `https://<apex>/api/v1/domains/{domain}/runs`. Any other apex `/api/<x>` with `x` ≠ `v1` (for example `/api/meta`) → 410 naming `https://<apex>/api/v1` (default, not interviewed). Row m comes first.
```
```quote crates/web/src/host/legacy.rs:183
                | "/api/index"
```

MINOR | crates/netray/src/config.rs:36 | Besides `NETRAY_LIVE_TESTS`, config loading also filters `NETRAY_BIN` and `NETRAY_DESIGN_DIR` out of the config keys; the spec names only the live-test switch. | Req 10. Input: `NETRAY_BIN=/x netray serve`. The variable is silently ignored instead of failing `deny_unknown_fields`.

```quote specs/features/v2-cutover/spec.md:174
10. Before loading, `serve` and `serve --check-config` refuse with exit 1 when any variable starts, ASCII case-insensitively, with one of `LENS_`, `NETRAY_DNS_`, `NETRAY_TLS_`, `NETRAY_HTTP_`, `NETRAY_EMAIL_`, `NETRAY_IP_`, `PRISM_`, `TLSIGHT_`, `SPECTRA_`, `BEACON_` or `IFCONFIG_`. Each prefix is checked through `refuse_legacy_prefix(<legacy>, "NETRAY_", …)`, whose message names the variable, `NETRAY_*` and `NETRAY_CONFIG`. If live reference tests survive, their switch is `NETRAY_LIVE_TESTS`, filtered out of config keys as `LENS_LIVE_TESTS` is today.
```
```quote crates/netray/src/config.rs:36
const TOOL_VARS: [&str; 3] = ["NETRAY_LIVE_TESTS", "NETRAY_BIN", "NETRAY_DESIGN_DIR"];
```

MINOR | .github/workflows/release.yml:92 | The release smoke step also requests `/` on every host, which the spec does not ask for. A host whose `/` fails now blocks the push. | Req 8. The spec asks for `/health` only, with the apex and the four inspector hosts.

```quote specs/features/v2-cutover/spec.md:148
8. The `Dockerfile` builds no Solid frontend. The image carries `netray.example.toml` and no `site/`, and its entrypoint runs `netray serve`. `release.yml` smoke-tests the built image by starting `serve` once and requesting `/health` with the apex `Host` and with each of `dns.`, `tls.`, `http.` and `ip.`. `specs/rules/workflow-rules.md` lines 76 and 81 (R-R4) say "`netray serve`, every host" instead of "every subcommand". The image still carries no data files (`test_image_data.sh`, release.yml data steps unchanged).
```
```quote .github/workflows/release.yml:92
            check smoke-serve "$host" / || failed=1
```

Requirement 27 is no phase's requirement, so it carries no mark.

### Refuted

None: the refuter confirmed every BLOCKER and MAJOR.

### Calibration

| finding | refuter | confidence | result | check |
|---|---|---|---|---|
| B1 legacy `enrichment` row | CONFIRMED | 9 | held | `netray serve --check-config` on `[enrichment] ip_url`, then on `[modules.http.enrichment]`: both refused |
| B2 legacy `rate_limit.exempt_cidrs` row | CONFIRMED | 9 | held | `[rate_limit] exempt_cidrs` refused naming `[api.ip] exempt_cidrs`; `[api.ip] exempt_cidrs` refused as unknown field |
| M1 misspelt module table with only `deadline_ms` | CONFIRMED | 8 | held | `[modules.emial] deadline_ms = 8000`: `config ok` |
| M2 `lens-*.json` still tracked | CONFIRMED | 8 | held | `git ls-files 'tests/fixtures/contracts/lens-*.json'`: 17 |
| M3 V1 lookup in `EntriesDeps` | CONFIRMED | 6 | held | `grep -n 'v1:' crates/web/src/entries/routes.rs`: line 49 |
| M4 prod agreement selectors | CONFIRMED | 8 | held | `grep check-row__tone crates/web/templates/components/check_row.html`: none |
| M5 example lacks module keys | CONFIRMED | 6 | held | read `crates/netray/netray.example.toml` `[modules.*]` |
| M6 dropped startup_rejects rows | CONFIRMED | 5 | held | diff of the row list against f268717d's test_check_config.sh |
| M7 route coverage lacks `/r/`, `/og/` | CONFIRMED | 9 | held | `grep "/og/" tests/repo/test_v1_route_coverage.sh`: none |
| M8 CHANGELOG claims | CONFIRMED | 8 | held | B1 and M5 above |

### Summary

Before refutation: reader 2/1/1, spec reader 1/7/3 (REQUIREMENTS met=19 partial=7 missed=0 contradicted=1). B1 was reported by both readers and counts once. After refutation: 2 blockers, 8 majors, 4 minors; verified 10, held 10. No roll call: `adlc.toml` declares no principles with a sentence. The blockers and majors are fixed on this branch and read again (next section).

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | convergence | no `*Status`/`*Verdict` enum outside crates/model; no codename in crates/web templates or sources; site/ deleted |
| P10 | absence | engine timeout branches map to `Timeout`; module error branches not traced in this range |
| P12 | convergence | `crates/common/src/fetch.rs` and the telemetry client carry timeouts; `HARD_DEADLINE` 20 s |
| P13 | convergence | domain-run slots in `crates/web/src/admission/state.rs`; one semaphore per inspector in `crates/web/src/pages/inspect.rs` |
| P18 | convergence | no `reqwest::Client` outside fetch.rs but the OTLP exporter |
| P26 | absence | the range adds no check ID |
| P35 | divergence | logging-rules.md and architecture-rules.md change without a tests/repo check that enforces them |
| P36 | convergence | no generated file changes without its source |
| P40 | convergence | crates/engine/src names no module crate |

Totals: answered 9, convergence 6, divergence 1, absence 2.

## 90a63b3..2a77462

### Reader

COUNTS blockers=0 majors=0 minors=1
LENSES Engineering, Security, Testing
MINOR | crates/netray/netray.example.toml:129 | The new example config says `failure_threshold` must lie strictly between 0.0 and 1.0, but the DNS loader accepts 1.0. Only the 1.0 edge is wrong: the default (0.5) and the rejection of 0.0 match the code. | `[modules.dns.circuit_breaker]` / `failure_threshold = 1.0`: the example marks 1.0 as out of range, but `DnsModule::new` → `into_config` → `validate` rejects only `<= 0.0` or `> 1.0`, so `netray serve --check-config` prints `config ok`. The DNS error text itself says `(0.0, 1.0]`.

```quote crates/netray/netray.example.toml:129
# failure_threshold = 0.5            # between 0.0 and 1.0, both exclusive
```

```quote crates/dns/src/config.rs:365
            || self.circuit_breaker.failure_threshold > 1.0
```

### Spec reader

COUNTS blockers=0 majors=2 minors=2
REQUIREMENTS met=6 partial=2 missed=0 contradicted=0
MAJOR | tests/repo/test_check_config.sh:62 | The TLS startup reject for a missing `custom_ca_dir` has no `startup_rejects` row and no test anywhere at END. At f268717d it had a row (tlsight fixture, `custom_ca_dir = "/nonexistent-ca-dir"`). Its `validate()` rule still runs through `ModuleConfig::into_config` → `check_startup`. | Req 9 ("every key keeps … its `startup_rejects` row"). Case: delete `cfg.check_startup()?` from crates/tls/src/config.rs:96. `[modules.tls.validation] custom_ca_dir = "/nonexistent-ca-dir"` then passes `serve --check-config`, `test_check_config.sh` still passes, and `git grep check_startup` finds no test.
MAJOR | CHANGELOG.md:37 | The Removed table says every other apex `/api/<x>` answers 410, and row 38 names only `/api/` and `/api/<name>.html` as 301. In code, bare `/api` and the extensionless `/api/index`, `/api/dns` … `/api/lens` answer 301 to `/api/v1/docs`. This range records that in the Decision log (spec.md:580) but leaves the CHANGELOG claim as it was. | Req 24 ("each claim is traced to code"). Case: `GET netray.info/api/dns` reaches `apex_legacy`'s first branch (legacy.rs:184) and gets 301 `https://netray.info/api/v1/docs`. CHANGELOG line 37 promises a 410 naming `/api/v1`.
MINOR | crates/netray/netray.example.toml:129 | This range added a comment that says `failure_threshold` must lie "between 0.0 and 1.0, both exclusive". The rule accepts 1.0. | Req 12 (example text). Case: `[modules.dns.circuit_breaker] failure_threshold = 1.0` passes `failure_threshold <= 0.0 || failure_threshold > 1.0` (crates/dns/src/config.rs:365) and loads, although the example calls it out of range.
MINOR | crates/web/src/entries/mod.rs:2 | This range removed `V1Lookup` from this file, but the module doc still describes `/r/<id>` as "history first, then the V1 snapshots lens hands in". That step no longer exists. This is a doc-only leftover, so req 23 still counts as met. | Req 23 (no tracked file names the snapshot lookup). Case: `git grep -n 'V1 snapshots' crates/web/src` at END prints this line. `entry_page` answers `Ok(None)` with 410 directly (routes.rs).

```quote specs/features/v2-cutover/spec.md:171
   Every key keeps its default and its `validate()` rule and `startup_rejects` row.
```
```quote tests/repo/test_check_config.sh:62
startup_rejects=(
```
```quote crates/tls/src/config.rs:96
        cfg.check_startup()?;
```
```quote specs/features/v2-cutover/spec.md:283
    Each claim is traced to code at the release commit, not to specs or the SDD (CLAUDE.md "Release notes trace to code"), and `report.md` records it as a claim → path table. Only `netray.info` and `example.com` appear.
```
```quote CHANGELOG.md:37
| any other `netray.info/api/<x>` but `/api/v1` | 410, successor `/api/v1` |
```
```quote CHANGELOG.md:38
| `netray.info/api/`, `/api/<name>.html` (V1 API docs) | 301 `https://netray.info/api/v1/docs` |
```
```quote crates/web/src/host/legacy.rs:184
                | "/api/dns"
```
```quote specs/features/v2-cutover/spec.md:580
- The apex answers bare `/api` and the extensionless `/api/index`, `/api/dns` … `/api/lens` with row 13m's 301 to `/api/v1/docs`, over row 13i's 410: they name the same V1 docs pages as `/api/<name>.html`, so a reader is sent to their successor (Phase 3 review amendment, recorded here by the branch review 2026-10-22; implementer under the operator's autonomy mandate, 2026-10-22).
```
```quote crates/netray/netray.example.toml:129
# failure_threshold = 0.5            # between 0.0 and 1.0, both exclusive
```
```quote crates/dns/src/config.rs:365
            || self.circuit_breaker.failure_threshold > 1.0
```
```quote specs/features/v2-cutover/spec.md:273
23. No tracked file outside `specs/` and `CHANGELOG.md` names `snapshots.db`, `[snapshots]` (except the legacy table of requirement 11) or the snapshot store. lens's `nanoid` and `sqlx` dependencies go with the crate, unless another crate uses them. V1 `/r/` IDs answer per 13j.
```
```quote crates/web/src/entries/mod.rs:2
//! routes that serve `/r/<id>`: history first, then the V1 snapshots lens hands in.
```

### Refuted

None: the refuter confirmed both MAJORs (confidence 8 each).

### Calibration

| finding | refuter | confidence | result | check |
|---|---|---|---|---|
| M1 no startup_rejects row for `custom_ca_dir` | CONFIRMED | 8 | held | `grep -c custom_ca_dir tests/repo/test_check_config.sh`: 0; the binary still refuses `/nonexistent-ca-dir` |
| M2 CHANGELOG promises 410 for `/api/dns` | CONFIRMED | 8 | held | `grep -n '"/api/dns"' crates/web/src/host/legacy.rs`: the 301 branch |

Both MAJORs and both MINORs are fixed in 672a7d92 (row restored, CHANGELOG row widened, the example's range text, the entries module doc).

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | divergence | the example config shows the HTTP module's outbound default `user_agent = "netray-spectra"`, a codename (not UI copy or API output) |
| P10 | absence | no check or scoring change |
| P12 | absence | no outbound client or module entry changed |
| P13 | absence | no limiter changed |
| P18 | convergence | unknown module tables are refused; startup_rejects rows added |
| P26 | absence | no check ID added |
| P35 | absence | no rule changed |
| P36 | absence | no generated file changed |
| P40 | absence | no module added or removed |

Totals: answered 9, convergence 1, divergence 1, absence 7.

### Summary

Before refutation: reader 0/0/1, spec reader 0/2/2 (REQUIREMENTS met=6 partial=2 missed=0 contradicted=0); the example-config MINOR was reported by both and counts once. After refutation: 0 blockers, 2 majors, 2 minors; verified 2, held 2.
