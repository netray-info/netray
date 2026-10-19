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
