use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use crate::backends::BackendResult;
use crate::check::SectionError;
use crate::scoring::engine::OverallScore;

/// Cached check result stored in the moka cache.
pub struct CachedResult {
    pub sections: HashMap<String, Result<BackendResult, SectionError>>,
    pub score: OverallScore,
    pub duration_ms: u64,
    pub cached_at: SystemTime,
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
