# Report: v2-ip-cli

## Phase 1 — Lookup options, formats and fields under /api/v1

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: IpLookup lookup_with, lookup_caller_with, geoip_build_epoch with defaults; IpModule over enrich; domain run keeps lookup | green | crates/web/tests/api_ip_dns.rs |
| C2 | R2: /ip/{address} reverse DNS by default, ?dns= toggle, host is an address field | green | crates/web/tests/api_ip_dns.rs |
| C3 | R3: /ip?ip=X answers X like /ip/X; caller-only field with ?ip= is 400 | green | crates/web/tests/api_ip_dns.rs |
| C4 | R4: ?lang= passes to the lookup, localized names; malformed tag 400 | green | crates/web/tests/api_ip_lang.rs |
| C5 | R5: ?format= json yaml toml csv plain; Accept yaml toml csv; defaults kept; unknown 400; no form 400 | green | crates/web/tests/api_ip_formats.rs |
| C6 | R6: ?fields= keeps named top-level keys of structured answers, no effect on plain | green | crates/web/tests/api_ip_formats.rs |
| C7 | R7: field cidr on both endpoints; field all on the caller, plain and structured | green | crates/web/tests/api_ip_cidr_all.rs |
| C8 | R8: IP module off, caller headers cidr ip plain answer 200; others 503 | green | crates/web/tests/api_ip_cidr_all.rs |
| C9 | R9: batch ?dns=true does reverse DNS per address, default off | green | crates/web/tests/api_ip_batch_meta.rs |
| C10 | R12: GEOIP_ATTRIBUTION constant, /api/v1/meta attribution | green | crates/web/tests/api_ip_batch_meta.rs |
| C11 | S: /ip/198.51.100.1 ip.hostname is stub.example.com | green | crates/web/tests/api_ip_dns.rs |
| C12 | S: ?dns=false hostname null | green | crates/web/tests/api_ip_dns.rs |
| C13 | S: ?field=host is stub.example.com and a newline | green | crates/web/tests/api_ip_dns.rs |
| C14 | S: /ip?ip=198.51.100.1 answers 198.51.100.1, not the caller | green | crates/web/tests/api_ip_dns.rs |
| C15 | S: /ip?ip=10.0.0.1 is 400 INVALID_INPUT | green | crates/web/tests/api_ip_dns.rs |
| C16 | S: /ip?ip=198.51.100.1&field=tcp is 400 naming tcp | green | crates/web/tests/api_ip_dns.rs |
| C17 | S: ?lang=de country_localized Deutschland | green | crates/web/tests/api_ip_lang.rs |
| C18 | S: ?lang=de%3Bx is 400 | green | crates/web/tests/api_ip_lang.rs |
| C19 | S: ?format=yaml application/yaml document | green | crates/web/tests/api_ip_formats.rs |
| C20 | S: ?format=toml and csv bodies | green | crates/web/tests/api_ip_formats.rs |
| C21 | S: Accept application/yaml gives YAML | green | crates/web/tests/api_ip_formats.rs |
| C22 | S: Accept text/plain keeps the JSON document | green | crates/web/tests/api_ip_formats.rs |
| C23 | S: field=location&format=json is location to_json | green | crates/web/tests/api_ip_formats.rs |
| C24 | S: field=country&format=toml is 400 naming country and toml | green | crates/web/tests/api_ip_formats.rs |
| C25 | S: format=xml is 400 | green | crates/web/tests/api_ip_formats.rs |
| C26 | S: fields=ip,location&format=json has exactly ip and location | green | crates/web/tests/api_ip_formats.rs |
| C27 | S: fields=ip&field=country plain unchanged | green | crates/web/tests/api_ip_formats.rs |
| C28 | S: field=cidr for caller 192.0.2.10 is 192.0.2.10/32 | green | crates/web/tests/api_ip_cidr_all.rs |
| C29 | S: /ip/2001:db8::1?field=cidr internal mode is /128 | green | crates/web/tests/api_ip_cidr_all.rs |
| C30 | S: field=all plain is all to_plain then x-test: one | green | crates/web/tests/api_ip_cidr_all.rs |
| C31 | S: field=all&format=json has headers.x-test one | green | crates/web/tests/api_ip_cidr_all.rs |
| C32 | S: module off, headers cidr ip are 200 plain | green | crates/web/tests/api_ip_cidr_all.rs |
| C33 | S: module off, field=country is 503 | green | crates/web/tests/api_ip_cidr_all.rs |
| C34 | S: batch?dns=true hostname set, without null | green | crates/web/tests/api_ip_batch_meta.rs |
| C35 | S: meta attribution equals GEOIP_ATTRIBUTION naming MaxMind GeoLite2 | green | crates/web/tests/api_ip_batch_meta.rs |
| C36 | S: an IpLookup with only the old methods answers lookup_with as lookup | green | crates/web/tests/api_ip_dns.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 45354 | 83 |
| G2 | 1 | sonnet | 95689 | 201 |

