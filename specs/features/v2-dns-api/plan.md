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
