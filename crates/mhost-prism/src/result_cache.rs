//! In-memory result cache for shareable permalinks.
//!
//! Stores completed SSE event sequences (batch, lint, hop, done) keyed by a
//! short prefix of a UUID v7. The cache is bounded by entry count and TTL.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Maximum number of cached results.
const MAX_ENTRIES: u64 = 1_000;

/// Time-to-live for each cached entry.
const TTL_SECS: u64 = 3_600; // 1 hour

/// Length of the cache key (prefix of a UUID v7 hex string, no hyphens).
const KEY_LENGTH: usize = 12;

/// A single SSE event captured during streaming.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedEvent {
    /// SSE event type (e.g., "batch", "lint", "hop", "done").
    pub event_type: String,
    /// JSON payload of the event.
    pub data: serde_json::Value,
}

/// The full cached result for a completed query/check/trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedResult {
    /// The original query string that produced these results.
    pub query: String,
    /// The endpoint mode: "query", "check", or "trace".
    pub mode: String,
    /// All SSE events in order.
    pub events: Vec<CachedEvent>,
}

/// Thread-safe, TTL-aware LRU result cache.
#[derive(Clone)]
pub struct ResultCache {
    inner: moka::future::Cache<String, CachedResult>,
}

impl Default for ResultCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ResultCache {
    pub fn new() -> Self {
        let inner = moka::future::Cache::builder()
            .max_capacity(MAX_ENTRIES)
            .time_to_live(Duration::from_secs(TTL_SECS))
            .build();
        Self { inner }
    }

    /// Generate a short, unique cache key from a UUID v7.
    pub fn generate_key() -> String {
        let id = uuid::Uuid::now_v7();
        // Use simple hex without hyphens, take first KEY_LENGTH chars.
        id.simple().to_string()[..KEY_LENGTH].to_owned()
    }

    /// Insert a completed result into the cache.
    pub async fn insert(&self, key: String, result: CachedResult) {
        self.inner.insert(key, result).await;
    }

    /// Look up a cached result by key.
    pub async fn get(&self, key: &str) -> Option<CachedResult> {
        self.inner.get(key).await
    }
}
