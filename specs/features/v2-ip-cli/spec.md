# Spec: the plain-text and structured surface of ip. (theme G, backend half)

Status: Ready for Implementation
Created: 2026-10-19

## Goal

The V1 command-line surface of `ip.` works again, built in `netray-web` on the `netray_ip::IpLookup`
seam: the `/api/v1/ip*` endpoints take `?ip=`, `?dns=`, `?lang=`, `?fields=` and `?format=`, answer
YAML, TOML, CSV and per-field JSON, look up reverse DNS for an address, and carry the V1 response
metadata headers; one entry point, `netray_web::api::ip_host`, serves the V1 path family of `ip.`
(`/`, `/json`, `/country`, `/asn`, `/all`, `/ip/cidr`, `/<field>/<fmt>` ...) for the cutover to mount.

Closes inventory rows (private `specs/netray/inventory/sections/06-ip.md`, `07-site-api.md`):
6.6, 6.26, 6.27, 6.29, 6.31, 6.33, 6.34, 6.35, 6.41, 6.42, 6.43, 6.48, 7.95, 7.96, and the backend
half of 6.5 and 6.51.

## Non-goals

- No template, stylesheet or island change: the IP page (6.1-6.22, the attribution's placement,
  the hostname row of 6.5) is the UI half, built later in lane A.
- No change to the domain IP module's run or its evidence (`ip.addresses` keeps its columns and no
  reverse DNS), so no lens or domain golden moves.
- No `ip.` host routing on main: the host dispatch exists only on `feature/v2-cutover`. This feature
  ships the entry point and states how the cutover mounts it.
- `/range`, `/batch`, `/diff`, `/ipv4`, `/ipv6`, `/meta` and `/asn/<n>` on `ip.` stay `410` (their
  successors under `/api/v1` are correct); YAML/TOML/CSV for batch and diff stay out.

## Context and constraints

- V1 reference: `crates/ip/src/routes.rs` (`dispatch`, `dispatch_all`, `ip_cidr_handler`),
  `negotiate.rs`, `format.rs`, `handlers.rs`, `middleware.rs` (`rate_limit`, `geoip_date_headers`,
  `etag_last_modified`). The renderers in `handlers.rs` and `format.rs` are reused, not copied.
- V2 today: `crates/web/src/api/ip.rs`; `IpModule::lookup` passes `skip_dns = true` and `lang = None`
  (`crates/ip/src/module.rs`); `/api/v1/ip` ignores `?ip=`; `?field=headers` answers 503 with the IP
  module off because the seam check runs first.