### Review

Reader over the phase diff: 1 BLOCKER, 4 AMENDMENT, 3 NIT.

- BLOCKER (fixed, second pass by the orchestrator, test `crates/web/tests/api_ip_review.rs`): with
  the IP module off the caller path charged the per-IP limiter before the seam check, so an 11th
  lookup request answered 429 instead of 503. Validation now runs first, a 503 charges nothing,
  and the no-lookup answers charge once.
- AMENDMENT (repaired in phase): structured `headers` lost V1's `x_forwarded_for_chain`; restored.
- AMENDMENT (repaired in phase): requirement 3 said "same validation" while `headers` and `all`
  are answered with `?ip=`; requirement 3 now says so.
- AMENDMENT (repaired in phase, was affected_phase 3): `?ip=X&field=cidr` and `headers` answered
  503 with the module off; they answer now (requirement 3).
- AMENDMENT (decision logged): validated `?ip=`, `?lang=`, `?format=` turn previously ignored
  malformed values into 400.
- NIT (acted on, SDD accuracy): §7.2 and S40 named `ip_host` before it exists; that sentence moves
  to Phase 3. §7.1's `?ip=` wording corrected.
- NIT (not acted on): with `?ip=`, the unknown-field message lists `tcp` and `user_agent`.

### Behavioural verification

`netray lens` from this worktree with `lens.dev.toml` (ports 18485/19495, scratch databases), IP
module off (no GeoIP locally), `curl -H 'X-Forwarded-For: 203.0.113.9'`:

```
== GET /api/v1/ip?field=cidr                     127.0.0.1/32 [200]
== GET /api/v1/ip?field=ip                       127.0.0.1 [200]
== GET /api/v1/ip?field=headers&format=json      {... "x_forwarded_for_chain": ["203.0.113.9"]} [200]
== GET /api/v1/ip?ip=198.51.100.1&field=cidr     198.51.100.1/32 [200]
== GET /api/v1/ip?field=country                  {"error":{"code":"UNAVAILABLE",...}} [503]
== GET /api/v1/meta                              {..., "attribution":"This product includes GeoLite2 data created by MaxMind, available from https://www.maxmind.com.", ...}
```

Lookups with data (reverse DNS, `?lang=`, formats of a document) are covered by the stub tests
only: no GeoIP data locally and public resolvers are blocked locally.

