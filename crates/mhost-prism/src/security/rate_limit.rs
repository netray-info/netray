//! GCRA-based rate limiting using the `governor` crate.
//!
//! Three independent rate limiters enforce the SDD §8.2 budget model:
//!
//! - **Per-IP**: Limits total query cost per client IP per minute.
//! - **Per-target**: Limits total query cost per DNS target (provider/server) per minute.
//! - **Global**: Limits total query cost across all clients per minute.
//!
//! Query cost is computed as `record_types × servers` — the number of individual DNS
//! lookups that will be issued. Active stream tracking prevents a single IP from
//! holding too many concurrent SSE connections.

use std::collections::HashMap;
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};

use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};

use netray_common::rate_limit::{self, KeyedLimiter};

use crate::config::LimitsConfig;
use crate::error::ApiError;

/// Rate limiting state shared across all request handlers.
pub struct RateLimitState {
    per_ip: KeyedLimiter<IpAddr>,
    per_target: KeyedLimiter<String>,
    global: RateLimiter<NotKeyed, InMemoryState, governor::clock::DefaultClock>,
    active_streams: Arc<Mutex<HashMap<IpAddr, usize>>>,
    per_ip_max_streams: u32,
}

impl RateLimitState {
    /// Build rate limiters from configuration values.
    ///
    /// Note: Governor's `DefaultKeyedStateStore` (DashMap) never evicts entries.
    /// Memory grows ~50 bytes per unique IP/target key. This is acceptable for
    /// production workloads — a server restart clears all state. TTL-based
    /// eviction is a future improvement (see roadmap Tier 4).
    pub fn new(config: &LimitsConfig) -> Self {
        let per_ip = RateLimiter::keyed(
            Quota::per_minute(
                NonZeroU32::new(config.per_ip_per_minute).expect("validated non-zero"),
            )
            .allow_burst(NonZeroU32::new(config.per_ip_burst).expect("validated non-zero")),
        );

        let per_target = RateLimiter::keyed(
            Quota::per_minute(
                NonZeroU32::new(config.per_target_per_minute).expect("validated non-zero"),
            )
            .allow_burst(NonZeroU32::new(config.per_target_burst).expect("validated non-zero")),
        );

        let global = RateLimiter::direct(
            Quota::per_minute(
                NonZeroU32::new(config.global_per_minute).expect("validated non-zero"),
            )
            .allow_burst(NonZeroU32::new(config.global_burst).expect("validated non-zero")),
        );

        Self {
            per_ip,
            per_target,
            global,
            active_streams: Arc::new(Mutex::new(HashMap::new())),
            per_ip_max_streams: config.per_ip_max_streams,
        }
    }

    /// Check whether a query with the given cost is allowed.
    ///
    /// `target_keys` are derived from the server specs (provider name, "system", or IP string).
    /// `total_cost` is `record_types × servers` — the total number of DNS lookups.
    /// `per_target_cost` is `record_types` — the lookups charged to each individual target.
    ///
    /// Returns `Ok(StreamGuard)` if allowed (caller must hold the guard for the stream's
    /// lifetime), or `Err(ApiError::RateLimited)` if any limiter rejects.
    pub fn check_query_cost(
        &self,
        client_ip: IpAddr,
        target_keys: &[String],
        total_cost: u32,
        per_target_cost: u32,
    ) -> Result<StreamGuard, ApiError> {
        let total_nz = NonZeroU32::new(total_cost.max(1)).expect("max(1) is non-zero");
        let target_nz = NonZeroU32::new(per_target_cost.max(1)).expect("max(1) is non-zero");

        // 1. Check active stream count for this IP.
        {
            let streams = self.active_streams.lock().expect("streams lock poisoned");
            let count = streams.get(&client_ip).copied().unwrap_or(0);
            if count >= self.per_ip_max_streams as usize {
                metrics::counter!("prism_rate_limit_hits_total", "scope" => "max_streams")
                    .increment(1);
                return Err(ApiError::RateLimited {
                    retry_after_secs: 1,
                    scope: "max_streams",
                });
            }
        }

        // 2. Per-IP rate limit (total cost: all lookups charged to this client).
        rate_limit::check_keyed_cost(&self.per_ip, &client_ip, total_nz, "per_ip", "prism")
            .map_err(|r| ApiError::RateLimited {
                retry_after_secs: r.retry_after_secs,
                scope: r.scope,
            })?;

        // 3. Per-target rate limit (each target only charged its share: record_types).
        for key in target_keys {
            rate_limit::check_keyed_cost(&self.per_target, key, target_nz, "per_target", "prism")
                .map_err(|r| ApiError::RateLimited {
                retry_after_secs: r.retry_after_secs,
                scope: r.scope,
            })?;
        }

        // 4. Global rate limit (total cost).
        rate_limit::check_direct_cost(&self.global, total_nz, "prism").map_err(|r| {
            ApiError::RateLimited {
                retry_after_secs: r.retry_after_secs,
                scope: r.scope,
            }
        })?;

        // All checks passed — increment active stream count.
        let guard = StreamGuard::new(Arc::clone(&self.active_streams), client_ip);
        Ok(guard)
    }
}

