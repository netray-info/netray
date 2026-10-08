# Plan: one outbound fetch policy

## Phase 1 — Results table

### Groups

One group: C1–C11 (beacon tables) share `crates/beacon/src/state.rs`. C12 (spectra) passes today and needs no production change.

### Plan

- `crates/beacon/src/state.rs`: extract the two inline client builders (`:46-54`, `:56-73`) into `pub(crate) fn http_client_builder(timeout_ms: u64) -> reqwest::ClientBuilder` and `pub(crate) fn http_client_follow_builder(timeout_ms: u64) -> reqwest::ClientBuilder`, each returning the builder before `.build()` with today's timeout, redirect policy and user agent. The state constructor calls them and `.build()`s as before. Behaviour unchanged.

The planner agent was not spawned: one file, two functions, nothing to group.

## Phase 2 — Fetch helper

### Groups

One group: C1–C11 all hang on the new module `crates/common/src/fetch.rs`.

### Plan

API contract, fixed here so tests and code agree:

- Feature `fetch` in `crates/common/Cargo.toml` = `["dep:reqwest", "dep:tokio", "dep:url", "dep:thiserror", "dep:bytes", "dep:futures"]` (whatever subset is needed); `[[test]] name = "fetch"`, `required-features = ["fetch"]`. beacon, spectra and tlsight add `"fetch"` to their `netray-common` features, so the workspace test run builds it.
- `pub trait Resolve: Send + Sync { fn resolve(&self, host: &str) -> impl Future<Output = Vec<IpAddr>> + Send; }` and `pub struct SystemResolver;` (via `tokio::net::lookup_host`, errors → empty).
- `pub enum AtLimit { Fail, ReturnLast }`.
- `pub struct FetchOptions { pub method: reqwest::Method, pub body: Option<bytes::Bytes>, pub max_redirects: usize, pub at_limit: AtLimit, pub https_only: bool, pub body_cap: usize, pub timeout: Duration, pub allow: fn(IpAddr) -> bool }`. `allow` defaults to `target_policy::is_allowed_target`. Tests pass a predicate that also admits their own listener addresses. `FetchOptions::new(method)` gives the defaults: 0 redirects, `Fail`, `https_only` false, 64 KiB, 10 s, `is_allowed_target`.
- `pub async fn fetch(base: impl Fn() -> reqwest::ClientBuilder, resolver: &impl Resolve, url: &str, opts: &FetchOptions) -> Result<FetchResponse, FetchError>`. The caller's `base` carries the user agent, TLS settings and default headers. Per hop the helper adds `redirect(Policy::none())`, `timeout`, and `resolve_to_addrs(host, &checked)`, where `checked` carries the URL's port.
- Per hop: parse with `url::Url` (userinfo is ignored as a host; bracketed IPv6 parsed); scheme http/https only, https only if `https_only`; host is an IP literal → that address, else `resolver.resolve(host)`; an empty set, or any address failing `allow` → `Err(FetchError::Blocked { url })` before any connection.
- Redirect semantics as reqwest's default today: 301/302/303 continue as GET without body (HEAD stays HEAD); 307/308 keep method and body. A `Location` is resolved against the current URL.
- `pub struct FetchResponse { pub status: StatusCode, pub headers: HeaderMap, pub url: Url, pub hops: Vec<Hop>, pub limit_reached: bool, pub body: Bytes }`, `pub struct Hop { pub url: Url, pub status: StatusCode, pub location: String }`. The body is read up to `body_cap`; beyond it, `Err(FetchError::BodyTooLarge)`.
- `pub enum FetchError { Blocked { url: Url }, InvalidUrl(String), Scheme(Url), TooManyRedirects, BodyTooLarge, Http(reqwest::Error) }` (thiserror). The `Display` of `Blocked` names neither the resolved address nor any response.
- `pub mod fetch;` in `lib.rs` behind `#[cfg(feature = "fetch")]`.

### Amendments after the phase review (repaired in Phase 2)