- Cutover (`feature/v2-cutover`, read only here): `crates/web/src/host/mod.rs` routes `ip.` through
  `legacy.ip_successor()` (410 table, `host/legacy.rs`), then `subdomain_common`, `to_apex`, then the
  inspector router whose `/` is `pages::inspect::ip_root` (caller's address as text for a CLI agent).
  The 410 table names wrong successors for `/asn` and `/ip/cidr` (6.33, 6.35).
- API v1 changes are additive (SDD §7.1); `tests/fixtures/contracts/api-v1.openapi.json` may only grow
  compatibly.
- SDD S23 (2026-10-08) ended YAML, TOML and CSV; the operator's theme decision of 2026-10-19
  ("G IP page and CLI: rebuild", private `v2-parity.md`) reverses that for the ip surface. The SDD is
  amended in the same commit (S40, §7.1, §7.2).
- No GeoIP data locally: every test runs on `netray_ip::testing::StubIpLookup`, which this feature
  extends to show reverse DNS, language and a GeoIP build time.

### How the cutover mounts `ip_host`

In `host/mod.rs`, the `Some("ip")` arm calls `netray_web::api::ip_host(&api_state, req).await` first,
with the same `ApiState` clone `/api/v1` is built over (a fresh state would bring fresh limiters).
`Ok(response)` is the answer. `Err(req)` hands the request back unchanged; the arm rebuilds its
`Legacy` view from it and continues as today: `ip_successor`, `subdomain_common`, `to_apex`, the
inspector. `ip_successor` stays as it is: `ip_host` answers GET and HEAD on its paths before the
table, and the table still answers what comes back (other methods, `/ip/<other>`, `/<field>/`,
`/asn/<n>`, `/range`, `/batch`, `/diff`, `/ipv4`, `/ipv6`, `/meta`). Its wrong successors for
`/asn` and `/ip/cidr` remain only for methods other than GET and HEAD. `pages::inspect::ip_root`'s
text branch becomes unreachable, because `ip_host` answers every GET or HEAD on `/` a CLI agent
sends; the root then needs only `render(Tool::Ip, uri)`.

## Requirements

1. `netray_ip::IpLookup` gains `lookup_with(ip, LookupOptions)` and
   `lookup_caller_with(remote, user_agent, LookupOptions)`, where `LookupOptions` carries
   `reverse_dns: bool` and `lang: Option<String>`, plus `geoip_build_epoch() -> Option<u64>`. Each has a
   default that keeps today's behaviour (`lookup`, `lookup_caller`, `None`). `IpModule` implements them
   over `enrich(.., skip_dns = !reverse_dns, lang)` and the City database's build epoch; its domain run
   keeps `lookup` (no reverse DNS).
2. `GET /api/v1/ip/{address}` looks up reverse DNS by default. `?dns=` with the value `true` or `1`
   (any case) keeps it on, and any other value turns it off. `host` becomes an address field
   (`?field=host`).
3. `GET /api/v1/ip?ip=X` answers for X as `GET /api/v1/ip/X` does: the same address validation,
   the same per-target charge, reverse DNS by default. It never answers the caller when `ip` is
   given. A field of the caller's connection (`tcp`, `user_agent`) with `?ip=` answers 400 naming
   the field. `headers` (the request's) and `all` (X's lookup with the request's headers) are
   answered, which `/ip/X` refuses as caller fields. `cidr` and `headers` with `?ip=` answer with
   the IP module off, as requirement 8 has them answer for the caller.
4. `?lang=<tag>` on `/api/v1/ip` and `/api/v1/ip/{address}` passes the tag to the lookup. Structured
   answers then carry `city_localized` and `country_localized` where the database has the locale, as
   V1 did. A tag that is empty, longer than 16 characters, or holds characters other than ASCII letters,
   digits and `-` answers 400.
5. `?format=` on `/api/v1/ip` and `/api/v1/ip/{address}` takes `json`, `yaml`, `toml`, `csv` or `plain`.
   It renders the whole document, or the `?field=` value, in that form with V1's content types
   (`application/json`, `application/yaml`, `application/toml`, `text/csv`, `text/plain`) and V1's
   serialisers (`netray_ip::format::OutputFormat`, `handlers::<field>::to_json`/`to_plain`). Without
   `?format=`, an `Accept` header naming `application/yaml`, `application/toml` or `text/csv` picks
   that form; any other request keeps today's default (the document as JSON, a field as plain text).
   An unknown format answers 400. A field with no form in the asked format (a scalar as TOML) answers
   400 naming field and format.
6. `?fields=a,b` keeps only the named top-level keys of a structured answer (the document, an object
   field, `all`); it has no effect on plain text. Unknown names are dropped silently, as in V1.
7. `cidr` is a field on both `/api/v1/ip` and `/api/v1/ip/{address}`. It answers `<address>/32` for
   IPv4 and `<address>/128` for IPv6, as plain text in any format. `all` is a caller field. Its plain
   form is V1's `/all` text: `handlers::all::to_plain` followed by the filtered request headers. Its
   structured form is the document with a top-level `headers` object.
8. With the IP module off, `GET /api/v1/ip` answers `?field=headers`, `?field=cidr` and
   `?field=ip` in plain form for the caller instead of 503. These need no lookup. Every other answer
   that needs a lookup stays 503, and a 503 charges no rate limiter. Structured `headers` carries
   V1's `x_forwarded_for_chain` (the `X-Forwarded-For` hops, an empty list without the header).
9. `POST /api/v1/ip/batch?dns=true` (or `1`) looks up reverse DNS per address; without it, batch
   stays as it is (no reverse DNS), as V1's batch defaulted.
10. Every answer the handlers of `/api/v1/ip`, `/api/v1/ip/{address}`, `/ip/batch`, `/ip/range/*`, `/ip/diff` and
    `/asn/{n}` produce carries `X-RateLimit-Limit` (`[api.ip] per_ip_per_minute`). Every answer to a request
    the per-IP limiter charged also carries `X-RateLimit-Remaining` (the budget left after this
    request; `0` on any 429, whichever limiter refused). A 413 that lens's body limit answers
    before routing, or rewrites, is not such an answer. When the seam reports a GeoIP build
    time, each answer also carries `X-GeoIP-Database-Date` (an HTTP date) and
    `X-GeoIP-Database-Age-Days` (whole days since the build).
11. A 200 answer to a GET on those endpoints carries a strong `ETag` derived from the body bytes and,
    when the build time is known, `Last-Modified` set to the GeoIP build date. A GET whose
    `If-None-Match` equals the answer's ETag (or is `*`) answers 304 with the ETag and an empty body.
12. `netray_ip::GEOIP_ATTRIBUTION` holds the MaxMind GeoLite2 attribution text. `GET /api/v1/meta`
    carries it as `attribution`, so the UI half and API clients read it from the backend.
13. `netray_web::api::ip_host(&ApiState, Request<Body>) -> Result<Response, Request<Body>>` serves GET
    and HEAD on `ip.`'s V1 paths: `/`; `/json`, `/yaml`, `/toml`, `/csv`; `/<field>` and
    `/<field>/<fmt>` for `ip`, `tcp`, `location`, `user_agent`, `network`, `host`, `isp`, `country`,
    `city`, `asn`, `timezone`, `latitude`, `longitude`, `region`, `headers` and `all`; `/ip/cidr`.
    Every other path, any other method, and `/asn/<digits>` come back as `Err(req)` unchanged.
14. On `ip_host`'s `/`, V1's negotiation picks the form, in this order: `?format=`; a CLI agent
    (`curl`, `Wget`, `HTTPie`, `python-requests`, `python-httpx`, `HTTP Library`) sending `Accept: */*`
    or no `Accept` gets plain text; the first of `application/json`, `application/yaml`,
    `application/toml`, `text/csv`, `text/plain` and `text/html` named in `Accept` decides. When that
    picks HTML, or nothing decides, the request comes back as `Err(req)` for the inspector page. On
    the other paths a suffix decides first, then the same order, and HTML or nothing falls to plain.
    An unknown suffix answers 404 naming the formats.
15. Every `ip_host` path answers for `?ip=X` when it is given, never silently for the caller: the root,
    the format paths, every field (`/ip/cidr?ip=X` answers `X/32`) and `/all`. `headers` always shows
    the request's headers. `tcp` and `user_agent` with `?ip=` answer 400, as requirement 3 does.
    `?fields=`, `?lang=`, `?dns=` and `?format=` act as requirements 2, 4, 5 and 6 do. `ip_host`
    shares the API's limiters, refusals and metadata headers (requirements 8, 10, 11). Its plain root
    and its `ip`, `cidr` and `headers` paths answer for the caller with the IP module off.

## Phase 1 — Lookup options, formats and fields under /api/v1

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 5, 6, 7, 8, 9, 12

The seam gains its options; `crates/web/src/api/ip.rs` gains the query parameters, the formats, the
fields `host`, `cidr` and `all`, and the no-lookup answers; batch takes `?dns=`; meta carries the
attribution. `StubIpLookup` answers `hostname` `stub.example.com` when reverse DNS is on, the German
names (`Berlin`, `Deutschland`) as `city_localized`/`country_localized` for `lang` `de`, and a fixed
build epoch. The OpenAPI fixture grows (`UPDATE_GOLDEN=1`). SDD S40, §7.1 and §7.2 are amended in
the commit that changes the behaviour.

### Test Scenarios

GIVEN the stub WHEN `GET /api/v1/ip/198.51.100.1` THEN the JSON's `ip.hostname` is `stub.example.com`.
GIVEN the stub WHEN `GET /api/v1/ip/198.51.100.1?dns=false` THEN `ip.hostname` is null.
GIVEN the stub WHEN `GET /api/v1/ip/198.51.100.1?field=host` THEN the body is `stub.example.com` and a newline.
GIVEN a caller 192.0.2.10 WHEN `GET /api/v1/ip?ip=198.51.100.1` THEN `ip.addr` is `198.51.100.1`, not the caller.
GIVEN a caller WHEN `GET /api/v1/ip?ip=10.0.0.1` THEN 400 `INVALID_INPUT`.
GIVEN a caller WHEN `GET /api/v1/ip?ip=198.51.100.1&field=tcp` THEN 400 naming `tcp`.
GIVEN the stub WHEN `GET /api/v1/ip/198.51.100.1?lang=de` THEN `location.country_localized` is `Deutschland`.
GIVEN the stub WHEN `GET /api/v1/ip/198.51.100.1?lang=de%3Bx` THEN 400 `INVALID_INPUT`.
GIVEN the stub WHEN `GET /api/v1/ip/198.51.100.1?format=yaml` THEN content type `application/yaml` and the body is the document as YAML.
GIVEN the stub WHEN `GET /api/v1/ip?format=toml` and `?format=csv` THEN `application/toml` and `text/csv` bodies of the document.
GIVEN the stub WHEN `GET /api/v1/ip` with `Accept: application/yaml` THEN the answer is YAML.
GIVEN the stub WHEN `GET /api/v1/ip` with `Accept: text/plain` THEN the answer is today's JSON document.
GIVEN the stub WHEN `GET /api/v1/ip/198.51.100.1?field=location&format=json` THEN the body is `handlers::location::to_json` of the lookup.
GIVEN the stub WHEN `GET /api/v1/ip/198.51.100.1?field=country&format=toml` THEN 400 naming `country` and `toml`.
GIVEN the stub WHEN `GET /api/v1/ip?format=xml` THEN 400 `INVALID_INPUT`.
GIVEN the stub WHEN `GET /api/v1/ip?fields=ip,location&format=json` THEN the JSON object has exactly the keys `ip` and `location`.
GIVEN the stub WHEN `GET /api/v1/ip?fields=ip&field=country` THEN the plain country is answered unchanged.
GIVEN a caller 192.0.2.10 WHEN `GET /api/v1/ip?field=cidr` THEN the body is `192.0.2.10/32` and a newline.
GIVEN the stub WHEN `GET /api/v1/ip/2001:db8::1?field=cidr` with an internal-mode config THEN the body is `2001:db8::1/128` and a newline.
GIVEN a request with header `X-Test: one` WHEN `GET /api/v1/ip?field=all` THEN the body is `all::to_plain` of the lookup followed by `x-test: one`.
GIVEN a request with header `X-Test: one` WHEN `GET /api/v1/ip?field=all&format=json` THEN the document has `headers.x-test` = `one`.
GIVEN the IP module off WHEN `GET /api/v1/ip?field=headers`, `?field=cidr`, `?field=ip` THEN 200 plain answers.
GIVEN the IP module off WHEN `GET /api/v1/ip?field=country` THEN 503 `UNAVAILABLE`.
GIVEN the stub WHEN `POST /api/v1/ip/batch?dns=true` with one address THEN its `result.ip.hostname` is `stub.example.com`; without `?dns` it is null.
GIVEN any config WHEN `GET /api/v1/meta` THEN `attribution` equals `netray_ip::GEOIP_ATTRIBUTION`, which names MaxMind GeoLite2.
GIVEN an `IpLookup` implementation with only the four old methods WHEN `lookup_with` is called THEN it answers what `lookup` answers.

## Phase 2 — Response metadata headers

**Depends on:** Phase 1
**Requirements:** 10, 11

One response step on the ip routes adds the rate-limit and GeoIP headers; the per-IP limiter reports
its remaining budget; GETs get the ETag, Last-Modified and the 304.

### Test Scenarios

GIVEN `[api.ip] per_ip_per_minute = 60` WHEN `GET /api/v1/ip` THEN `X-RateLimit-Limit` is `60` and `X-RateLimit-Remaining` is one below the burst.
GIVEN a caller who spent the per-IP burst WHEN the next `GET /api/v1/ip` THEN 429 with `X-RateLimit-Remaining: 0` and `Retry-After`.
GIVEN the stub's build epoch WHEN `GET /api/v1/ip/198.51.100.1` THEN `X-GeoIP-Database-Date` is the epoch as an HTTP date and `X-GeoIP-Database-Age-Days` is the whole days since it.
GIVEN a seam without a build epoch WHEN `GET /api/v1/ip` THEN neither GeoIP header nor `Last-Modified` is present.
GIVEN the stub WHEN `GET /api/v1/ip/198.51.100.1` twice THEN both carry the same strong `ETag` and `Last-Modified` equals the GeoIP date.
GIVEN the ETag of a first answer WHEN the same GET with `If-None-Match` set to it THEN 304, the same ETag, an empty body.
GIVEN a different `If-None-Match` WHEN the GET THEN 200 with the body.
GIVEN the stub WHEN `POST /api/v1/ip/diff` and `GET /api/v1/asn/15169` THEN both carry the rate-limit headers; the POST carries no ETag.

## Phase 3 — The ip. host entry point

**Depends on:** Phase 2
**Requirements:** 13, 14, 15

`ip_host` in `crates/web/src/api/` maps the V1 paths onto the Phase 1 renderers and the Phase 2
headers.

### Test Scenarios

GIVEN `curl/8.5.0` with `Accept: */*` WHEN `ip_host` gets `/` THEN 200 `text/plain` with the caller's address and a newline.
GIVEN `curl/8.5.0` WHEN `ip_host` gets `/?ip=198.51.100.1` THEN the body is `198.51.100.1` and a newline.
GIVEN `curl/8.5.0` with `Accept: application/json` WHEN `ip_host` gets `/` THEN the document as JSON.
GIVEN a browser `Accept: text/html,*/*` WHEN `ip_host` gets `/` THEN `Err(req)` with path and headers unchanged.
GIVEN any agent WHEN `ip_host` gets `/?format=yaml` THEN YAML.
GIVEN `curl/8.5.0` WHEN `ip_host` gets `/country` THEN the stub country and a newline.
GIVEN a browser WHEN `ip_host` gets `/country` THEN plain text, not `Err`.
GIVEN any agent WHEN `ip_host` gets `/country?ip=198.51.100.1&lang=de` THEN the plain answer for 198.51.100.1.
GIVEN any agent WHEN `ip_host` gets `/location/json` THEN `handlers::location::to_json` of the caller's lookup.
GIVEN any agent WHEN `ip_host` gets `/json?fields=ip` THEN a JSON object with only `ip`.
GIVEN any agent WHEN `ip_host` gets `/yaml`, `/toml`, `/csv` THEN the document in that form.
GIVEN `curl/8.5.0` WHEN `ip_host` gets `/asn` THEN `AS64496` and a newline (the caller's ASN, not a 410).
GIVEN any agent WHEN `ip_host` gets `/asn/15169` THEN `Err(req)`.
GIVEN a caller 192.0.2.10 WHEN `ip_host` gets `/ip/cidr` THEN `192.0.2.10/32` and a newline; with `?ip=198.51.100.1` THEN `198.51.100.1/32`.
GIVEN header `X-Test: one` WHEN `ip_host` gets `/all` THEN `all::to_plain` followed by `x-test: one`.
GIVEN any agent WHEN `ip_host` gets `/headers/json` THEN the filtered request headers as a JSON object.
GIVEN any agent WHEN `ip_host` gets `/country/xml` THEN 404 naming the formats.
GIVEN any agent WHEN `ip_host` gets `/range`, `/batch`, `/meta`, `/nope` THEN `Err(req)`.
GIVEN a POST WHEN `ip_host` gets `/` THEN `Err(req)`.
GIVEN the IP module off WHEN `ip_host` gets `/` from curl, `/ip/cidr` or `/headers` THEN 200 plain; `/country` THEN 503.
GIVEN any agent WHEN `ip_host` gets `/tcp?ip=198.51.100.1` THEN 400 naming `tcp`.
GIVEN the stub WHEN `ip_host` answers `/country` THEN it carries `X-RateLimit-Limit`, the GeoIP headers and an ETag.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate for features 0–7
  (operator, 2026-10-19).
- S23's end of YAML, TOML and CSV is superseded for the ip surface by the theme decision "G IP page
  and CLI: rebuild" (operator, 2026-10-19); recorded as SDD S40 rather than halting, because the
  operator's later decision names exactly these rows (implementer under the operator's autonomy
  mandate, 2026-10-19).
- Theme I's "V1 paths stay 410" is read as the V1 API clients (`/api/*`, `/meta`, batch, diff);
  theme G names `ip.`'s field and format paths (6.29, 6.31, 6.33, 6.34) for rebuilding, so those
  answer again (implementer under the operator's autonomy mandate, 2026-10-19).
- The entry point lives in `netray-web` (`crates/web/src/api/`), not in `netray-ip`: SDD §3.1 gives
  the router, the API and the rate limits to `netray-web`, and the module crates lose their routers
  (implementer under the operator's autonomy mandate, 2026-10-19).
- `ip_host` returns `Result<Response, Request>` instead of a `Router`, so the cutover's host arm can
  fall through to its 410 table and the inspector with the untouched request (implementer under the
  operator's autonomy mandate, 2026-10-19).
- Accept negotiation under `/api/v1` is narrowed to YAML, TOML and CSV, which today wrongly get JSON.
  `text/plain` or `application/json` on a field keeps today's answer, so v1 stays additive. The full
  V1 order applies on `ip_host` (implementer under the operator's autonomy mandate, 2026-10-19).
- The ETag is derived from the body, not V1's `"<geoip epoch>-<version>"`. V1's tag was the same for
  every caller and every path, so a client whose address changed got a 304 with its old address.
  For the same reason `If-Modified-Since` yields no 304; `Last-Modified` keeps V1's GeoIP date
  (implementer under the operator's autonomy mandate, 2026-10-19).
- V1 carried the GeoLite2 attribution in no structured answer, only in the SPA footer
  (`crates/ip/frontend/src/App.tsx`; no match in `crates/ip/src`), so no body or header changes for
  it. The backend exposes it as a constant and in `/api/v1/meta` for the UI half and API clients
  (implementer under the operator's autonomy mandate, 2026-10-19).
- A field path on `ip.` that negotiates HTML answers plain text instead of handing back `Err`.
  The cutover would otherwise send `/country` to the apex, where it reads as a domain (implementer
  under the operator's autonomy mandate, 2026-10-19).
- `tcp` and `user_agent` with `?ip=` answer 400, as the API's caller-only refusal does, not V1's
  `unknown` (implementer under the operator's autonomy mandate, 2026-10-19).
- `/ip/cidr?ip=X` answers X (V1 ignored `?ip=` there): the theme's rule is that `?ip=` never
  silently answers the caller (implementer under the operator's autonomy mandate, 2026-10-19).
- The domain IP module keeps no reverse DNS and its evidence keeps its columns. A hostname column
  would move every domain golden, and the inspector's hostname display belongs to the UI half
  (implementer under the operator's autonomy mandate, 2026-10-19).

- Phase 1 review: `?ip=`, `?lang=` and `?format=` were ignored before and are validated now, so a
  malformed value answers 400 where it got the caller's document. Those answers were wrong for the
  request (the caller instead of X, no form instead of the asked one), so the stricter answer is
  kept and the v1 contract's additivity is read as the document's shape, which only grows
  (implementer under the operator's autonomy mandate, 2026-10-19).
- Phase 1 review: a 503 charges no limiter, validation runs before any charge, and the answers that
  need no lookup are charged once (implementer under the operator's autonomy mandate, 2026-10-19).
- Phase 1 review: structured `headers` keeps V1's `x_forwarded_for_chain`; `all` keeps V1's
  `headers` object without it (implementer under the operator's autonomy mandate, 2026-10-19).

- `X-RateLimit-Remaining` only on charged requests: governor reports the remaining budget only when
  it charges, and a request refused before the charge (validation, module off) costs nothing since
  Phase 1's review, so it carries the limit alone (implementer under the operator's autonomy
  mandate, 2026-10-19).

- Phase 3 review: every ip answer (API and `ip_host`) carries `Vary: Accept, User-Agent` and
  `Cache-Control: private, no-cache`. The answer depends on the caller, and `Last-Modified` alone
  would make it heuristically fresh in a cache. V1 sent the same `Vary` with
  `private, max-age=60`; `no-cache` keeps the ETag useful without serving another network's
  address (implementer under the operator's autonomy mandate, 2026-10-19).
- Phase 3 review: `ip_host`'s HEAD carries GET's `Content-Length`; `/<field>/` (empty suffix) is
  handed back; its own refusals (unknown suffix or format, malformed query) pass the metadata step
  (implementer under the operator's autonomy mandate, 2026-10-19).
- `ip_host` carries `#[allow(clippy::result_large_err)]`: the spec's `Err(Request)` hands the
  request back by value and cannot be boxed without changing the mount (implementer under the
  operator's autonomy mandate, 2026-10-19).

## Open decisions

None.

## Out of scope

The IP page itself (6.1-6.22), the attribution's placement in the templates, and the cutover's
mount: these happen on `feature/v2-cutover`, as "How the cutover mounts `ip_host`" describes.
