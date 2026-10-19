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

## Phase 2 — Response metadata headers

### Groups

G1: C1–C10 (one response step; every criterion hangs on it).

### Plan

#### G1
1. `Cargo.toml`, `crates/ip/Cargo.toml`, `crates/web/Cargo.toml`: `httpdate = "1"` moves to
   `[workspace.dependencies]` (two crates use it); both crates take `{ workspace = true }`.
2. `crates/web/src/api/state.rs`: `IpLimiters.per_ip` becomes a keyed governor limiter with
   `StateInformationMiddleware`; `IpLimiters::charge_per_ip(client, cost) -> Result<u32, u64>`
   (remaining burst, or retry-after seconds); `prune_if_due` keeps pruning it.
3. `crates/web/src/api/ip.rs`: a `tokio::task_local!` slot for the per-IP remaining budget;
   `charge_caller` charges through `charge_per_ip` and writes the slot (0 on refusal).
   `pub(super) async fn metadata(State<ApiState>, Request, Next) -> Response` runs the request
   inside the slot's scope, then adds `x-ratelimit-limit` (`[api.ip] per_ip_per_minute`),
   `x-ratelimit-remaining` (the slot, when the request was charged; 0 on a 429),
   `x-geoip-database-date`/`-age-days` (from `IpLookup::geoip_build_epoch`), and for a GET with
   status 200: buffers the body, a strong ETag from its hash, `last-modified` (GeoIP date), and a
   304 with empty body when `If-None-Match` lists the ETag or `*`.
4. `crates/web/src/api/jobs.rs`: the inspector's per-IP admission charges through
   `charge_per_ip`.
5. `crates/web/src/api/mod.rs`: the ip routes get `route_layer(from_fn_with_state(state, ip::metadata))`.

## Phase 3 — The ip. host entry point

### Groups

G1: C1–C25 (one entry point over the Phase 1 renderers and the Phase 2 response step).

### Plan

#### G1
1. `crates/web/src/api/ip.rs`: the Phase 2 middleware body becomes a reusable step
   (`with_metadata(state, method, if_none_match, future)`), used by `metadata` and by `ip_host`;
   the query parsing, `answer_address`, the caller answer and `respond` become `pub(super)` (or
   gain a shared core) so `ip_host` calls them with a field, a format suffix and the query.
2. `crates/web/src/api/ip_host.rs` (new): `pub async fn ip_host(&ApiState, Request<Body>) ->
   Result<Response, Request<Body>>`: GET/HEAD only; path match (`/`, `/json|yaml|toml|csv`,
   `/<field>`, `/<field>/<fmt>`, `/ip/cidr`, `/asn` without digits); V1 negotiation
   (`netray_ip::negotiate`): root HTML or nothing hands the request back, other paths fall to
   plain; unknown suffix 404 naming the formats; `?ip=` and the other parameters as `/api/v1/ip`.
3. `crates/web/src/api/mod.rs`: `mod ip_host; pub use ip_host::ip_host;`.
4. `specs/sdd/v2.md`: S40 and §7.2 name `netray_web::api::ip_host` serving the V1 path family
   ahead of the cutover's `410` table.