- Location handling as reqwest/tower-http today: decode the header as UTF-8 (bytes ≥ 0x80 allowed), join against the current URL; a Location that cannot be decoded or joined ends the chain and returns that 3xx as the final response, not an error. (Superseded by the redesign: tower-http returns a 3xx whose Location is missing, undecodable or unjoinable before the redirect policy runs, so such a response comes back with `limit_reached == false` even at the limit, as reqwest does today. Phase 3's MTA-STS port tests `status.is_redirection()`, not `limit_reached`.)
- Body: new fields `read_body: bool` (default true) and `truncate_body: bool` (default false). Only the final response's body is read, never a 3xx body. With `truncate_body`, a body over `body_cap` is cut at the cap and `FetchResponse.body_truncated` is true; without it, `BodyTooLarge` as before. With `read_body` false, `body` is empty.
- `FetchError::Blocked { url, reason: BlockReason, hop: usize, hops: Vec<Hop> }`: `BlockReason::{NoAddress, Disallowed}`, `hop` 0 for the initial URL and n for the n-th redirect target, `hops` the redirects followed before the refusal. `Display` stays neutral.
- `timeout` is one deadline for the whole chain, DNS resolution included (each `resolve` call wrapped in `tokio::time::timeout` with the remaining budget, each hop's client timeout set to the remaining budget). Expiry → `FetchError::Timeout`.
- Redirect method semantics exactly as reqwest 0.13: 301/302 turn only POST into GET without body; 303 turns every method except HEAD into GET without body; 307/308 keep method and body. On a conversion the request headers `Content-Type`, `Content-Length`, `Content-Encoding` and `Transfer-Encoding` are dropped. Request headers move from `base` defaults to a new field `headers: HeaderMap` (default empty), so the helper can drop them.
- `Hop.url` and `FetchResponse.url` carry no userinfo; `Hop.location` is the joined absolute URL, as spectra records today.

### Redesign after the second pass (operator, 2026-10-08)

The hand-written redirect loop kept diverging from reqwest (3xx body read, Location-before-limit, fragments, Uri conversion, Referer, sensitive headers). The helper is rebuilt on reqwest's own redirect handling; the public API stays except the resolver parameter.

- Resolver: `pub trait Resolve: Send + Sync + 'static { fn resolve(&self, host: &str) -> futures-free BoxFuture<'static, Vec<IpAddr>>; }` (a `Pin<Box<dyn Future<Output = Vec<IpAddr>> + Send>>`), and `fetch(base, resolver: Arc<dyn Resolve>, url, opts)`. The trait must be object-safe because reqwest needs `'static`.
- One client per `fetch` call: `base()` plus `.no_proxy()`, `.timeout(opts.timeout)` (reqwest's total timeout: connect, DNS, redirects and body), `.dns_resolver(Checked { inner: resolver, allow, state })` and `.redirect(Policy::custom(..))`.
- `Checked` implements `reqwest::dns::Resolve`: resolves through the inner resolver, refuses an empty set or any address failing `allow` with a typed error, and otherwise returns exactly the checked addresses. hyper connects only to what the resolver returned, so the connection is pinned to the checked set by construction, on every hop, rebinding included.
- IP-literal hosts never reach a resolver (hyper parses them). They are checked before the first request and in the redirect policy for every hop, against `allow`.
- Redirect policy: records each followed hop (`url` without userinfo, `status`, `location` = `attempt.url()` without userinfo, as spectra records today); refuses an IP-literal hop that fails `allow`; at `hops.len() >= max_redirects` returns `attempt.stop()` (`ReturnLast`, with the limit flag) or `attempt.error(..)` mapped to `TooManyRedirects` (`Fail`); enforces `https_only` per hop. Everything else (Location decoding and joining, method rules, header stripping, Referer) is reqwest's.
- `Blocked { url, reason, hop, hops }`: from the typed error in the reqwest error's source chain; `hop` is the number of hops recorded when the refusal happened, `hops` the recorded list.
- Body: only for a non-3xx final response, as before (`read_body`, `truncate_body`, `body_cap`); a 3xx final response is returned without reading its body.
- Request headers from `opts.headers` go on the request, so reqwest's cross-host stripping applies.
- Tests: the harness changes from `&stub` to `Arc::new(stub)` and the stub's `resolve` returns a boxed future (`ADLC-Test-Change`, harness only, no assertion changes). New cases: a final 3xx with a stalled body is returned at once; Referer is sent on a followed hop; Authorization is dropped on a cross-host redirect; a Location with a fragment records the hop without it.

### Fixes after the redesign review

- `base: impl Fn() -> ClientBuilder` is replaced by a typed `pub struct ClientSettings { pub user_agent: Option<String>, pub accept_invalid_certs: bool, pub root_certificates: Vec<reqwest::Certificate>, pub default_headers: HeaderMap }` (`Default`), applied by the helper itself. A caller can no longer hand in `resolve`/`resolve_to_addrs` overrides, `unix_socket` or a proxy, each of which bypasses `Checked` (review probe: both returned Ok from a refused address). `fetch(settings: &ClientSettings, resolver, url, opts)`. Phase 3's spectra port drops its `.resolve()` pin in favour of `Checked`.
- The initial URL's fragment is dropped (`set_fragment(None)`), so `hops[0].url` and `Blocked.url` match reqwest's `attempt.previous()` as spectra records today.
- New tests: a redirect to an IP literal that `allow` refuses (`http://[::1]:p/` under an IPv4-only allow) → `Blocked`, hop 1, 0 connections; `https_only` refuses an https→http hop with `Scheme` (TLS listener, `accept_invalid_certs`); an env proxy (`HTTP_PROXY` pointing at a counting listener, in its own test binary) sees 0 connections; a fragment on the initial URL is not recorded.
- Known and kept (reqwest parity): `opts.headers` and userinfo Basic auth are stripped only on the hop that changes host; a later same-host hop gets them again, as reqwest does today.

### Notes for Phase 3

- MTA-STS: a 3xx is detected with `status.is_redirection()`, not `limit_reached` (see above).
- Callers never put a `FetchError`'s `Display` or its URL into a check detail: the initial-URL `Scheme` error keeps the caller's userinfo, and `Http` errors carry reqwest's text.

## Phase 3 — Callers

### Seams

Read-only pass on `/Users/lukas/Documents/src/netray`, branch `outbound-fetch/p1`. No Phase 3 failing tests exist yet. All C1–C16 are `open` in `specs/features/outbound-fetch/report.md`.

**One blocker for the MTA-STS table.** `fetch::Resolve` returns `Vec<IpAddr>`, and `Checked` builds `SocketAddr(ip, 0)` (`crates/common/src/fetch.rs:223-224`). The policy URL has no port, so every fetch goes to :443. The table currently reaches its listener only because of reqwest's `resolve` override, which keeps a non-zero port (`mta_sts_results_table.rs:159-165`). Recommended fix: `check_mta_sts` takes the policy URL through a delegate (below). Changing the helper's `Resolve` to return ports would reopen the Phase 2 API.

**beacon**
- `crates/beacon/src/dns/mod.rs`, new: `pub struct FetchResolver<R>(pub Arc<R>);` with `impl<R: DnsLookup + 'static> netray_common::fetch::Resolve for FetchResolver<R>`.
  - The body is `let r = self.0.clone(); let h = host.to_owned(); Box::pin(async move { r.lookup_ips(&h).await })`.
  - It is `'static` because it owns an `Arc<R>`. Production shares the existing `Arc<DnsResolver>` (`AppState.dns_resolver`, `state.rs:16`).
  - `TestDnsResolver` is not `Clone` (Mutex/atomics, `test_support.rs:17-32`). Tests therefore build `Arc<TestDnsResolver>`, pass `&*arc` as the `DnsLookup`, and pass `FetchResolver(arc.clone())` to the fetch.
- `crates/beacon/src/state.rs`, new `#[derive(Clone)] pub struct OutboundFetch`:
  - fields: `settings: fetch::ClientSettings`, `resolver: Arc<dyn fetch::Resolve>`, `allow: fn(IpAddr) -> bool`, `timeout: Duration`;
  - `pub fn new(timeout_ms: u64, resolver: Arc<dyn fetch::Resolve>) -> Self` sets user agent `beacon/{ver} (netray.info)` and `allow = is_allowed_target`;
  - tests override fields with struct-update syntax: `allow: |ip| ip == IpAddr::V4(Ipv4Addr::LOCALHOST)`, `accept_invalid_certs` or `root_certificates`.
  - `AppState` replaces `http_client` and `http_client_follow` with `pub fetch: OutboundFetch`.
- New signatures:
  - `check_bimi(domain, resolver: &impl DnsLookup, fetch: &OutboundFetch)`
  - `check_mta_sts(domain, resolver: &impl DnsLookup, fetch: &OutboundFetch)`, which formats the URL and delegates to `pub(crate) async fn check_mta_sts_at(domain, resolver, fetch, policy_url: &str)`
  - `run_all_checks` and `run_inspection_inner`: the two `reqwest::Client` parameters become one `fetch: OutboundFetch`.
- Existing beacon test call sites that change (each needs `ADLC-Test-Change`):
  - `checks/mta_sts.rs`: the `client()` helper at :384-389 goes. The three tests at :421, :449 and :480 call the new `evaluate_policy` with a hand-built status, headers and body instead of an http axum server, because HTTPS-only makes plain http fail. `ssrf_hostname_blocked` at :504 changes its call.
  - `checks/mta_sts_results_table.rs:156-174`
  - `checks/bimi_results_table.rs:385-400`, plus `Row.client_hosts` and the constants
  - `checks/mod.rs:745-755` (`run_all_checks_timeout_yields_grade_skipped`)
  - `AppState::with_overrides` keeps its signature, so the routes tests do not change.

**spectra**
- `crates/spectra/src/inspect/request.rs`, new `#[derive(Clone)] pub struct Outbound { pub resolver: Arc<dyn fetch::Resolve>, pub allow: fn(IpAddr) -> bool }`, with `Outbound::system()` = `SystemResolver` + `is_allowed_target`.
- `execute_request(outbound: &Outbound, url, resolved_addr, max_redirects, timeout, user_agent, cors_origin)`. The parameter `client: &reqwest::Client` is unused today (`let _ = client;`, :112) and is replaced, so the arity stays at 7.
- `inspect(url, resolved_addr, config, outbound: &Outbound)`. `AppState.http_client: Arc<reqwest::Client>` becomes `outbound: Outbound`.
- Recommended: keep `resolved_addr`. A private `Pinned { host, ip, inner }` resolver answers the initial *domain* host with `[resolved_addr.ip()]` and passes everything else to `inner`.
  - It replaces `.resolve(&host, resolved_addr)` (:95) and still goes through `Checked`, so the Phase 2 note's intent (no bypass) holds.
  - It keeps today's connected IP equal to the validated, reported and enriched one, does one lookup, and allows no hop-0 refusal.
  - Alternative: drop the pin and `resolved_addr`. That re-resolves the initial host (the connected IP can differ from the enriched one, and a rebind gives a hop-0 "blocked"), and table rows 1–2 must then move to stub names.
- Production change in common: `FetchResponse` has no HTTP version, and spectra reports `http_version`. Add `pub version: reqwest::Version` (`fetch.rs:131-144`, set from `response.version()` near :280). Nothing builds a `FetchResponse` literal, so no test breaks.
- Existing spectra test call sites that change (each needs `ADLC-Test-Change`):
  - `tests/redirect_results_table.rs:87-99` (`run()`)
  - `src/inspect/mod.rs` tests at :357-359, :389-392, :421-424, :450 and :459 (`&client` becomes a test `Outbound` admitting 127.0.0.1)
  - `src/inspect/request.rs:218-227`

**tlsight**
- `crates/tlsight/src/tls/ocsp.rs`, new `pub async fn check_live_ocsp_with(resolver: Arc<dyn fetch::Resolve>, allow: fn(IpAddr) -> bool, ocsp_url, leaf_der, issuer_der)`. `check_live_ocsp` keeps its signature and delegates with `Arc::new(SystemResolver)` and `is_allowed_target`.
- `ClientSettings` stays default: no user agent, as `Client::new()` today, and plain http needs no certificates.
- The caller at `routes.rs:1006` does not change, and no test calls `check_live_ocsp` today.
- Tests need a valid DER leaf/issuer pair (rcgen is already a dev-dependency), otherwise `build_ocsp_request` returns `unknown` before any fetch. Write `body: Some(req_bytes.into())`, so tlsight needs no `bytes` dependency.

### Mapping

**MTA-STS** (`fetch_and_parse_policy`, `mta_sts.rs:124-366`)
- Options: GET, `max_redirects` 0, `ReturnLast`, `https_only`, `body_cap` = `MTA_STS_MAX_BODY_BYTES`, `truncate_body` true, `timeout`/`allow` from `OutboundFetch`.
- Pre-flight (`mta_sts.rs:87-111`): keep, but check with `fetch.allow` instead of `is_allowed_target`. Otherwise 127.0.0.1, the stand-in for "public", is `ssrf_blocked`. This keeps `ssrf_blocked` Fail plus "MTA-STS fetch blocked". Dropping it and mapping `Disallowed` to `ssrf_blocked` would contradict R3's wording "refused … gives https_fetch_failed".
- `Err(Blocked{..})` (any reason, always hop 0) → `https_fetch_failed` Fail, detail "policy host not reachable", result detail "MTA-STS fetch failed", metric kind `ssrf_blocked`.
- Other `Err` → `https_fetch_failed` Fail with a fixed detail such as "failed to fetch policy" (optionally ": timeout" / ": connection failed"), never `{e}`. Metric from `classify_fetch_error`: `Timeout`→timeout, `Http(e)`→ the old `classify_reqwest_error(e)`, else other.
- `Ok(r)`:
  - `r.status.is_redirection()` → `https_redirect` Fail (unchanged; this test, not `limit_reached`);
  - otherwise a non-success status → `https_fetch_failed` "HTTP {status}" (an allowed target, so the status may be shown);
  - otherwise `wrong_content_type` from `r.headers`, `policy_body_too_large` Warn when `r.body_truncated`, then the existing UTF-8/parse logic. Split this into `evaluate_policy(status, &headers, body: &[u8], truncated, dns_id)` so the unit tests need no network.

**BIMI** (`bimi.rs:81-179`)
- The scheme guard (`logo_http_not_allowed`) stays.
- Options: HEAD, `max_redirects` 4 with `Fail`. This matches today's `previous().len() >= 5`: 4 redirects are followed and the 5th errors.
- Options also set `https_only`, `read_body` false, `timeout`/`allow` from `fetch`.
- `initial_is_name = matches!(parsed.host(), Some(url::Host::Domain(_)))`, using the URL the scheme guard already parsed.

| Result | Sub-check, verdict | Detail |
|---|---|---|
| `Ok`, success | `logo_reachable` Pass | "logo reachable at {l=}" (unchanged) |
| `Ok`, other status | `logo_unreachable` Warn | "logo unreachable: HTTP {s}" (allowed target) |
| `Blocked{hop ≥ 1}`, any reason | `logo_redirect_ssrf_blocked` Fail | existing detail |
| `Blocked{hop 0, Disallowed}` and `initial_is_name` | `logo_ssrf_blocked` Fail | existing detail |
| `Blocked{hop 0, ..}` otherwise (NoAddress, refused literal) | `logo_unreachable` Warn | "logo host not reachable" |
| `TooManyRedirects` / `Scheme` / `Timeout` / `Http` / `InvalidUrl` / `BodyTooLarge` | `logo_unreachable` Warn | a fixed text per variant |

**spectra**
- Options: GET, `max_redirects` = config, `ReturnLast`, http allowed, `read_body` false, `timeout`, `accept_invalid_certs` true.
- `default_headers` = today's Accept-Encoding + User-Agent + optional Origin (they are default headers today, so reqwest's stripping behaves the same).
- `Ok(r)` → `TaskResult { final_url: r.url, status, http_version: format_http_version(r.version), headers, redirects: r.hops → RedirectHop{url, status, location: Some(location), http_version: ""}, redirect_limit_reached: r.limit_reached, error: None }`.
- `Err(Blocked{hops,..})` → today's blocked shape (initial `final_url`, status 0, empty headers, limit false). `redirects` = `hops` mapped, which includes the refused redirect. Today's literal guard left it out, but today's row 3 recorded it, so its hop list stays. `error: Some("Redirect destination blocked")` without the `: host:port` suffix.
- Other `Err` → `"Request failed: {e}"` as today. It reaches only logs (`inspect/mod.rs:108-119`), not a check.

**OCSP**
- Options: POST, body, `headers` {Content-Type: application/ocsp-request} (dropped on 301/302/303 by reqwest), `max_redirects` 10, `Fail`, http allowed, 64 KiB, `read_body`, timeout 3 s, `allow`.
- `Ok` with 200 → `OcspResponse::from_der(&r.body)` as today. Other `Ok` → `unknown`.
- `Err(Blocked{..})` → `unknown` with `reason: Some("blocked")`. Any other `Err` (including `BodyTooLarge`) → `unknown`.

### Results-table test changes

Harness for all tables: one `Arc<TestDnsResolver>` (beacon) or stub (spectra) serves both TXT and fetch resolution; `allow` admits exactly 127.0.0.1; "public" stub addresses become 127.0.0.1; resolve maps and `client_hosts` go.

**BIMI** (`bimi_results_table.rs`): `PUBLIC_IP` becomes "127.0.0.1", `accept_invalid_certs` true.

| Row | Class | Expected (sub-check, verdict, grade) |
|---|---|---|
| 1 refuses connections | harness-only | `logo_unreachable` Warn B |
| 2 NXDOMAIN (stub `[]`) | refused (NoAddress), unchanged | `logo_unreachable` Warn B |
| 3 → 10.0.0.1 | refused (name, hop 0), unchanged | `logo_ssrf_blocked` Fail D |
| 4a final → 10.0.0.1, 200 | refused (hop 1), unchanged | `logo_redirect_ssrf_blocked` Fail D |
| 4b final → 10.0.0.1, 404 | **changed** | `logo_unreachable` Warn B → `logo_redirect_ssrf_blocked` Fail D |
| 5 intermediate → 10.0.0.1 | **changed** | `logo_reachable` Pass A → `logo_redirect_ssrf_blocked` Fail D |
| 6a 4 redirects | harness-only | `logo_reachable` Pass A |
| 6b 5 redirects | harness-only | `logo_unreachable` Warn B |
| 7a literal, URL → `https://127.0.0.2:{p}/l.svg` | **changed** | → `logo_unreachable` Warn B |
| 7b userinfo, URL → `https://x@127.0.0.2:{p}/l.svg` | **changed** | → `logo_unreachable` Warn B |
| 7c `[::1]` (URL unchanged) | **changed** | → `logo_unreachable` Warn B |
| 8 stub `[]`, client resolved | **changed** | `logo_reachable` Pass A → `logo_unreachable` Warn B |
| 9 redirect to literal; Location → `https://10.1.2.3/l.svg` (the spec's original) | **changed** | → `logo_redirect_ssrf_blocked` Fail D |

**MTA-STS** (`mta_sts_results_table.rs`): call `check_mta_sts_at` with `https://mta-sts.example.com:{port}/.well-known/mta-sts.txt`; root certificate via `ClientSettings.root_certificates`.

| Row | Class | Expected |
|---|---|---|
| 1 stub `[]` | **changed** | `mode` Pass A → `https_fetch_failed` Fail D |
| 2 10.0.0.1 | harness-only | `ssrf_blocked` Fail D |
| 3 public → 127.0.0.1 | harness-only | `mode` Pass A |
| 4 301 (stub 127.0.0.1) | harness-only | `https_redirect` Fail D |

**spectra** (`tests/redirect_results_table.rs`)

| Row | Class | Expected |
|---|---|---|
| 1 three redirects | harness-only (`run()`); `localhost` stays because it is pinned to 127.0.0.1 (move to stub names only if the pin is dropped) | unchanged |
| 2 loop at 10 | harness-only | unchanged |
| 3 302 → `http://localhost:{p2}/`, stub `localhost` → `[::1]` | **changed** | see below |
| 4 new (C15) 302 → `http://mixed.test:{p2}/`, stub `[127.0.0.1, 10.0.0.1]` | added | error "Redirect destination blocked", status 0, hits2 0, hops `[first→target 302]` |

Row 3's new expectation: error "Redirect destination blocked", status 0, `final_url` = first, limit false, hits2 0, no `x-second-listener` header, hops `[first→target 302]` (same as today), `redirect_limit` check absent.

The Phase 3 scenario tests (C6–C15) are separate tests, not table rows:
- C8 and C12 use the production `allow`, so the literal 127.0.0.1 is refused and a counting listener on :p sees 0.
- C9, C10, C13 and C15 use the 127.0.0.1-only `allow` with refused targets on `[::1]`/stub names and dual-stack counting listeners.
- C7 uses `check_mta_sts_at("pinned.invalid", …)` with the certificate issued for `mta-sts.pinned.invalid`.

### Groups

G1: C4, C12, C13 · G2: C1, C2, C6, C7, C8, C9, C14 · G3: C3, C10, C11, C15 · G4: C5, C16

- G1–G3 touch disjoint production files.
- G3's change to `crates/common/src/fetch.rs` adds a field only; nothing in G1 or G2 builds a `FetchResponse`, so the groups are independent.
- G4 touches no production file and closes after G1–G3. Its table edits ride in G2's and G3's failing-test commits, with `ADLC-Test-Change: <path> -- requirement 8: refused-target rows` for changed rows and `-- harness: helper seam` for harness-only edits.

### Plan

### G1
1. `crates/tlsight/src/tls/ocsp.rs` — change — add `check_live_ocsp_with(resolver, allow, …)`. Replace the `reqwest::Client::new().post(...)` block (:48-67) with `fetch::fetch(&ClientSettings::default(), resolver, ocsp_url, &opts)` and map `Blocked` → `unknown` + `reason: Some("blocked")`. `check_live_ocsp` (:29) becomes the wrapper. Update the `reason` doc at :17, which says "only set when status == revoked". — hangs on `check_live_ocsp`, ocsp.rs:29.

### G2
1. `crates/beacon/src/dns/mod.rs` — change — add `FetchResolver<R>` — hangs on the `DnsLookup` trait, dns/mod.rs:18 (`lookup_ips`, :21).
2. `crates/beacon/src/state.rs` — change — add `OutboundFetch` plus `new`; delete `http_client_builder` (:25-33) and `http_client_follow_builder` (:35-53). The `AppState` fields at :18-19 become `fetch`. Rebuild construction in `AppState::new` (:77-83, :93-94; wrap `dns_resolver` in an `Arc` first) and in `with_overrides` (:122-123) — hangs on `AppState::new`, state.rs:56, and `with_overrides`, :110.
3. `crates/beacon/src/checks/mta_sts.rs` — change:
   - `check_mta_sts` (:49) delegates to the new `check_mta_sts_at`, and the pre-flight (:90-91) uses `fetch.allow`;
   - in `fetch_and_parse_policy` (:124), `http_client.get().send()` (:132) and the chunk loop (:205-240) become `fetch::fetch` plus the mapping above, and the evaluation part splits into `evaluate_policy`;
   - `classify_reqwest_error` (:27) becomes `classify_fetch_error`, and the module doc at :12-16 is updated.
   - hangs on `check_mta_sts` :49 and `fetch_and_parse_policy` :124.
4. `crates/beacon/src/checks/bimi.rs` — change:
   - delete the pre-flight lookup (:101-121), the post-hoc redirect check (:127-153) and `extract_host` (:204-215);
   - `http_client_follow.head(url).send()` (:125) becomes `fetch::fetch` with the BIMI mapping;
   - the signature at :49-53 changes; the module doc at :10-15 and `classify_reqwest_error` at :27 are updated.
   - hangs on `check_bimi`, bimi.rs:49.
5. `crates/beacon/src/checks/mod.rs` — change — the two client parameters of `run_all_checks` (:69-70, :82-83) and `run_inspection_inner` (:215-216) become `fetch: OutboundFetch`. The BIMI spawn (:352-356) and the MTA-STS spawn (:482-486) clone it — hangs on `run_all_checks` :63 and `run_inspection_inner` :209.
6. `crates/beacon/src/routes.rs` — change — :219-220 become `state.fetch.clone()` — hangs on the inspect handler's `checks::run_all_checks` call, routes.rs:213.

### G3
1. `crates/common/src/fetch.rs` — change — add `pub version: reqwest::Version` to `FetchResponse` (:131-144), filled from `response.version()` in `fetch` (:280-308) — hangs on `fetch`, fetch.rs:236.
2. `crates/spectra/src/inspect/request.rs` — change:
   - add `Outbound` and a private `Pinned` resolver;
   - delete the redirect closure plus the `ssrf_blocked` state (:25-77), the per-request client build (:79-111), `let _ = client` (:112) and the two blocked branches (:114-175);
   - call `fetch::fetch` and use the spectra mapping above.
   - hangs on `execute_request`, request.rs:16.
3. `crates/spectra/src/inspect/mod.rs` — change — the `inspect` parameter `client` becomes `outbound` (:41-46), passed to the three `execute_request` calls (:56, :76, :89) — hangs on `inspect`, mod.rs:42.
4. `crates/spectra/src/state.rs` — change — `http_client` (:14, :29-35, :42) becomes `outbound: Outbound::system()` — hangs on `AppState::new`, state.rs:18.
5. `crates/spectra/src/routes.rs` — change — :352 becomes `&state.outbound` — hangs on `do_inspect_inner`, routes.rs:321.

### G4
No production file. Run the three tables and confirm that only the rows marked changed or added above differ.

**Where the spec is silent** (decide or record in the report):
1. The MTA-STS policy port seam (`check_mta_sts_at`).
2. Keeping the MTA-STS pre-flight versus folding it into `Blocked{Disallowed}`.
3. The fixed detail texts for transport errors on allowed targets.
4. MTA-STS body-read failures merge into "fetch failed": the "MTA-STS body read failed" text goes, and `wrong_content_type` no longer comes before a body-read failure.
5. spectra loses the hops on a non-`Blocked` error mid-chain, because `Http`/`Timeout` carry no hops. That differs from today for an allowed target.
6. spectra's `Blocked.hops` includes the refused redirect, and the error text loses its `host:port`.
7. The spectra pin versus re-resolving the initial host.
8. The tlsight frontend shows `reason` only for `revoked` (`TlsParams.tsx:84`), so "blocked" is not displayed.
9. Metric kinds for refused targets.
10. A refused hop ≥ 1 with NoAddress still says "redirects to private address".
11. beacon's fetches now resolve through its configured DNS servers rather than getaddrinfo.
12. tlsight's OCSP request no longer honours environment proxies, and the 64 KB cap is new.


### Decisions on the points the spec leaves open (orchestrator, 2026-10-08)

1. `check_mta_sts_at(domain, resolver, fetch, policy_url)` is the test seam for the policy port.
2. The MTA-STS pre-flight stays, checked with `fetch.allow`, so `ssrf_blocked` Fail keeps its meaning; any `Blocked` from the helper then maps to `https_fetch_failed` "policy host not reachable".
3. Fixed detail texts for transport errors as listed; never a `FetchError` Display.
4. Accepted: body-read failures merge into "failed to fetch policy".
5. Accepted and recorded: spectra loses hops on a mid-chain `Http`/`Timeout` error (allowed targets only; logs, not checks).
6. Accepted: `Blocked.hops` includes the refused redirect (as today's row 3), error text "Redirect destination blocked" without `host:port`.
7. spectra keeps the pin: a private `Pinned` resolver answers the initial host with the validated address and delegates the rest to `Checked`'s inner resolver.
8. Accepted: the tlsight frontend shows `reason` only for `revoked`.
9. Metric kind for a refused target: `blocked`.
10. A refused hop ≥ 1 keeps the existing `logo_redirect_ssrf_blocked` detail text.
11. Intended: beacon's fetches resolve through its configured resolvers (SC13 refusal rule).
12. Accepted: OCSP ignores environment proxies; the 64 KB cap is new.

### Fixes after the Phase 3 review

- Helper: `pub async fn fetch_traced(..same args..) -> (Result<FetchResponse, FetchError>, Vec<Hop>)` returns the hops followed before any outcome; `fetch` delegates to it. spectra uses it, so a mid-chain `Http`/`Timeout` keeps today's hop list (it feeds `redirects_to_https`, which lens scores). Decision 5 is withdrawn.
- spectra on `Blocked`: the refused redirect is not recorded (drop the last hop when `hop >= 1`), as today's literal guard stopped before recording. Decision 6 is corrected; table row 3's hop expectation becomes `[]` (`ADLC-Test-Change`, requirement 8).
- spectra port: `Pinned` answers the IP only, so each hop uses its URL's port. Today's `.resolve(host, resolved_addr)` carried port 80 to a same-host `https://` hop, a TLS handshake to :80 that always failed. Accepted as a stated fix: lens's score reads only `redirects_to_https`, which is true either way; spectra's upgrade `status_code`, `same_host` and message text can change.
- G5, prism (requirement 9): `check_mta_sts` in `crates/mhost-prism/src/api/check.rs:1277` takes an outbound seam (`Arc<dyn Resolve>`, `allow`) instead of `&reqwest::Client`; production uses `SystemResolver` and `is_allowed_target`; `AppState.http_client` and its builder (`lib.rs:94`, `api/mod.rs:86,269,579`) go. Mapping: `Ok` as today; `Blocked`, `TooManyRedirects`, `Http`, `BodyTooLarge` → Warning "MTA-STS policy file unreachable" (fixed text); `Timeout` → Warning "MTA-STS policy file fetch timed out". Body read with `body_cap` 64 KiB; non-UTF-8 → today's "unreadable" Warning with fixed text.