/// RAII guard that tracks active SSE streams per IP.
///
/// Increments the count on creation, decrements on drop. Move this into
/// the spawned SSE task so it lives for the stream's entire lifetime.
///
/// Owns an `Arc` to the shared stream map so it is `Send + 'static` and
/// can be moved into a `tokio::spawn` future.
pub struct StreamGuard {
    active_streams: Arc<Mutex<HashMap<IpAddr, usize>>>,
    client_ip: IpAddr,
}

impl StreamGuard {
    fn new(active_streams: Arc<Mutex<HashMap<IpAddr, usize>>>, client_ip: IpAddr) -> Self {
        let mut streams = active_streams.lock().expect("streams lock poisoned");
        *streams.entry(client_ip).or_insert(0) += 1;
        drop(streams);
        Self {
            active_streams,
            client_ip,
        }
    }
}

impl Drop for StreamGuard {
    fn drop(&mut self) {
        let mut streams = self.active_streams.lock().expect("streams lock poisoned");
        if let Some(count) = streams.get_mut(&self.client_ip) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                streams.remove(&self.client_ip);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> LimitsConfig {
        LimitsConfig {
            per_ip_per_minute: 30,
            per_ip_burst: 40,
            per_target_per_minute: 30,
            per_target_burst: 20,
            global_per_minute: 500,
            global_burst: 50,
            max_concurrent_connections: 256,
            per_ip_max_streams: 3,
            max_timeout_secs: 10,
            max_record_types: 10,
            max_servers: 4,
        }
    }

    #[test]
    fn allows_query_within_budget() {
        let state = RateLimitState::new(&test_config());
        let ip: IpAddr = "198.51.100.1".parse().unwrap();
        let targets = vec!["cloudflare".to_string()];

        // 4 record types × 1 server = total 4, per-target 4.
        let guard = state.check_query_cost(ip, &targets, 4, 4);
        assert!(guard.is_ok());
    }

    #[test]
    fn rejects_when_per_ip_exhausted() {
        let state = RateLimitState::new(&test_config());
        let ip: IpAddr = "198.51.100.1".parse().unwrap();
        let targets = vec!["cloudflare".to_string()];

        // Burst is 40 — first call with cost 40 should succeed.
        assert!(state.check_query_cost(ip, &targets, 40, 10).is_ok());
        // Second call should be rejected (burst exhausted).
        assert!(state.check_query_cost(ip, &targets, 1, 1).is_err());
    }

    #[test]
    fn different_ips_have_independent_per_ip_budgets() {
        let state = RateLimitState::new(&test_config());
        let ip1: IpAddr = "198.51.100.1".parse().unwrap();
        let ip2: IpAddr = "198.51.100.2".parse().unwrap();
        // Use different targets so per-target limits don't interfere.
        let targets1 = vec!["cloudflare".to_string()];
        let targets2 = vec!["google".to_string()];

        assert!(state.check_query_cost(ip1, &targets1, 10, 10).is_ok());
        // ip2 has its own per-IP budget.
        assert!(state.check_query_cost(ip2, &targets2, 10, 10).is_ok());
    }

    #[test]
    fn rejects_when_max_streams_exceeded() {
        let state = RateLimitState::new(&test_config());
        let ip: IpAddr = "198.51.100.1".parse().unwrap();
        let targets = vec!["cloudflare".to_string()];

        // Hold 3 guards (max_streams = 3).
        let _g1 = state.check_query_cost(ip, &targets, 1, 1).unwrap();
        let _g2 = state.check_query_cost(ip, &targets, 1, 1).unwrap();
        let _g3 = state.check_query_cost(ip, &targets, 1, 1).unwrap();

        // 4th should be rejected.
        assert!(state.check_query_cost(ip, &targets, 1, 1).is_err());
    }

    #[test]
    fn stream_guard_decrements_on_drop() {
        let state = RateLimitState::new(&test_config());
        let ip: IpAddr = "198.51.100.1".parse().unwrap();
        let targets = vec!["cloudflare".to_string()];

        let g1 = state.check_query_cost(ip, &targets, 1, 1).unwrap();
        let _g2 = state.check_query_cost(ip, &targets, 1, 1).unwrap();
        let _g3 = state.check_query_cost(ip, &targets, 1, 1).unwrap();

        // At max streams — 4th rejected.
        assert!(state.check_query_cost(ip, &targets, 1, 1).is_err());

        // Drop one guard — now 4th should succeed.
        drop(g1);
        assert!(state.check_query_cost(ip, &targets, 1, 1).is_ok());
    }

    #[test]
    fn cost_calculation_matches_query_shape() {
        // 4 record types × 2 servers = total cost 8, per-target cost 4.
        let state = RateLimitState::new(&test_config());
        let ip: IpAddr = "198.51.100.1".parse().unwrap();
        let targets = vec!["cloudflare".to_string(), "google".to_string()];

        // Per-IP burst is 40, total cost 8 should succeed.
        assert!(state.check_query_cost(ip, &targets, 8, 4).is_ok());
        // Per-target burst is 20; cost 21 exceeds it entirely (InsufficientCapacity) → rejected.
        assert!(state.check_query_cost(ip, &targets, 42, 21).is_err());
    }
}
