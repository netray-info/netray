//! The result cache of API v1: each DNS job's collected result under a 12-hex key, for
//! `GET /dns/results/{key}`.

use std::time::Duration;

use netray_engine::SectionOutcome;
use netray_model::evidence::{Block, BlockId};
use netray_model::run::SectionKind;
use netray_model::{CheckId, Protocol, Status};
use serde::Serialize;
use utoipa::ToSchema;

use super::events::{
    DnsAuthCompareResult, DnsCompareResult, DnsDnssecResult, DnsQueryResult, DnsTraceResult,
};

const MAX_ENTRIES: u64 = 1_000;
const TTL: Duration = Duration::from_secs(3_600);
const KEY_LENGTH: usize = 12;

/// The collected result of a job, whichever job it was.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(untagged)]
pub enum DnsCachedJob {
    Query(DnsQueryResult),
    Compare(DnsCompareResult),
    AuthCompare(DnsAuthCompareResult),
    Trace(DnsTraceResult),
    Dnssec(DnsDnssecResult),
    Check(DnsCheckResult),
}

impl DnsCachedJob {
    /// The evidence blocks of the result, as the job's own mapping builds them.
    pub fn blocks(&self) -> Vec<Block> {
        match self {
            Self::Query(r) => super::blocks::query_blocks(r),
            Self::Compare(r) => super::blocks::compare_blocks(r),
            Self::AuthCompare(r) => super::blocks::authcompare_blocks(r),
            Self::Trace(r) => super::blocks::trace_blocks(r),
            Self::Dnssec(r) => super::blocks::dnssec_blocks(r),
            Self::Check(c) => c.blocks.clone(),
        }
    }

    /// The blocks the inspector page shows: for a query, compare or authcompare result they
    /// lead with `dns.answers` and `dns.servers`; every other result's are `blocks()`.
    pub fn page_blocks(&self) -> Vec<Block> {
        match self {
            Self::Query(r) => super::blocks::query_page_blocks(r),
            Self::Compare(r) => super::blocks::compare_page_blocks(r),
            Self::AuthCompare(r) => super::blocks::authcompare_page_blocks(r),
            _ => self.blocks(),
        }
    }
}

/// One check of the section; findings and evidence are always present, empty or not.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DnsCheckEntry {
    pub id: CheckId,
    pub status: Status,
    pub findings: Vec<String>,
    pub evidence: Vec<BlockId>,
}

/// The DNS module's section of one run: what `GET /domains/{domain}/runs` streams as `section`,
/// with the key its result is stored under.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DnsCheckResult {
    pub protocol: Protocol,
    pub outcome: SectionKind,
    pub reason: Option<String>,
    pub checks: Vec<DnsCheckEntry>,
    pub blocks: Vec<Block>,
    /// The 12-character hex key `GET /dns/results/{key}` replays this result under.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_key: Option<String>,
}

/// The section a module run ended in, as an answer.
pub fn check_result(outcome: SectionOutcome) -> DnsCheckResult {
    let (outcome, reason, checks, blocks) = match outcome {
        SectionOutcome::Measured { checks, evidence } => {
            let checks = checks
                .into_iter()
                .map(|c| DnsCheckEntry {
                    id: c.id,
                    status: c.status,
                    findings: c.findings,
                    evidence: c.evidence,
                })
                .collect();
            (SectionKind::Measured, None, checks, evidence)
        }
        SectionOutcome::NotApplicable { reason } => {
            (SectionKind::NotApplicable, Some(reason), vec![], vec![])
        }
        SectionOutcome::Incomplete { reason } => {
            (SectionKind::Incomplete, Some(reason), vec![], vec![])
        }
        SectionOutcome::TimedOut => (SectionKind::TimedOut, None, vec![], vec![]),
    };
    DnsCheckResult {
        protocol: Protocol::Dns,
        outcome,
        reason,
        checks,
        blocks,
        cache_key: None,
    }
}

macro_rules! cached_from {
    ($($result:ident => $variant:ident),+ $(,)?) => {
        $(impl From<$result> for DnsCachedJob {
            fn from(result: $result) -> Self {
                Self::$variant(result)
            }
        })+
    };
}

cached_from!(
    DnsQueryResult => Query,
    DnsCompareResult => Compare,
    DnsAuthCompareResult => AuthCompare,
    DnsTraceResult => Trace,
    DnsDnssecResult => Dnssec,
);

/// A stored job result and what produced it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DnsCachedResult {
    /// The 12-character hex key from the job's `done`.
    pub key: String,
    /// The job: `query`, `compare`, `authcompare`, `trace`, `dnssec` or `check`.
    pub mode: String,
    /// The `q` of the GET, or the `domain` of the POST body.
    pub query: String,
    /// The job's collected result, as the job answered it.
    pub result: DnsCachedJob,
}

/// Whether `s` is the shape of a key: 12 lowercase hex characters.
pub fn is_key(s: &str) -> bool {
    s.len() == KEY_LENGTH && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Bounded by entry count and age.
#[derive(Clone)]
pub struct DnsResultCache {
    inner: moka::future::Cache<String, DnsCachedResult>,
}

impl Default for DnsResultCache {
    fn default() -> Self {
        Self::new()
    }
}

impl DnsResultCache {
    pub fn new() -> Self {
        Self {
            inner: moka::future::Cache::builder()
                .max_capacity(MAX_ENTRIES)
                .time_to_live(TTL)
                // LRU, not moka's default TinyLFU: a full cache must drop the oldest result, not
                // refuse the new one whose key the job just handed out.
                .eviction_policy(moka::policy::EvictionPolicy::lru())
                .build(),
        }
    }

    /// A key of random bits: the first 12 hex characters of a UUID v4, all random (its version
    /// nibble is the 13th). A v7's tail holds counter bits shared by keys of one millisecond.
    pub fn fresh_key() -> String {
        uuid::Uuid::new_v4().simple().to_string()[..KEY_LENGTH].to_owned()
    }

    pub async fn insert(&self, result: DnsCachedResult) {
        self.inner.insert(result.key.clone(), result).await;
    }

    /// Runs the cache's pending admission and eviction now instead of on its next operation.
    pub async fn run_pending_tasks(&self) {
        self.inner.run_pending_tasks().await;
    }

    pub async fn get(&self, key: &str) -> Option<DnsCachedResult> {
        self.inner.get(key).await
    }
}
