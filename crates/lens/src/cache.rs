use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use moka::future::Cache;

use crate::check::SectionError;
use crate::modules::BackendResult;
use crate::score::OverallScore;

/// Cached check result stored in the moka cache.
pub struct CachedResult {
    pub sections: HashMap<String, Result<BackendResult, SectionError>>,
    pub score: OverallScore,
    pub duration_ms: u64,
    pub cached_at: SystemTime,
    /// Snapshot id created at first compute. Reused on cache hits so the
    /// snapshot URL stays available for the lifetime of the cache entry.
    pub snapshot_id: Option<String>,
}

/// Build the cache key for a domain: lowercased + trimmed.
pub fn cache_key(domain: &str) -> String {
    domain.trim().to_lowercase()
}

/// Returns true if the cached result is still within the TTL window.
pub fn is_fresh(cached: &CachedResult, ttl_seconds: u64) -> bool {
    match cached.cached_at.elapsed() {
        Ok(age) => age < Duration::from_secs(ttl_seconds),
        Err(_) => false,
    }
}

/// The one cache writer for `/api/check`: refuses an incomplete result.
/// Returns whether the entry was stored.
pub async fn store_result(
    cache: &Cache<String, Arc<CachedResult>>,
    key: String,
    entry: Arc<CachedResult>,
) -> bool {
    if !entry.score.complete {
        return false;
    }
    cache.insert(key, entry).await;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_lowercases_and_trims() {
        assert_eq!(cache_key("  Example.COM  "), "example.com");
        assert_eq!(cache_key("DNS.NETRAY.INFO"), "dns.netray.info");
    }

    #[test]
    fn cache_key_empty() {
        assert_eq!(cache_key(""), "");
        assert_eq!(cache_key("   "), "");
    }
}
