//! Connection pool for predefined DNS provider resolvers.
//!
//! Caches [`Resolver`] instances keyed by `(provider, transport)` to avoid
//! repeated TLS/HTTPS handshakes under sustained load. Only predefined
//! providers are pooled — arbitrary user-supplied IP servers are always
//! created fresh (security: no caching untrusted endpoints).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use mhost::nameserver::NameServerConfig;
use mhost::resolver::{Resolver, ResolverConfig, ResolverOpts};

use crate::parser::Transport;

/// Pool key: provider name (lowercase) + transport protocol.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PoolKey {
    provider: String,
    transport: Transport,
}

/// A cached set of resolvers for a single provider+transport combination.
struct CachedEntry {
    resolvers: Vec<Resolver>,
    created_at: Instant,
    last_used: Instant,
}

/// Thread-safe resolver pool with TTL-based eviction and LRU bounding.
pub struct ResolverPool {
    entries: tokio::sync::RwLock<HashMap<PoolKey, CachedEntry>>,
    ttl: Duration,
    max_size: usize,
}

impl ResolverPool {
    pub fn new(ttl_secs: u64, max_size: usize) -> Self {
        Self {
            entries: tokio::sync::RwLock::new(HashMap::new()),
            ttl: Duration::from_secs(ttl_secs),
            max_size,
        }
    }

    /// Get cached resolvers for a predefined provider+transport, or create and cache new ones.
    ///
    /// `ns_configs` are the filtered nameserver configs (already transport- and IPv4-filtered).
    /// `timeout` is the per-query timeout for resolver construction.
    pub async fn get_or_create(
        &self,
        provider: &str,
        transport: Transport,
        ns_configs: &[NameServerConfig],
        timeout: Duration,
    ) -> Result<Vec<Resolver>, String> {
        let key = PoolKey {
            provider: provider.to_owned(),
            transport,
        };

        // Fast path: write lock to update last_used on hit (required for LRU eviction accuracy).
        {
            let mut entries = self.entries.write().await;
            if let Some(entry) = entries.get_mut(&key) {
                if entry.created_at.elapsed() < self.ttl {
                    entry.last_used = Instant::now();
                    metrics::counter!("prism_resolver_pool_hits_total").increment(1);
                    return Ok(entry.resolvers.clone());
                }
                // Expired — remove and fall through to create.
                entries.remove(&key);
                metrics::counter!("prism_resolver_pool_evictions_total").increment(1);
            }
        }

        metrics::counter!("prism_resolver_pool_misses_total").increment(1);

        // Slow path: create resolvers outside of lock.
        let opts = ResolverOpts {
            timeout,
            ..Default::default()
        };
        let mut resolvers = Vec::with_capacity(ns_configs.len());
        for ns_config in ns_configs {
            let config = ResolverConfig::new(ns_config.clone());
            let resolver = Resolver::new(config, opts.clone())
                .await
                .map_err(|e| e.to_string())?;
            resolvers.push(resolver);
        }

        // Insert under write lock, evicting LRU if at capacity.
        {
            let mut entries = self.entries.write().await;

            // Evict if at capacity and this is a new key.
            if !entries.contains_key(&key) && entries.len() >= self.max_size {
                self.evict_lru(&mut entries);
            }

            entries.insert(
                key,
                CachedEntry {
                    resolvers: resolvers.clone(),
                    created_at: Instant::now(),
                    last_used: Instant::now(),
                },
            );

            metrics::gauge!("prism_resolver_pool_size").set(entries.len() as f64);
        }

        Ok(resolvers)
    }

    /// Run periodic cleanup: remove entries older than TTL.
    pub async fn cleanup(&self) {
        let mut entries = self.entries.write().await;
        let before = entries.len();
        entries.retain(|_, entry| entry.created_at.elapsed() < self.ttl);
        let evicted = before - entries.len();
        if evicted > 0 {
            metrics::counter!("prism_resolver_pool_evictions_total").increment(evicted as u64);
            metrics::gauge!("prism_resolver_pool_size").set(entries.len() as f64);
            tracing::debug!(evicted, remaining = entries.len(), "resolver pool cleanup");
        }
    }

