
## cd0c2a12..eaaf3037

### Reader

COUNTS blockers=0 majors=1 minors=0
LENSES Engineering, Security, Testing
MAJOR | tests/repo/test_api_no_verdict.sh:16 | The no-verdict guard passes a grouped import of `score` from `netray_engine` whenever the import spans more than one line, which rustfmt produces once the list is longer than 100 columns, so a scoring call in `crates/web/src/api/` is not flagged. | A file `crates/web/src/api/run.rs` holding `use netray_engine::{\n    SectionEvent, SectionOutcome, catalogue::CheckSpec, profile::Profile, score,\n};` and `let g = score(run, p);` (`score` is the crate-root re-export at `crates/engine/src/lib.rs:11`): `verdict_offenders` on it prints nothing, because the line-based pattern `netray_engine::\{[^}]*\bscore\b` needs `{` and `score` on one line and `score(` matches no other alternative. The same scan flags the one-line `use netray_engine::{catalogue, score};` (reproduced on scratch copies: multi-line `[]`, one-line `run.rs:1`). Requirement 15 names this input ("a grouped import of score from netray_engine"), and the self-test fixtures plant only the one-line form (`planted3`).

```quote tests/repo/test_api_no_verdict.sh:16
    grep -rnE 'netray_engine::score|scoring::|rescore|history::diff|netray_model::diff::diff|fingerprint|lens::score|score_run|diff::diff|netray_engine::\{[^}]*\bscore\b' \
```

```quote crates/engine/src/lib.rs:11
pub use scoring::score;
```

### Spec reader

COUNTS blockers=3 majors=5 minors=1
REQUIREMENTS met=29 partial=5 missed=0 contradicted=3
LENSES requirements (spec reader: phases 1-7, requirements 1-37 of specs/features/v2-api/spec.md at eaaf3037)
BLOCKER | crates/tls/src/module.rs:150 | v1's TLS inspection type is built from the TLS module's `[modules.tls]` state (timeouts, trust store, resolver, handshake semaphore), not from `[api.tls]` settings; `[api.tls]` holds only the four quotas. Decision log line 317 records the departure but requirement 27 was not amended | Req 27: an operator who sets a TLS job setting for v1 has no `[api.tls]` key for it (only `per_ip_*`/`per_target_*` exist, so any other key is refused by `deny_unknown_fields`), and `POST /api/v1/tls/inspect` runs on `[modules.tls]`'s handshake semaphore and timeouts
BLOCKER | crates/dns/src/module.rs:143 | v1's DNS jobs apply `QueryPolicy`, the circuit breakers and the query semaphore from `[modules.dns]` through `DnsModule::jobs()`; only the limiter quotas come from `[api.dns]`. Decision log line 301 records this without amending requirement 30 | Req 30: `POST /api/v1/dns/query` with 10 record types is validated against `[modules.dns]`'s `max_record_types`/servers, and `[api.dns]` has no key that could set it (it holds only the seven limit keys)
BLOCKER | crates/dns/src/api/mod.rs:157 | prism's own `?stream=false` now reads the job to its end. The old route's 32/64-slot channel blocked the job and answered `{"events":[],"truncated":true}` after 35 s, so a V1 answer changes (decision log line 300 calls the old behaviour a defect) | Req 32: prism `POST /api/query?stream=false` with 10 record types over 4 servers gives 40 batch events against the old 32-slot channel (cd0c2a12 query.rs:416, send blocks at :595). Before: `truncated: true` after 35 s. Now: all events and `truncated: false`
MAJOR | crates/netray/src/main.rs:350 | `ApiState`'s inspector job handles are the lens modules' own (`tls.inspector()`, `dns.jobs()`, `email.inspector()`, `http`), not handles built from `[api.<inspector>]` | Req 1 (ApiState bullet): with `[modules.tls]` and `[api.tls]` both set, v1's TLS handle carries `[modules.tls]`'s settings
MAJOR | crates/web/src/api/config.rs:31 | The `[api.tls]`, `[api.http]` and `[api.dns]` tables hold limit quotas only, with no job settings. `[api.email]` alone adds one job setting (`max_concurrent_inspections`) | Req 8: `[api.tls] max_concurrent_handshakes = 4` or `[api.http] request_timeout_secs = 5` is refused as an unknown field instead of configuring v1's job
MAJOR | tests/repo/test_api_no_verdict.sh:16 | The grouped-import rule is a single-line grep, so a rustfmt-wrapped grouped import of `score` and its later `score::grade(...)` call are not flagged | Req 15: a file under `crates/web/src/api/` with `use netray_engine::{` / `    catalogue::catalogue,` / `    score,` / `};` and `score::grade(&c)` passes the check (reproduced on a scratch copy: grep exit 1)
MAJOR | crates/ip/src/module.rs:110 | `IpModule::lookup_caller` has no UA parser, because `load_for_module` loads the enrichment with `with_user_agent = false`. It fills `user_agent` with an all-"Other" unparsed value instead of the parsed agent ifconfig's root answers | Req 19: production `GET /api/v1/ip` with `User-Agent: curl/8.5.0` answers `user_agent.browser.family = "Other"`, not curl's parsed family/version (decision log lines 326/329 defer the parser)
MAJOR | crates/web/src/api/ip.rs:114 | `?field=user_agent` renders the unparsed value, not "the request's parsed `User-Agent`" | Req 20: production `GET /api/v1/ip?field=user_agent` with `User-Agent: curl/8.5.0` answers `Other, , Other, \n`; only `StubIpLookup` tests exercise the field
MINOR | crates/lens/src/api_v1.rs:52 | v1's `POST /domains/{domain}/runs` increments lens's V1 request counter `lens_check_requests_total{result="fresh"|"joined"}`, a metric of `/api/check` that the spec never extends to v1 (only the plan names it) | No requirement: one v1 run POST and no `/api/check` request raises `lens_check_requests_total{result="fresh"}` by 1

