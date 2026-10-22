// The DNS inspector as a library: its `Module` implementation and the jobs netray-web runs. The
// modules are `pub` for the integration tests in `tests/`.

pub mod api;
pub mod circuit_breaker;
pub mod config;
pub mod dns_dnssec;
pub mod dns_raw;
pub mod dns_trace;
pub mod error;
pub mod jobs;
pub mod module;
pub mod parser;
pub mod record_format;
pub mod result_cache;
pub mod security;
#[cfg(feature = "testing")]
pub mod testing;

pub use api::check::{CheckEvent, lint_status};
pub use api::dnssec::DnssecRequest;
pub use api::query::PostQueryRequest;
pub use api::trace::TraceRequest;
pub use config::ModuleConfig;
pub use jobs::blocks;
pub use jobs::blocks::{authcompare_page_blocks, compare_page_blocks, query_page_blocks};
pub use jobs::{
    Agreement, AnswerOutcome, DnsJobSummary, DnsRecordDetail, DnsServerAnswer, DnsServerSummary,
    DnsTypeComparison,
};
pub use jobs::{
    DnsAuthCompareDone, DnsAuthCompareEvent, DnsAuthCompareResult, DnsBackend, DnsBatch,
    DnsChainEvent, DnsCompareDone, DnsCompareEvent, DnsCompareResult, DnsDnssecDone,
    DnsDnssecEvent, DnsDnssecResult, DnsEnrichmentEvent, DnsErrorEvent, DnsHopEvent, DnsJob,
    DnsJobEvent, DnsJobs, DnsQueryDone, DnsQueryEvent, DnsQueryResult, DnsRefusal, DnsTraceDone,
    DnsTraceEvent, DnsTraceResult,
};
pub use jobs::{
    DnsCachedJob, DnsCachedResult, DnsCheckResult, DnsResultCache, check_result, is_key,
};
pub use jobs::{
    DnsParseAnswer, DnsParseRequest, DnsParsedQuery, DnsRecordTypes, DnsServerCatalogue,
    DnsServerGroup, parse_answer, record_types, server_catalogue,
};
pub use module::{DnsModule, facts_from_lookups, translate};
pub use netray_common::middleware::RequestId;
pub use netray_common::middleware::request_id as request_id_middleware;
