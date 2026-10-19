//! The typed events of the DNS jobs and their collected results.
//!
//! The payloads are what prism's SSE frames carry, field order unchanged; an event serialises
//! as its payload alone and [`DnsJobEvent::name`] gives the SSE event name.

use std::collections::{BTreeMap, HashMap};

use axum::response::sse::Event;
use mhost::resolver::Lookups;
use netray_common::enrichment::IpInfo;
use serde::{Serialize, Serializer};
use utoipa::ToSchema;

use crate::dns_dnssec::ChainLevel;
use crate::dns_trace::TraceHop;
use crate::record_format;

use super::results::DnsCachedJob;
use super::schema::DnsLookups;
use super::summary::{DnsJobSummary, summarise};

/// A non-fatal error of a job, e.g. one resolver failing.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DnsErrorEvent {
    pub code: String,
    pub message: String,
}

impl DnsErrorEvent {
    pub(crate) fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_owned(),
            message: message.to_owned(),
        }
    }
}

/// Enrichment of the IP addresses the job found.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DnsEnrichmentEvent {
    pub request_id: String,
    pub enrichments: HashMap<String, IpInfo>,
}

/// Summary of a query job.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DnsQueryDone {
    pub request_id: String,
    pub total_queries: u32,
    pub duration_ms: u64,
    pub warnings: Vec<String>,
    /// Transport used for this query (e.g., "udp", "tls").
    pub transport: String,
    /// Whether DNSSEC mode was requested.
    pub dnssec: bool,
    /// Providers skipped due to open circuit breakers during this query.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[schema(required = false)]
    pub degraded_providers: Vec<String>,
    /// Cache key for retrieving this result via permalink.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_key: Option<String>,
}

/// Summary of a compare job.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DnsCompareDone {
    pub request_id: String,
    pub total_queries: u32,
    pub duration_ms: u64,
    pub warnings: Vec<String>,
    pub transports: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_key: Option<String>,
}

/// Summary of an authcompare job.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DnsAuthCompareDone {
    pub request_id: String,
    pub total_queries: u32,
    pub duration_ms: u64,
    pub warnings: Vec<String>,
    /// Discovered authoritative nameservers, e.g. ["ns1.example.com (1.2.3.4)"].
    pub auth_servers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_key: Option<String>,
}

/// One delegation level of a trace.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DnsHopEvent {
    pub request_id: String,
    pub hop: TraceHop,
}

/// Summary of a trace job.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DnsTraceDone {
    pub request_id: String,
    pub duration_ms: u64,
    pub hops: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_key: Option<String>,
}

/// One zone level of a DNSSEC chain.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DnsChainEvent {
    pub request_id: String,
    pub level: ChainLevel,
}

/// Summary of a dnssec job.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DnsDnssecDone {
    pub request_id: String,
    pub duration_ms: u64,
    pub levels: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_key: Option<String>,
}

// ---------------------------------------------------------------------------
// Batches
// ---------------------------------------------------------------------------

/// The lookups of a batch: mhost's, or the authoritative branch's synthesised form.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum BatchLookups {
    Resolved(Lookups),
    Authoritative(AuthLookups),
}

