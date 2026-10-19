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