```quote specs/features/v2-api/spec.md:113
27. `POST /tls/inspect` takes `{host, port}` with `deny_unknown_fields`, where `port` defaults to 443; `starttls` is not a field (operator, 2026-10-17). The port decides STARTTLS exactly as tlsight decides it today. It answers tlsight's `InspectResponse` (`crates/tls/src/routes.rs:32`) and applies tlsight's input parsing (`:391`), target policy with the rebinding re-check (`:635-644`), per-IP and per-target cost limits (`security/rate_limit.rs:23-73`) and handshake semaphore. `netray-tls` exposes the inspection as a type built from its `[api.tls]` settings, independent of axum. tlsight's `/api/inspect` and v1 both call it, and tlsight's answers are unchanged.
```

```quote crates/tls/src/module.rs:149
    /// The inspection over this module's own state.
    pub fn inspector(&self) -> crate::inspector::TlsInspector {
        self.state.inspector()
    }
```

```quote specs/features/v2-api/spec.md:317
- v1's TLS inspection runs on the TLS module's state (`[modules.tls]`: timeouts, trust store, resolver, handshake semaphore) through `TlsModule::inspector()`, and `[api.tls]` holds only the per-IP and per-target quotas whose `Arc` lives in `ApiState`, over a second TLS state built from `[api.tls]`: requirement 27's "built from its `[api.tls]` settings" would run two TLS configurations in one process; the cost is one handshake semaphore shared by v1 and lens runs (Phase 5 interface; implementer under the operator's autonomy mandate, 2026-10-18).
```

```quote specs/features/v2-api/spec.md:116
30. `POST /dns/query`, `/dns/trace`, `/dns/compare`, `/dns/authcompare` and `/dns/dnssec` take prism's request bodies (`PostQueryRequest`, `crates/dns/src/api/query.rs:263`; `TraceRequest`, `trace.rs:59`; `DnssecRequest`, `dnssec.rs:55`). They apply `QueryPolicy::validate`, `check_query_cost` per IP, global and per target, the circuit breakers and the query semaphore, with `[api.dns]` values. With `Accept: text/event-stream` they answer prism's events for that job; otherwise one typed JSON object per job collecting those events, never `serde_json::Value`. The fan-out of `query`, `compare` and `authcompare` moves out of the handlers into `netray-dns` functions that prism's routes and v1 both call, and prism's answers are unchanged.
```

