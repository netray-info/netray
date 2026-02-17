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

use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};

use crate::config::LimitsConfig;
use crate::error::ApiError;

/// Keyed rate limiter type alias for readability.
type KeyedLimiter<K> = RateLimiter<K, governor::state::keyed::DefaultKeyedStateStore<K>, DefaultClock>;

/// Rate limiting state shared across all request handlers.
pub struct RateLimitState {
    per_ip: KeyedLimiter<IpAddr>,
    per_target: KeyedLimiter<String>,
    global: RateLimiter<NotKeyed, InMemoryState, DefaultClock>,
    active_streams: Arc<Mutex<HashMap<IpAddr, usize>>>,
    per_ip_max_streams: u32,
}

impl RateLimitState {
    /// Build rate limiters from configuration values.
    pub fn new(config: &LimitsConfig) -> Self {
        let per_ip = RateLimiter::keyed(
            Quota::per_minute(
                NonZeroU32::new(config.per_ip_per_minute).expect("validated non-zero"),
            )
            .allow_burst(
                NonZeroU32::new(config.per_ip_burst).expect("validated non-zero"),
            ),
        );

        let per_target = RateLimiter::keyed(
            Quota::per_minute(
                NonZeroU32::new(config.per_target_per_minute).expect("validated non-zero"),
            )
            .allow_burst(NonZeroU32::new(10).expect("non-zero literal")),
        );

        let global = RateLimiter::direct(
            Quota::per_minute(
                NonZeroU32::new(config.global_per_minute).expect("validated non-zero"),
            )
            .allow_burst(NonZeroU32::new(50).expect("non-zero literal")),
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
    /// `cost` is `record_types × servers` — the number of DNS lookups that will be issued.
    ///
    /// Returns `Ok(StreamGuard)` if allowed (caller must hold the guard for the stream's
    /// lifetime), or `Err(ApiError::RateLimited)` if any limiter rejects.
    pub fn check_query_cost(
        &self,
        client_ip: IpAddr,
        target_keys: &[String],
        cost: u32,
    ) -> Result<StreamGuard, ApiError> {
        // Ensure cost is at least 1.
        let cost_nz = NonZeroU32::new(cost.max(1)).expect("max(1) is non-zero");

        // 1. Check active stream count for this IP.
        {
            let streams = self.active_streams.lock().expect("streams lock poisoned");
            let count = streams.get(&client_ip).copied().unwrap_or(0);
            if count >= self.per_ip_max_streams as usize {
                metrics::counter!("prism_rate_limit_hits_total", "scope" => "max_streams").increment(1);
                return Err(ApiError::RateLimited {
                    retry_after_secs: 1,
                });
            }
        }

        // 2. Per-IP rate limit.
        check_keyed_cost(&self.per_ip, &client_ip, cost_nz, "per_ip")?;

        // 3. Per-target rate limit (check each target).
        for key in target_keys {
            check_keyed_cost(&self.per_target, key, cost_nz, "per_target")?;
        }

        // 4. Global rate limit.
        check_direct_cost(&self.global, cost_nz)?;

        // All checks passed — increment active stream count.
        let guard = StreamGuard::new(Arc::clone(&self.active_streams), client_ip);
        Ok(guard)
    }
}

/// Check a keyed rate limiter and convert rejection to `ApiError`.
fn check_keyed_cost<K: std::hash::Hash + Eq + Clone>(
    limiter: &KeyedLimiter<K>,
    key: &K,
    cost: NonZeroU32,
    scope: &'static str,
) -> Result<(), ApiError> {
    let result = match limiter.check_key_n(key, cost) {
        Ok(Ok(())) => return Ok(()),
        Ok(Err(not_until)) => {
            let wait = not_until.wait_time_from(governor::clock::Clock::now(&DefaultClock::default()));
            wait.as_secs()
        }
        // InsufficientCapacity: cost exceeds burst size entirely.
        // The request can never fit in a single burst — report a 60s retry.
        Err(_) => 60,
    };
    metrics::counter!("prism_rate_limit_hits_total", "scope" => scope).increment(1);
    Err(ApiError::RateLimited {
        retry_after_secs: result.max(1),
    })
}

/// Check the global (direct/unkeyed) rate limiter and convert rejection to `ApiError`.
fn check_direct_cost(
    limiter: &RateLimiter<NotKeyed, InMemoryState, DefaultClock>,
    cost: NonZeroU32,
) -> Result<(), ApiError> {
    let result = match limiter.check_n(cost) {
        Ok(Ok(())) => return Ok(()),
        Ok(Err(not_until)) => {
            let wait = not_until.wait_time_from(governor::clock::Clock::now(&DefaultClock::default()));
            wait.as_secs()
        }
        Err(_) => 60,
    };
    metrics::counter!("prism_rate_limit_hits_total", "scope" => "global").increment(1);
    Err(ApiError::RateLimited {
        retry_after_secs: result.max(1),
    })
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
            per_ip_burst: 10,
            per_target_per_minute: 30,
            global_per_minute: 500,
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

        let guard = state.check_query_cost(ip, &targets, 4);
        assert!(guard.is_ok());
    }

    #[test]
    fn rejects_when_per_ip_exhausted() {
        let state = RateLimitState::new(&test_config());
        let ip: IpAddr = "198.51.100.1".parse().unwrap();
        let targets = vec!["cloudflare".to_string()];

        // Burst is 10 — first call with cost 10 should succeed.
        assert!(state.check_query_cost(ip, &targets, 10).is_ok());
        // Second call should be rejected (burst exhausted).
        assert!(state.check_query_cost(ip, &targets, 1).is_err());
    }

    #[test]
    fn different_ips_have_independent_per_ip_budgets() {
        let state = RateLimitState::new(&test_config());
        let ip1: IpAddr = "198.51.100.1".parse().unwrap();
        let ip2: IpAddr = "198.51.100.2".parse().unwrap();
        // Use different targets so per-target limits don't interfere.
        let targets1 = vec!["cloudflare".to_string()];
        let targets2 = vec!["google".to_string()];

        assert!(state.check_query_cost(ip1, &targets1, 10).is_ok());
        // ip2 has its own per-IP budget.
        assert!(state.check_query_cost(ip2, &targets2, 10).is_ok());
    }

    #[test]
    fn rejects_when_max_streams_exceeded() {
        let state = RateLimitState::new(&test_config());
        let ip: IpAddr = "198.51.100.1".parse().unwrap();
        let targets = vec!["cloudflare".to_string()];

        // Hold 3 guards (max_streams = 3).
        let _g1 = state.check_query_cost(ip, &targets, 1).unwrap();
        let _g2 = state.check_query_cost(ip, &targets, 1).unwrap();
        let _g3 = state.check_query_cost(ip, &targets, 1).unwrap();

        // 4th should be rejected.
        assert!(state.check_query_cost(ip, &targets, 1).is_err());
    }

    #[test]
    fn stream_guard_decrements_on_drop() {
        let state = RateLimitState::new(&test_config());
        let ip: IpAddr = "198.51.100.1".parse().unwrap();
        let targets = vec!["cloudflare".to_string()];

        let g1 = state.check_query_cost(ip, &targets, 1).unwrap();
        let _g2 = state.check_query_cost(ip, &targets, 1).unwrap();
        let _g3 = state.check_query_cost(ip, &targets, 1).unwrap();

        // At max streams — 4th rejected.
        assert!(state.check_query_cost(ip, &targets, 1).is_err());

        // Drop one guard — now 4th should succeed.
        drop(g1);
        assert!(state.check_query_cost(ip, &targets, 1).is_ok());
    }

    #[test]
    fn cost_calculation_matches_query_shape() {
        // 4 record types × 2 servers = cost 8
        let state = RateLimitState::new(&test_config());
        let ip: IpAddr = "198.51.100.1".parse().unwrap();
        let targets = vec!["cloudflare".to_string(), "google".to_string()];

        // Burst is 10, cost 8 should succeed.
        assert!(state.check_query_cost(ip, &targets, 8).is_ok());
        // Remaining burst is 2, cost 4 should fail.
        assert!(state.check_query_cost(ip, &targets, 4).is_err());
    }
}
