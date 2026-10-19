//! The result cache of API v1: each DNS job's collected result under a 12-hex key, for
//! `GET /dns/results/{key}`.

use std::time::Duration;

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
    /// The job: `query`, `compare`, `authcompare`, `trace` or `dnssec`.
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

    /// A key from the random bits of a UUID v7: its last 12 hex characters.
    pub fn fresh_key() -> String {
        let id = uuid::Uuid::now_v7().simple().to_string();
        id[id.len() - KEY_LENGTH..].to_owned()
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