```quote crates/dns/src/module.rs:141
    /// The DNS jobs over this module's configuration, circuit breakers and query semaphore, with
    /// no enrichment and no `prism_*` metrics.
    pub fn jobs(&self) -> DnsJobs {
        DnsJobs::new(
            Arc::new(self.config.clone()),
```

```quote specs/features/v2-api/spec.md:301
- v1's DNS jobs run on the DNS module's state through `DnsModule::jobs()` (`QueryPolicy` limits, allow flags, servers, trace settings, breakers and the query semaphore from `[modules.dns]`, shared with lens runs), and `[api.dns]` holds prism's per-IP, per-target, global and stream limits whose `Arc` lives in `ApiState`, over a second DNS state from `[api.dns]`: the same choice as Phase 5's TLS; breakers model provider health and the semaphore waits (Phase 7 interface; implementer under the operator's autonomy mandate, 2026-10-18).
```

```quote specs/features/v2-api/spec.md:120
32. Every V1 API is unchanged: lens's `/api/check*`, `/api/meta` and `/api-docs/openapi.json` (which lists no `/api/v1` path), and each service's routes and documents. The lens goldens and the contract goldens stay byte-identical.
```

```quote crates/dns/src/api/mod.rs:157
    if !stream {
        let mut done = false;
        let timeout = Duration::from_secs(STREAM_TIMEOUT_SECS + 5);
        let finished = tokio::time::timeout(timeout, async {
            while let Some(mut event) = job.next().await {
                record(&mut event, &mut cached, &cache).await;
                done |= event.is_done();
            }
        })
```



```quote specs/features/v2-api/spec.md:300
- prism's `?stream=false` now reads the whole job before answering, over its old channel that answered `truncated: true` after 35 s once a job emitted more events than the buffer held: the move into `DnsJobs` reads the job to its end, every answer within the buffer is byte-identical (the goldens), and the old truncation was a defect, not a contract (review of Phase 7; implementer under the operator's autonomy mandate, 2026-10-18).
```

```quote specs/features/v2-api/spec.md:54
   - `ApiState` holds the history store (optional), the catalogue and profiles, the client-address extractor, the `[api]` limiters, the inspector job handles built from `[api.<inspector>]`, the IP module (optional), and a value implementing the trait `api::DomainRuns` (requirement 14).
```

```quote crates/netray/src/main.rs:350
                tls: Some(tls.inspector()),
                http: Some(http as Arc<dyn netray_http::HttpInspect>),
                email: Some(Arc::new(email.inspector()) as Arc<dyn netray_email::EmailInspect>),
                dns: Some(dns.jobs()),
```

```quote specs/features/v2-api/spec.md:74
   - Each inspector table holds its inspector's limits and job settings; every key defaults to the same key's default in that service's config today.
```

```quote crates/web/src/api/config.rs:31
/// The limiters of `POST /tls/inspect`; the inspection itself runs on `[modules.tls]`.
```

```quote specs/features/v2-api/spec.md:97
15. The API computes no verdict. `tests/repo/test_api_no_verdict.sh` fails when a file under `crates/web/src/api/` names `netray_engine::score`, `scoring::`, `rescore`, `history::diff`, `netray_model::diff::diff`, `fingerprint`, `lens::score`, `score_run`, `diff::diff` or a grouped import of `score` from `netray_engine`. It scans only that directory. A self-test on a planted fixture must be flagged, and a clean fixture must pass. A test asserts that the `run` and `diff` served by requirements 10, 11 and 13 equal, as JSON, what the store returns for that entry.
```

```quote tests/repo/test_api_no_verdict.sh:16
    grep -rnE 'netray_engine::score|scoring::|rescore|history::diff|netray_model::diff::diff|fingerprint|lens::score|score_run|diff::diff|netray_engine::\{[^}]*\bscore\b' \
```

