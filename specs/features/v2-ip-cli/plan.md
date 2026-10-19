# Plan: v2-ip-cli

## Phase 1 — Lookup options, formats and fields under /api/v1

### Groups

G1: C1, C36 · G2: C2–C35

G1 is the seam: `crates/ip/src/{lookup,lib,module,testing}.rs`. G2 depends on G1:
`crates/ip/src/format.rs`, `crates/web/src/api/{ip,mod}.rs`, the SDD, the OpenAPI fixture and the
protected tests. `GEOIP_ATTRIBUTION` lands in `lib.rs` with G1 so the groups never share a file.

### Plan

#### G1
1. `crates/ip/src/lookup.rs`: `LookupOptions { reverse_dns, lang }` (Default); `IpLookup` gains
   `lookup_with`, `lookup_caller_with`, `geoip_build_epoch` with defaults delegating to `lookup`,
   `lookup_caller`, `None`.
2. `crates/ip/src/lib.rs`: re-export `LookupOptions`; `pub const GEOIP_ATTRIBUTION`.
3. `crates/ip/src/module.rs`: `IpModule` overrides over `enrich(.., !reverse_dns, lang)`;
   `lookup`/`lookup_caller` delegate with today's values; `geoip_build_epoch` from
   `EnrichmentContext::geoip_city_build_epoch`. The domain run keeps `lookup`.
4. `crates/ip/src/testing.rs`: `STUB_GEOIP_EPOCH`; options helper (hostname `stub.example.com`,
   `lang` `de` gives `Berlin`/`Deutschland`); old methods byte-identical.

#### G2
1. `crates/ip/src/format.rs`: `OutputFormat::from_name` public (no `cfg(test)`).
2. `crates/web/src/api/ip.rs`: `AddressField` + `Host`, `Cidr`; `CallerField` + `Cidr`, `All`;
   name lists extended; one validated query (`field ip dns lang fields format`); one renderer per
   field returning plain or JSON; `respond(form, body)` choosing the form (explicit `?format=`, else
   Accept yaml/toml/csv only, else today's default); shared `answer_address` used by `/ip/{address}`
   and `/ip?ip=`; caller answers `headers`, `cidr`, `ip` before the seam check; batch `?dns`;
   OpenAPI params, a `Format` schema and media types. With `?ip=`, `headers` (the request's) and
   `all` (X's lookup plus the headers) are accepted; `/ip/{address}?field=headers` stays refused.
3. `crates/web/src/api/mod.rs`: `Format` in `ApiDoc` components; `Meta.attribution`.
4. `crates/web/tests/api_ip_lookup.rs`, `crates/web/tests/api_ip_fields.rs` (protected,
   `ADLC-Test-Change`): route bodies compared against the reverse-DNS lookups; `host` leaves
   `CALLER_ONLY`; enum expectations grow by `host`, `cidr`, `all`.
5. `specs/sdd/v2.md`: S40 row; §7.1 ip, batch and meta cells; §7.2 paragraph (first cells unchanged).
6. `tests/fixtures/contracts/api-v1.openapi.json`: regenerated with `UPDATE_GOLDEN=1`.