    /// Evict the least-recently-used entry.
    fn evict_lru(&self, entries: &mut HashMap<PoolKey, CachedEntry>) {
        if let Some(lru_key) = entries
            .iter()
            .min_by_key(|(_, v)| v.last_used)
            .map(|(k, _)| k.clone())
        {
            entries.remove(&lru_key);
            metrics::counter!("prism_resolver_pool_evictions_total").increment(1);
        }
    }

    /// Spawn a background task that periodically cleans up expired entries.
    pub fn spawn_cleanup_task(self: &std::sync::Arc<Self>, interval_secs: u64) {
        let pool = std::sync::Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
            interval.tick().await; // Skip the initial immediate tick.
            loop {
                interval.tick().await;
                pool.cleanup().await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_ns_configs() -> Vec<NameServerConfig> {
        use std::net::SocketAddr;
        vec![NameServerConfig::udp(SocketAddr::from(([1, 1, 1, 1], 53)))]
    }

    #[tokio::test]
    async fn pool_hit_returns_cached_resolvers() {
        let pool = ResolverPool::new(300, 32);
        let configs = mock_ns_configs();
        let timeout = Duration::from_secs(5);

        // First call: miss, creates resolvers.
        let r1 = pool
            .get_or_create("cloudflare", Transport::Udp, &configs, timeout)
            .await
            .unwrap();
        assert_eq!(r1.len(), 1);

        // Second call: hit, returns cached.
        let r2 = pool
            .get_or_create("cloudflare", Transport::Udp, &configs, timeout)
            .await
            .unwrap();
        assert_eq!(r2.len(), 1);

        // Different transport: miss.
        let r3 = pool
            .get_or_create("cloudflare", Transport::Tls, &configs, timeout)
            .await
            .unwrap();
        assert_eq!(r3.len(), 1);
    }

    #[tokio::test]
    async fn pool_ttl_eviction() {
        // TTL of 0 seconds means everything expires immediately.
        let pool = ResolverPool::new(0, 32);
        let configs = mock_ns_configs();
        let timeout = Duration::from_secs(5);

        let _ = pool
            .get_or_create("cloudflare", Transport::Udp, &configs, timeout)
            .await
            .unwrap();

        // Sleep just enough to ensure the entry has expired (TTL=0).
        tokio::time::sleep(Duration::from_millis(1)).await;

        pool.cleanup().await;

        // Verify pool is empty by checking that next call creates fresh resolvers.
        // (We can't directly inspect size, but cleanup should have removed the entry.)
        let entries = pool.entries.read().await;
        assert!(entries.is_empty(), "expired entries should be cleaned up");
    }

    #[tokio::test]
    async fn pool_lru_eviction_at_max_size() {
        let pool = ResolverPool::new(300, 2);
        let configs = mock_ns_configs();
        let timeout = Duration::from_secs(5);

        // Fill to capacity.
        let _ = pool
            .get_or_create("cloudflare", Transport::Udp, &configs, timeout)
            .await
            .unwrap();
        let _ = pool
            .get_or_create("google", Transport::Udp, &configs, timeout)
            .await
            .unwrap();

        // Access cloudflare again to make it most-recently-used.
        let _ = pool
            .get_or_create("cloudflare", Transport::Udp, &configs, timeout)
            .await
            .unwrap();

        // Insert a third entry — should evict google (LRU).
        let _ = pool
            .get_or_create("quad9", Transport::Udp, &configs, timeout)
            .await
            .unwrap();

        let entries = pool.entries.read().await;
        assert_eq!(entries.len(), 2);
        assert!(!entries.contains_key(&PoolKey {
            provider: "google".to_owned(),
            transport: Transport::Udp,
        }));
        assert!(entries.contains_key(&PoolKey {
            provider: "cloudflare".to_owned(),
            transport: Transport::Udp,
        }));
        assert!(entries.contains_key(&PoolKey {
            provider: "quad9".to_owned(),
            transport: Transport::Udp,
        }));
    }
}
