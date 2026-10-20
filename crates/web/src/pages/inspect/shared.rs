//! The finished inspector runs a share key opens (spec v2-inspector-shell, requirement 13): kept
//! in memory as data, never in the history.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use netray_model::evidence::Block;
use netray_model::{CheckResult, Protocol};

use super::Tool;
use crate::evidence::Badge;

const MAX_RUNS: u64 = 1_000;
const TTL: Duration = Duration::from_secs(3_600);

/// A finished run as its page renders it.
pub(crate) struct SharedRun {
    pub(crate) tool: Tool,
    pub(crate) input: String,
    pub(crate) target: String,
    pub(crate) started: i64,
    pub(crate) link: Option<(String, String)>,
    pub(crate) grade: Option<&'static str>,
    pub(crate) parts: Vec<(String, Result<Vec<Block>, String>)>,
    pub(crate) band: Option<(Protocol, Result<Vec<CheckResult>, String>)>,
    pub(crate) badges: Vec<(IpAddr, Vec<Badge>)>,
}

/// The finished runs under their share keys, bounded by count and age; a full cache drops the
/// least recently used run.
#[derive(Clone)]
pub struct SharedRuns {
    inner: moka::future::Cache<String, Arc<SharedRun>>,
}

impl Default for SharedRuns {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedRuns {
    pub fn new() -> Self {
        Self::with_ttl(TTL)
    }

    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            inner: moka::future::Cache::builder()
                .max_capacity(MAX_RUNS)
                .time_to_live(ttl)
                .eviction_policy(moka::policy::EvictionPolicy::lru())
                .build(),
        }
    }

    pub(crate) async fn insert(&self, key: String, run: SharedRun) {
        self.inner.insert(key, Arc::new(run)).await;
    }

    pub(crate) async fn get(&self, key: &str) -> Option<Arc<SharedRun>> {
        self.inner.get(key).await
    }
}