## Phase 2 — Response metadata headers

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R10: every answer of the ip handlers carries X-RateLimit-Limit, a charged one X-RateLimit-Remaining (0 on any 429), and, with a known GeoIP build time, X-GeoIP-Database-Date and X-GeoIP-Database-Age-Days | green | crates/web/tests/api_ip_headers.rs |
| C2 | R11: a 200 GET carries a strong body-derived ETag and Last-Modified (GeoIP date); If-None-Match equal or * gives 304 with ETag, empty body | green | crates/web/tests/api_ip_headers.rs |
| C3 | S: per_ip_per_minute 60, GET /ip has X-RateLimit-Limit 60 and Remaining one below the burst | green | crates/web/tests/api_ip_headers.rs |
| C4 | S: past the burst, 429 with X-RateLimit-Remaining 0 and Retry-After | green | crates/web/tests/api_ip_headers.rs |
| C5 | S: stub epoch gives X-GeoIP-Database-Date as HTTP date and Age-Days whole days since | green | crates/web/tests/api_ip_headers.rs |
| C6 | S: a seam without build epoch gives no GeoIP header and no Last-Modified | green | crates/web/tests/api_ip_headers.rs |
| C7 | S: two GETs of /ip/198.51.100.1 carry the same strong ETag; Last-Modified equals the GeoIP date | green | crates/web/tests/api_ip_headers.rs |
| C8 | S: If-None-Match set to the ETag gives 304, same ETag, empty body | green | crates/web/tests/api_ip_headers.rs |
| C9 | S: a different If-None-Match gives 200 with the body | green | crates/web/tests/api_ip_headers.rs |
| C10 | S: POST /ip/diff and GET /asn/15169 carry the rate-limit headers; the POST no ETag | green | crates/web/tests/api_ip_headers.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 2 | sonnet | 54886 | 93 |

### Review

Reader over the phase diff: 1 BLOCKER, 1 AMENDMENT, 4 NIT.

- BLOCKER (fixed, second pass by the orchestrator, test `crates/web/tests/api_ip_headers_review.rs`):
  a 429 from the per-target or batch limiter carried the caller's per-IP budget as
  `X-RateLimit-Remaining`; any 429 now carries 0.
- AMENDMENT (repaired in phase, spec): lens's body limit answers or rewrites a 413 under
  `/api/v1` without the ip handlers' headers; requirement 10 now names the answers the handlers
  produce.
- NIT (not acted on): `If-None-Match` compared strongly (a `W/` tag from a proxy misses), only
  its first field line read, HEAD gets no 304; C1's text predated the amended R10 (corrected).

### Behavioural verification

`netray lens` from this worktree, IP module off (no GeoIP locally, so no GeoIP headers or
Last-Modified here; the stub tests cover them):

```
== GET /api/v1/ip?field=cidr
HTTP/1.1 200 OK
x-ratelimit-limit: 60
x-ratelimit-remaining: 9
etag: "7f428b5b40aa8013"
127.0.0.1/32
== GET /api/v1/ip?field=cidr If-None-Match: "7f428b5b40aa8013"
HTTP/1.1 304 Not Modified
etag: "7f428b5b40aa8013"
x-ratelimit-limit: 60
x-ratelimit-remaining: 7
== GET /api/v1/ip?field=country
HTTP/1.1 503 Service Unavailable
x-ratelimit-limit: 60
```

