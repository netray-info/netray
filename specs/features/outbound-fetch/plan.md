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
