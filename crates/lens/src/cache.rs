use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use moka::future::Cache;
use moka::ops::compute::{CompResult, Op};

use crate::backends::BackendResult;
use crate::check::SectionError;
use crate::scoring::engine::OverallScore;

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

/// Coalesced recompute for badge and OG: concurrent callers for one key share a fresh
/// entry. A fresh entry is returned as is; otherwise `init` runs and its result is stored
/// only when complete. An incomplete result is returned to the caller without caching.
pub async fn get_or_compute<F>(
    cache: &Cache<String, Arc<CachedResult>>,
    key: String,
    ttl_seconds: u64,
    init: F,
) -> Arc<CachedResult>
where
    F: Future<Output = CachedResult>,
{
    let computed: Arc<Mutex<Option<Arc<CachedResult>>>> = Arc::new(Mutex::new(None));
    let slot = Arc::clone(&computed);
    let result = cache
        .entry(key)
        .and_compute_with(move |existing| async move {
            if let Some(e) = existing
                && is_fresh(e.value(), ttl_seconds)
            {
                return Op::Nop;
            }
            let value = Arc::new(init.await);
            if let Ok(mut guard) = slot.lock() {
                *guard = Some(Arc::clone(&value));
            }
            if value.score.complete {
                Op::Put(value)
            } else {
                Op::Nop
            }
        })
        .await;
    let fresh = computed.lock().ok().and_then(|mut g| g.take());
    match (fresh, result) {
        (Some(v), _) => v,
        (None, CompResult::Unchanged(e)) => e.into_value(),
        (None, CompResult::Inserted(e) | CompResult::ReplacedWith(e)) => e.into_value(),
        (None, CompResult::Removed(e)) => e.into_value(),
        (None, CompResult::StillNone(_)) => unreachable!("init runs when no fresh entry exists"),
    }
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