## Phase 3 — The ip. host entry point

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R13: ip_host serves GET and HEAD on the V1 paths; every other path, method and /asn/digits come back as Err(req) unchanged | green | crates/web/tests/ip_host_paths.rs |
| C2 | R14: root negotiation in V1 order, HTML or nothing gives Err; other paths suffix first, HTML or nothing falls to plain; unknown suffix 404 naming the formats | green | crates/web/tests/ip_host_root.rs |
| C3 | R15: every ip_host path answers for ?ip=X; headers shows the request; tcp and user_agent with ?ip= 400; fields lang dns format act; limiters refusals and headers shared; plain root ip cidr headers answer with the module off | green | crates/web/tests/ip_host_misc.rs |
| C4 | S: curl with Accept */* on / gives text/plain caller address and newline | green | crates/web/tests/ip_host_root.rs |
| C5 | S: curl on /?ip=198.51.100.1 gives 198.51.100.1 and newline | green | crates/web/tests/ip_host_root.rs |
| C6 | S: curl with Accept application/json on / gives the JSON document | green | crates/web/tests/ip_host_root.rs |
| C7 | S: browser Accept text/html on / gives Err(req) unchanged | green | crates/web/tests/ip_host_root.rs |
| C8 | S: any agent on /?format=yaml gives YAML | green | crates/web/tests/ip_host_root.rs |
| C9 | S: curl on /country gives the stub country and newline | green | crates/web/tests/ip_host_paths.rs |
| C10 | S: browser on /country gives plain text, not Err | green | crates/web/tests/ip_host_paths.rs |
| C11 | S: /country?ip=198.51.100.1&lang=de gives the plain answer for 198.51.100.1 | green | crates/web/tests/ip_host_paths.rs |
| C12 | S: /location/json gives location to_json of the caller lookup | green | crates/web/tests/ip_host_paths.rs |
| C13 | S: /json?fields=ip gives a JSON object with only ip | green | crates/web/tests/ip_host_paths.rs |
| C14 | S: /yaml /toml /csv give the document in that form | green | crates/web/tests/ip_host_paths.rs |
| C15 | S: curl on /asn gives AS64496 and newline | green | crates/web/tests/ip_host_paths.rs |
| C16 | S: /asn/15169 gives Err(req) | green | crates/web/tests/ip_host_paths.rs |
| C17 | S: /ip/cidr gives 192.0.2.10/32; with ?ip=198.51.100.1 gives 198.51.100.1/32 | green | crates/web/tests/ip_host_paths.rs |
| C18 | S: /all with X-Test one gives all to_plain followed by x-test: one | green | crates/web/tests/ip_host_paths.rs |
| C19 | S: /headers/json gives the filtered request headers as a JSON object | green | crates/web/tests/ip_host_paths.rs |
| C20 | S: /country/xml gives 404 naming the formats | green | crates/web/tests/ip_host_paths.rs |
| C21 | S: /range /batch /meta /nope give Err(req) | green | crates/web/tests/ip_host_paths.rs |
| C22 | S: POST / gives Err(req) | green | crates/web/tests/ip_host_paths.rs |
| C23 | S: module off, curl /, /ip/cidr, /headers give 200 plain; /country 503 | green | crates/web/tests/ip_host_misc.rs |
| C24 | S: /tcp?ip=198.51.100.1 gives 400 naming tcp | green | crates/web/tests/ip_host_misc.rs |
| C25 | S: /country carries X-RateLimit-Limit, the GeoIP headers and an ETag | green | crates/web/tests/ip_host_misc.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 93203 | 292 |

### Review

Reader over the phase diff: 0 BLOCKER, 5 AMENDMENT, 1 NIT. All five amendments repaired in this
phase by the orchestrator, with `crates/web/tests/ip_host_review.rs`:

- HEAD on `ip_host` lost GET's `Content-Length` once mounted under a router; it now carries it.
- No `Cache-Control`/`Vary` on caller-dependent answers that carry `Last-Modified` (also on
  `/api/v1/ip*` since Phase 2); every ip answer is now `private, no-cache` with
  `Vary: Accept, User-Agent`.
- The spec's mount section told the cutover to remove `ip_successor` rows that `ip_host` hands
  back (`/ip/<other>`, other methods); the section now keeps the table unchanged.
- `/<field>/` answered 404 instead of coming back; it now comes back.
- `ip_host`'s own refusals skipped the metadata step; they pass it now.
- NIT (not acted on): the shared-limiter test spends its budget through `ip_host` only.

### Behavioural verification

skipped: `ip_host` has no route on main; the `ip.` host dispatch exists only on
`feature/v2-cutover`, which mounts it as the spec's "How the cutover mounts `ip_host`" says. The
entry point is exercised end to end by direct calls in `crates/web/tests/ip_host_*.rs`.

### Checks to build

- A test file added under an `ADLC-Baseline` commit passes `cargo fmt --check` at that commit
  (a later rustfmt-only change otherwise needs `ADLC-Test-Change`, as `api_ip_headers.rs` did);
  lives as an `adlc` commit check or a `tests/repo` script.