```quote specs/features/v2-api/spec.md:101
19. `GET /ip` answers the caller's address (requirement 9), and `GET /ip/{address}` answers any global address. Both answer with the `Ifconfig` the IP lookup seam produces (requirement 36): `GET /ip/{address}` through `lookup(ip)` (`IpModule::lookup`, `crates/ip/src/module.rs:91-118`, `skip_dns`, no user agent), `GET /ip` through `lookup_caller(remote, user_agent)` (the caller's address of requirement 9 with the connection's peer port, the request's `User-Agent`, the UA parser and the PTR lookup, as ifconfig's root answers its caller). A non-global `{address}` answers `400 INVALID_INPUT` when `[api.ip] internal_mode` is false (default false, ifconfig's key and default): the same `is_global_ip` check ifconfig applies (`crates/ip/src/backend/mod.rs:82`, at `routes.rs:312`) under the same condition. Without `[modules.ip]`, every `/ip*` and `/asn/*` endpoint answers `503 UNAVAILABLE`.
```

```quote crates/ip/src/module.rs:108
            let mut ifconfig = enrich(&ctx, &self.dns_cache, remote, user_agent, false, None).await;
            if ifconfig.user_agent.is_none() {
                ifconfig.user_agent = user_agent.map(UserAgent::unparsed);
            }
```

```quote crates/ip/src/enrichment.rs:102
    pub async fn load_for_module(config: &Config) -> Result<Self, LoadError> {
        Self::load_with(config, false).await
    }
```

```quote specs/features/v2-api/spec.md:104
    - caller-dependent, accepted only on `GET /ip?field=` without an address: `tcp` (`:125`, the peer port), `user_agent` (`:331`, the request's parsed `User-Agent`), `host` (`:510`, the caller's PTR) and `headers` (`:353`, the request headers through `extract_headers` and `filter_headers` with `[api.ip] filtered_headers`, default empty as ifconfig's, `crates/ip/src/extractors.rs:89-109`).
```

```quote crates/web/src/api/ip.rs:114
            Self::UserAgent => handlers::user_agent::to_plain(ifconfig),
```

```quote crates/lens/src/api_v1.rs:49
                    if stream.joined() {
                        crate::metrics::count_request("joined");
                    } else {
                        crate::metrics::count_request("fresh");
                        self.state.client_runs.record(client);
                    }
```

### Refuted

None dropped. F3 (`crates/dns/src/api/mod.rs:157`, prism's `?stream=false` reading the whole job) came back UNCERTAIN (confidence 6) and is downgraded to MINOR.

### Calibration

| finding | refuter | confidence | result | command |
|---|---|---|---|---|
| F1 BLOCKER `crates/tls/src/module.rs:150` | CONFIRMED | 8 | held | read `TlsModule::inspector` and `crates/netray/src/main.rs` `ApiParts` |
| F2 BLOCKER `crates/dns/src/module.rs:143` | CONFIRMED | 8 | held | read `DnsModule::jobs`; `[api.dns]` has only the seven limit keys |
| F3 BLOCKER→MINOR `crates/dns/src/api/mod.rs:157` | UNCERTAIN | 6 | not checked (MINOR) | — |
| F4 MAJOR `crates/netray/src/main.rs:350` | CONFIRMED | 7 | held | read the Lens arm's `ApiParts` |
| F5 MAJOR `crates/web/src/api/config.rs:31` | CONFIRMED | 7 | held | read `ApiTlsConfig`/`ApiHttpConfig`/`ApiDnsConfig` |
| F6 MAJOR `tests/repo/test_api_no_verdict.sh:16` | CONFIRMED | 8 | held | the script's grep over a scratch file with a wrapped `use netray_engine::{ … score, };` prints nothing |
| F7 MAJOR `crates/ip/src/module.rs:110` | CONFIRMED | 8 | held | `load_for_module` → `load_with(config, false)` (no UA parser) |
| F8 MAJOR `crates/web/src/api/ip.rs:114` | CONFIRMED | 8 | held | follows F7: `to_plain` over the unparsed value |

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | convergence | no new Status/Verdict enum; codenames only in internal metric prefixes |
| P10 | absence | no new check or scoring path; API timeouts map to 503 |
| P12 | convergence | DNS hop/level timeouts, IP batch timeout; no new reqwest client |
| P13 | convergence | separate limiter state per inspector family, API and domain run slots |
| P18 | convergence | no new reqwest client; `test_config_strict.sh` green |
| P26 | absence | no new check ID |
| P35 | convergence | new `test_api_no_verdict.sh`, `test_api_sdd.sh`, check-config rows, api-v1 and prism goldens |
| P36 | absence | no generated artefact besides the api-v1 golden, which moves with its source |
| P40 | convergence | engine gains derives and `Profile::versions` only; no module crate path |

### Summary

Before refutation: reader 0/1/0, spec reader 3/5/1 (REQUIREMENTS met=29 partial=5 missed=0 contradicted=3); merged (F6 reported by both) 3/7/1 → 3 blockers, 6 majors, 1 minor. After refutation: 2 blockers, 5 majors, 2 minors (F3 downgraded; lens's `lens_check_requests_total` on v1 runs). verified 7, held 7. Roll call: 9 answered, 6 convergence, 0 divergence, 3 absence. F1, F2, F4, F5, F7 and F8 are requirement texts the Decision log already departs from; F6 is a gap in the no-verdict guard.

## cd0c2a12..f6099277

### Reader

COUNTS blockers=0 majors=0 minors=1
LENSES Engineering, Security, Testing
MINOR | tests/repo/test_api_no_verdict.sh:17 | The no-verdict guard passes a file under `crates/web/src/api/` that imports and calls the diff function `netray_model::diff::diff` through a grouped import, because the multi-line grouped-import scan covers only `score` from `netray_engine`. | Scratch root containing `crates/web/src/api/run.rs` with `use netray_model::diff::{Diff, diff};` and `fn changes(prev: &Run, cur: &Run) -> Diff { diff(prev, cur, &[], None) }`. Running the script's `verdict_offenders` (copied verbatim, lines 15-23) on that root returns nothing, so the guard passes. This is the natural way to write the call, since `runs.rs:17` and `reads.rs:9` already `use netray_model::diff::Diff;`. Rated MINOR because requirement 15 lists the grouped-import form only for `score`.

```quote tests/repo/test_api_no_verdict.sh:17
        grep -rnE 'netray_engine::score|scoring::|rescore|history::diff|netray_model::diff::diff|fingerprint|lens::score|score_run|diff::diff|netray_engine::\{[^}]*\bscore\b' \
```

```quote tests/repo/test_api_no_verdict.sh:21
            xargs -0 perl -0777 -ne 'while (/netray_engine::\{[^}]*\bscore\b/gs) { my $l = 1 + (substr($_, 0, $-[0]) =~ tr/\n//); print "$ARGV:$l\n" }' 2>/dev/null
```

Traced and found sound:
- v1 router: mount, JSON 404/405/413, per-IP route layer, docs CSP.
- Domain/entry reads and their 404/410/500/503 split.
- `DomainRuns::start` over `check_limits` then `admit`, with the refusal mapping and counting.
- JSON and SSE run answers.
- The requirement-37 head-read/throttle race path in `/api/check`.
- `RunCheck` findings and evidence: outside the fingerprint, defaulted on read.
- IP seam: caller/address/field/batch/range/diff/asn, per-target and batch limiters, config caps.
- ifconfig `/diff`, `/batch`, `/range` and `/asn` answers unchanged by the move: same key set and order, same messages.
- TLS, HTTP and email seams and the beacon route refactor: same order of parse, limit and slot.
- DNS jobs against the old prism handlers: authcompare body diffed; prism goldens unchanged since their pre-move capture.
- `respond`'s stream=false and cache handling.
- `[api.*]` config validation with its startup rejects.
- `compose`.
- The contract compatibility check and the untyped-schema walk.

### Spec reader

COUNTS blockers=0 majors=0 minors=1
REQUIREMENTS met=37 partial=0 missed=0 contradicted=0
LENSES n/a (spec reader: requirements 1-37 of phases 1-7)
MINOR | crates/lens/src/api_v1.rs:50 | v1's `POST /domains/{domain}/runs` increments lens's V1 `lens_check_requests_total{result="fresh"|"joined"}` (and records `lens_client_hourly_runs`), a metric side effect no requirement names; the spec names only `lens_admission_rejections_total` for v1 runs and leaves run counting by trigger to v2-cutover | Unrequested behaviour: `POST /api/v1/domains/example.com/runs` admitted fresh runs `count_request("fresh")`, so `lens_check_requests_total{result="fresh"}` rises by one with no `/api/check` request made; R14 asks only for the rejection counter

```quote crates/lens/src/api_v1.rs:49
                    if stream.joined() {
                        crate::metrics::count_request("joined");
                    } else {
                        crate::metrics::count_request("fresh");
                        self.state.client_runs.record(client);
                    }
```

```quote crates/lens/src/metrics.rs:74
pub fn count_request(result: &'static str) {
    metrics::counter!("lens_check_requests_total", "result" => result).increment(1);
}
```

```quote specs/features/v2-api/spec.md:90
    - A refused run answers `429` with the `Retry-After` v2-admission computes (`retry_after_s`) and one code per refusal reason: `throttle` → `THROTTLED` (seconds left in the minute), `budget` → `BUDGET_EXHAUSTED` (the budget limiter's), `ip_limit` and `global_limit` → `RATE_LIMITED`, `slots` → `BUSY` (`Retry-After: 1`) (codes default, not interviewed). Each counts `lens_admission_rejections_total{reason}` as v2-admission counts it (`ip_limit` and `global_limit` as `limit`). A refused request spends no budget token, holds no slot and starts no throttle minute.
```

Traced at f6099277: the API router, state, config, error, limiter and every handler module under crates/web/src/api/ (reads, runs, ip, inspectors, email, dns, docs, contract); lens's `LensRuns`, its `/api/v1` mount, 413 rewrite and docs CSP, and the requirement-37 head-read/admit path in routes.rs; `build_run`'s findings/evidence copy and the `RunCheck` fields; the netray-ip `IpLookup` seam, `IpModule` impl, `StubIpLookup` and ifconfig's moved batch/range/diff/asn logic; the TLS inspector, HTTP job, email job and DNS jobs seams; crates/netray's `ApiParts` wiring; the justfile gate entry; and the SDD 3.1/4.1/7.1/10 amendments. Ran `test_api_no_verdict.sh`, `test_api_sdd.sh`, `test_web_crate.sh` and `test_lens_names_no_module.sh` (all PASS).

### Refuted

No BLOCKER or MAJOR finding: no refutation pass.

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | convergence | codenames only in doc comments and `prism_*` metric names, none in API serializers |
| P10 | absence | no changed check or scoring branch |
| P12 | convergence | DNS jobs carry resolver and trace timeouts; no reqwest client in the API |
| P13 | convergence | own limiter per consumer; reads behind the `[api]` limiter, runs through admission |
| P18 | convergence | no new reqwest client; new config structs reject unknown keys |
| P26 | absence | no new check ID |
| P35 | convergence | `test_api_no_verdict.sh`, `test_api_sdd.sh`, `test_web_crate.sh`, the api-v1 golden |
| P36 | absence | goldens move with their producing code |
| P40 | convergence | engine gains derives only; no module crate path |

### Summary

Before and after refutation: reader 0/0/1 (the no-verdict guard's grouped-import scan covers `score` only, not `diff`), spec reader 0/0/1 (REQUIREMENTS met=37 partial=0 missed=0 contradicted=0; v1 runs count `lens_check_requests_total`). verified 0, held 0. Roll call: 9 answered, 6 convergence, 0 divergence, 3 absence. The first reading's blockers and majors are resolved by 061ab907 and 87b59002.