/// The authoritative lookups of authcompare, shaped like mhost's `Lookups`.
#[derive(Debug, Clone, Serialize)]
pub struct AuthLookups {
    pub lookups: Vec<AuthLookup>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthLookup {
    pub query: AuthQuery,
    pub name_server: String,
    pub result: AuthResult,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthQuery {
    pub name: String,
    pub record_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub enum AuthResult {
    Response {
        records: Vec<AuthRecord>,
        response_time: WireDuration,
        /// The response code of an answer that is no answer (REFUSED, SERVFAIL, …); not on
        /// the V1 wire.
        #[serde(skip)]
        failure: Option<String>,
        /// The response code mnemonic of a failed answer (`REFUSED`); not on the V1 wire.
        #[serde(skip)]
        failure_code: Option<String>,
    },
    NxDomain {
        response_time: WireDuration,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthRecord {
    pub data: BTreeMap<String, String>,
    pub name: String,
    #[serde(rename = "type")]
    pub record_type: String,
    pub ttl: u32,
    /// The record data in zone-file form, as the recursive rows of the inspector's table show
    /// it; not on the V1 wire.
    #[serde(skip)]
    pub presentation: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct WireDuration {
    pub secs: u64,
    pub nanos: u32,
}

/// The answers for one record type: one per record type, transport or source.
///
/// Serialises with the human-readable record fields injected (`*_human`, `*_explain`,
/// `txt_string`); [`DnsBatch::cache_value`] is the same without them.
#[derive(Debug, Clone, ToSchema)]
pub struct DnsBatch {
    pub request_id: String,
    pub record_type: String,
    #[schema(value_type = DnsLookups)]
    pub lookups: BatchLookups,
    pub completed: u32,
    pub total: u32,
    /// Transport used for this batch (compare only).
    pub transport: Option<String>,
    /// Source of this batch: "recursive" or "authoritative" (authcompare only).
    pub source: Option<String>,
}

#[derive(Serialize)]
struct PlainBatch<'a> {
    request_id: &'a str,
    record_type: &'a str,
    lookups: &'a BatchLookups,
    completed: u32,
    total: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    transport: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: &'a Option<String>,
}

impl DnsBatch {
    fn plain(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(PlainBatch {
            request_id: &self.request_id,
            record_type: &self.record_type,
            lookups: &self.lookups,
            completed: self.completed,
            total: self.total,
            transport: &self.transport,
            source: &self.source,
        })
    }

    /// The batch as prism caches it: without the record enrichment.
    pub fn cache_value(&self) -> serde_json::Value {
        self.plain().unwrap_or(serde_json::Value::Null)
    }
}

impl Serialize for DnsBatch {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut v = self.plain().map_err(serde::ser::Error::custom)?;
        record_format::enrich_lookups_json(&mut v, &self.record_type);
        v.serialize(serializer)
    }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// An event of a DNS job.
pub trait DnsJobEvent: Serialize + Clone + Send + 'static {
    /// What the job's events collect into.
    type Collected: Serialize + Clone + Default + Send + Into<DnsCachedJob>;

    /// The job's name in the result cache and on its route.
    const MODE: &'static str;

    /// The SSE event name prism emits.
    fn name(&self) -> &'static str;

    fn is_done(&self) -> bool;

    /// Sets the permalink key of a `done` event that carries one; otherwise a no-op.
    fn set_cache_key(&mut self, key: String);

    /// The payload as prism caches it (batches unenriched).
    fn cache_value(&self) -> serde_json::Value;

    fn collect_into(self, into: &mut Self::Collected);

    /// The `summary` event that precedes `done`, when the job has one.
    fn summary_event(_collected: &Self::Collected) -> Option<Self> {
        None
    }

    /// prism's SSE frame.
    fn sse_event(&self) -> Event {
        let name = self.name();
        Event::default()
            .event(name)
            .json_data(self)
            .unwrap_or_else(|_| Event::default().event(name).data("{}"))
    }
}

fn value<T: Serialize>(payload: &T) -> serde_json::Value {
    serde_json::to_value(payload).unwrap_or(serde_json::Value::Null)
}

macro_rules! batch_job {
    ($(#[$meta:meta])* $event:ident, $result:ident, $done:ident, $mode:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Serialize, ToSchema)]
        #[serde(untagged)]
        pub enum $event {
            Batch(DnsBatch),
            Error(DnsErrorEvent),
            Enrichment(DnsEnrichmentEvent),
            /// The job's summary, emitted by API v1 before `done`; the job never sends it.
            Summary(DnsJobSummary),
            Done($done),
        }

        /// The events of the job, collected.
        #[derive(Debug, Clone, Default, Serialize, ToSchema)]
        pub struct $result {
            pub batches: Vec<DnsBatch>,
            pub errors: Vec<DnsErrorEvent>,
            #[schema(required = true)]
            pub enrichment: Option<DnsEnrichmentEvent>,
            /// `null` when the stream ended before the job finished.
            #[schema(required = true)]
            pub done: Option<$done>,
            /// `null` until the job is done.
            #[schema(required = true)]
            pub summary: Option<DnsJobSummary>,
        }

        impl From<DnsErrorEvent> for $event {
            fn from(e: DnsErrorEvent) -> Self {
                Self::Error(e)
            }
        }

        impl DnsJobEvent for $event {
            type Collected = $result;
            const MODE: &'static str = $mode;

            fn name(&self) -> &'static str {
                match self {
                    Self::Batch(_) => "batch",
                    Self::Error(_) => "error",
                    Self::Enrichment(_) => "enrichment",
                    Self::Summary(_) => "summary",
                    Self::Done(_) => "done",
                }
            }

            fn is_done(&self) -> bool {
                matches!(self, Self::Done(_))
            }

            fn set_cache_key(&mut self, key: String) {
                if let Self::Done(d) = self {
                    d.cache_key = Some(key);
                }
            }

            fn cache_value(&self) -> serde_json::Value {
                match self {
                    Self::Batch(b) => b.cache_value(),
                    Self::Error(e) => value(e),
                    Self::Enrichment(e) => value(e),
                    Self::Summary(s) => value(s),
                    Self::Done(d) => value(d),
                }
            }

            fn collect_into(self, into: &mut $result) {
                match self {
                    Self::Batch(b) => into.batches.push(b),
                    Self::Error(e) => into.errors.push(e),
                    Self::Enrichment(e) => into.enrichment = Some(e),
                    Self::Summary(s) => into.summary = Some(s),
                    Self::Done(d) => {
                        into.done = Some(d);
                        // A `summary` event already collected (the v1 stream's) wins: one summary
                        // per job, the one that was stored and sent.
                        if into.summary.is_none() {
                            into.summary = Some(summarise(&into.batches));
                        }
                    }
                }
            }

            fn summary_event(collected: &$result) -> Option<Self> {
                collected.summary.clone().map(Self::Summary)
            }
        }
    };
}

batch_job!(
    /// An event of a query job.
    DnsQueryEvent,
    DnsQueryResult,
    DnsQueryDone,
    "query"
);
batch_job!(
    /// An event of a compare job.
    DnsCompareEvent,
    DnsCompareResult,
    DnsCompareDone,
    "compare"
);
batch_job!(
    /// An event of an authcompare job.
    DnsAuthCompareEvent,
    DnsAuthCompareResult,
    DnsAuthCompareDone,
    "authcompare"
);

/// An event of a trace job.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(untagged)]
pub enum DnsTraceEvent {
    Hop(DnsHopEvent),
    Error(DnsErrorEvent),
    Enrichment(DnsEnrichmentEvent),
    Done(DnsTraceDone),
}

/// The events of a trace job, collected.
#[derive(Debug, Clone, Default, Serialize, ToSchema)]
pub struct DnsTraceResult {
    pub hops: Vec<DnsHopEvent>,
    pub errors: Vec<DnsErrorEvent>,
    #[schema(required = true)]
    pub enrichment: Option<DnsEnrichmentEvent>,
    /// `null` when the stream ended before the job finished.
    #[schema(required = true)]
    pub done: Option<DnsTraceDone>,
}

impl From<DnsErrorEvent> for DnsTraceEvent {
    fn from(e: DnsErrorEvent) -> Self {
        Self::Error(e)
    }
}

impl DnsJobEvent for DnsTraceEvent {
    type Collected = DnsTraceResult;
    const MODE: &'static str = "trace";

    fn name(&self) -> &'static str {
        match self {
            Self::Hop(_) => "hop",
            Self::Error(_) => "error",
            Self::Enrichment(_) => "enrichment",
            Self::Done(_) => "done",
        }
    }

    fn is_done(&self) -> bool {
        matches!(self, Self::Done(_))
    }

    fn set_cache_key(&mut self, key: String) {
        if let Self::Done(d) = self {
            d.cache_key = Some(key);
        }
    }

    fn cache_value(&self) -> serde_json::Value {
        match self {
            Self::Hop(e) => value(e),
            Self::Error(e) => value(e),
            Self::Enrichment(e) => value(e),
            Self::Done(d) => value(d),
        }
    }

    fn collect_into(self, into: &mut DnsTraceResult) {
        match self {
            Self::Hop(h) => into.hops.push(h),
            Self::Error(e) => into.errors.push(e),
            Self::Enrichment(e) => into.enrichment = Some(e),
            Self::Done(d) => into.done = Some(d),
        }
    }
}

/// An event of a dnssec job.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(untagged)]
pub enum DnsDnssecEvent {
    Chain(DnsChainEvent),
    Error(DnsErrorEvent),
    Done(DnsDnssecDone),
}

/// The events of a dnssec job, collected.
#[derive(Debug, Clone, Default, Serialize, ToSchema)]
pub struct DnsDnssecResult {
    pub levels: Vec<DnsChainEvent>,
    pub errors: Vec<DnsErrorEvent>,
    /// `null` when the stream ended before the job finished.
    #[schema(required = true)]
    pub done: Option<DnsDnssecDone>,
}

impl From<DnsErrorEvent> for DnsDnssecEvent {
    fn from(e: DnsErrorEvent) -> Self {
        Self::Error(e)
    }
}

impl DnsJobEvent for DnsDnssecEvent {
    type Collected = DnsDnssecResult;
    const MODE: &'static str = "dnssec";

    fn name(&self) -> &'static str {
        match self {
            Self::Chain(_) => "chain",
            Self::Error(_) => "error",
            Self::Done(_) => "done",
        }
    }

    fn is_done(&self) -> bool {
        matches!(self, Self::Done(_))
    }

    fn set_cache_key(&mut self, key: String) {
        if let Self::Done(d) = self {
            d.cache_key = Some(key);
        }
    }

    fn cache_value(&self) -> serde_json::Value {
        match self {
            Self::Chain(e) => value(e),
            Self::Error(e) => value(e),
            Self::Done(d) => value(d),
        }
    }

    fn collect_into(self, into: &mut DnsDnssecResult) {
        match self {
            Self::Chain(c) => into.levels.push(c),
            Self::Error(e) => into.errors.push(e),
            Self::Done(d) => into.done = Some(d),
        }
    }
}
