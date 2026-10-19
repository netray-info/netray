# Plan: v2-dns-api

## Phase 1 — Per-server comparison and run summary

## Groups

- G1: C1–C21 (one production path: the summary types feed the collected results, which feed the v1 SSE)

## Plan

G1:
- `crates/dns/Cargo.toml`: `chrono` (workspace, `clock`) as a normal dependency.
- `crates/dns/src/jobs/summary.rs` (new): `AnswerOutcome`, `Agreement`, `DnsRecordDetail`, `DnsServerAnswer`, `DnsTypeComparison`, `DnsServerSummary`, `DnsJobSummary`; `summarise(&[DnsBatch])`, `agreement`, `median_ms`, `soa_serial_age_days`. Records via `crate::module::rdata_text`.
- `crates/dns/src/jobs/mod.rs`, `crates/dns/src/lib.rs`: `pub mod summary`, re-exports.
- `crates/dns/src/jobs/events.rs`: `summary: Option<DnsJobSummary>` on the three batch results, set in `collect_into(Done)`; `DnsJobEvent::summary_event(&Collected) -> Option<Event>` (default `None`).
- `crates/dns/src/testing.rs`: stub names `diverge.example.com` (A per server) and `soa.example.com` (SOA).
- `crates/web/src/api/dns.rs`: SSE path keeps a running collection and emits `summary_event` before `done`; components registered in the DNS doc part.

## Phase 2 — Query language on GET

## Groups

- G1: C1–C15 (one file: the v1 DNS routes)

## Plan

G1:
- `crates/web/src/api/dns.rs`: a `QueryString { q: Option<String> }` extractor (axum `Query`, rejection → 400 INVALID_INPUT); `parsed_q(q) -> Result<ParsedQuery, V1Error>` (missing or blank → INVALID_INPUT "missing query parameter `q`", parser error → INVALID_INPUT with its message). Handlers `dns_query_get`, `dns_compare_get`, `dns_authcompare_get`, `dns_trace_get`, `dns_dnssec_get` with `#[utoipa::path(get, …, params(("q" = String, Query, …)))]` and the same responses as their POST twins plus 307 on query; they call `jobs.query_parsed` / `compare_parsed` / `authcompare_parsed` / `trace` (TraceRequest from the first type, A when none) / `dnssec` (DnssecRequest from the domain), then the shared `respond`. Routing flags read from the whitespace tokens of `q` (`+trace`, `+compare`, `+auth`, `+check`, case-insensitive, in that order) before parsing → `307` with `Location: /api/v1/dns/<path>?q=<percent-encoded>` (`percent_encoding::utf8_percent_encode` with an ASCII set that keeps only unreserved characters). `dns_doc_routes` registers each GET with its POST twin in one `routes!`.

## Phase 3 — Parse, servers and record types

## Groups

- G1: C1–C12 (the reference functions in netray-dns and their v1 routes)

## Plan

G1:
- `crates/dns/src/api/parse.rs`: `pub fn parse_input(input, cursor_pos: Option<usize>, allow_system: bool) -> (Vec<TokenInfo>, Vec<Completion>)` holding the V1 handler's body (the V1 handler calls it); `TokenInfo`, `Completion` stay as they serialise.
- `crates/dns/src/api/meta.rs`: `pub fn server_catalogue() -> Vec<ServerInfo>` and `pub fn record_type_names() -> Vec<String>` from the V1 handlers' bodies (the handlers call them).
- `crates/dns/src/parser.rs`: `pub fn server_groups() -> Vec<(&'static str, Vec<String>)>` over `SERVER_GROUP_ALIASES`.
- `crates/dns/src/jobs/reference.rs` (new, re-exported): `DnsParseRequest {input, cursor_pos}`, `DnsParseAnswer {tokens, completions, query: Option<DnsParsedQuery>, error: Option<String>}`, `DnsParsedQuery {domain, record_types, servers, transport, dnssec, warnings}`, `DnsServerCatalogue {providers, groups}`, `DnsServerGroup {name, providers}`, `DnsRecordTypes {record_types: Vec<RecordTypeInfo>}`; `pub fn parse_answer(&DnsParseRequest, allow_system) -> DnsParseAnswer`, `server_catalogue()`, `record_types()`.
- `crates/dns/src/jobs/mod.rs`: `DnsJobs::allows_system_resolvers()`.
- `crates/web/src/api/dns.rs`: `dns_parse` (POST, JSON body via `json_body`), `dns_servers`, `dns_record_types` (GET) in `dns_read_routes()`; `crates/web/src/api/mod.rs` layers them with the `[api]` per-IP limiter (as `read_routes`) and composes them into the DNS document part.

## Phase 4 — Result cache and replay

## Groups

- G1: C1–C9 (the cache in netray-dns, the v1 respond path and the replay route)

## Plan

G1:
- `crates/dns/src/jobs/results.rs` (new, re-exported): `DnsCachedJob` (untagged enum: Query, Compare, AuthCompare, Trace, Dnssec results; `ToSchema`, `Clone`, `Serialize`) with `From` for each collected result; `DnsCachedResult {key, mode, query, result: DnsCachedJob}`; `DnsResultCache` (moka future cache, 1 000 entries, TTL one hour) with `fresh_key()` (the last 12 hex characters of a UUID v7, its random bits), `insert`, `get`; `is_key(&str)` (12 ASCII hex characters).
- `crates/dns/src/jobs/events.rs`: `DnsDnssecDone.cache_key: Option<String>` (`skip_serializing_if = "Option::is_none"`), `DnsDnssecEvent::set_cache_key` sets it; `DnsJobEvent` gains `const MODE: &'static str` (or a `mode()` fn) per job.
- `crates/web/src/api/state.rs`: `ApiState` holds an `Arc<DnsResultCache>` (`dns_results()`).
- `crates/web/src/api/dns.rs`: `respond(job, headers, state, query)` — both paths run the events through one scan: on `done`, a fresh key is set on the event (`set_cache_key`), the event is collected, the collected result is stored as `DnsCachedResult {key, mode, query, result}`; the SSE path emits `summary` and the keyed `done`; the JSON path answers the collected result. `query` is the GET's `q` or the POST body's `domain`. `dns_result` handler `GET /dns/results/{key}` behind the `[api]` per-IP limiter (in `dns_read_routes`): malformed key 400 INVALID_INPUT, unknown 404 NOT_FOUND.
