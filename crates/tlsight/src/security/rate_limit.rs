//! GCRA-based rate limiting using the `governor` crate.
//!
//! Two independent rate limiters enforce the SDD §8.2 budget model:
//!
//! - **Per-IP**: Limits total inspection cost per client IP per minute.
//! - **Per-target**: Limits total inspection cost per target hostname per minute.
//!
//! Inspection cost is computed as `ports * inspected_ips` — the number of individual
//! TLS handshakes that will be issued.

use std::net::IpAddr;
use std::num::NonZeroU32;

use governor::Quota;
use governor::RateLimiter;
use governor::clock::DefaultClock;

use crate::config::LimitsConfig;
use crate::error::AppError;

/// Keyed rate limiter type alias for readability.
type KeyedLimiter<K> =
    RateLimiter<K, governor::state::keyed::DefaultKeyedStateStore<K>, DefaultClock>;

/// Rate limiting state shared across all request handlers.
pub struct RateLimitState {
    per_ip: KeyedLimiter<IpAddr>,
    per_target: KeyedLimiter<String>,
}

impl RateLimitState {
    /// Build rate limiters from configuration values.
    ///
    /// Note: Governor's `DefaultKeyedStateStore` (DashMap) never evicts entries.
    /// Memory grows ~50 bytes per unique IP/target key. This is acceptable for
    /// production workloads — a server restart clears all state.
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

        Self { per_ip, per_target }
    }

    /// Check whether a request with the given cost is allowed.
    ///
    /// Returns `Ok(())` if allowed, or `Err(AppError::RateLimited)` if rejected.
    /// Checks per-IP first, then per-target.
    pub fn check_cost(&self, client_ip: IpAddr, hostname: &str, cost: u32) -> Result<(), AppError> {
        let cost_nz = NonZeroU32::new(cost.max(1)).expect("max(1) is non-zero");

        // Per-IP check
        check_keyed_cost(&self.per_ip, &client_ip, cost_nz, "per_ip")?;

        // Per-target check
        check_keyed_cost(
            &self.per_target,
            &hostname.to_lowercase(),
            cost_nz,
            "per_target",
        )?;

        Ok(())
    }
}

/// Check a keyed rate limiter and convert rejection to `AppError`.
fn check_keyed_cost<K: std::hash::Hash + Eq + Clone>(
    limiter: &KeyedLimiter<K>,
    key: &K,
    cost: NonZeroU32,
    scope: &'static str,
) -> Result<(), AppError> {
    let result = match limiter.check_key_n(key, cost) {
        Ok(Ok(())) => return Ok(()),
        Ok(Err(not_until)) => {
            let wait =
                not_until.wait_time_from(governor::clock::Clock::now(&DefaultClock::default()));
            wait.as_secs()
        }
        // InsufficientCapacity: cost exceeds burst size entirely.
        // The request can never fit in a single burst — report a 60s retry.
        Err(_) => 60,
    };
    metrics::counter!("tlsight_rate_limit_hits_total", "scope" => scope).increment(1);
    Err(AppError::RateLimited {
        retry_after_secs: result.max(1),
        scope,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> LimitsConfig {
        LimitsConfig {
            per_ip_per_minute: 30,
            per_ip_burst: 10,
            per_target_per_minute: 60,
            per_target_burst: 20,
            max_concurrent_connections: 256,
            max_concurrent_handshakes: 10,
            handshake_timeout_secs: 5,
            request_timeout_secs: 15,
            max_ports: 5,
            max_ips_per_hostname: 10,
            max_domain_length: 253,
        }
    }

    #[test]
    fn allows_request_within_budget() {
        let state = RateLimitState::new(&test_config());
        let ip: IpAddr = "198.51.100.1".parse().unwrap();

        // Cost 4 (e.g. 2 ports * 2 IPs) should succeed.
        assert!(state.check_cost(ip, "example.com", 4).is_ok());
    }

    #[test]
    fn rejects_when_per_ip_exhausted() {
        let state = RateLimitState::new(&test_config());
        let ip: IpAddr = "198.51.100.1".parse().unwrap();

        // Burst is 10 — first call with cost 10 should succeed.
        assert!(state.check_cost(ip, "example.com", 10).is_ok());
        // Second call should be rejected (burst exhausted).
        assert!(state.check_cost(ip, "example.com", 1).is_err());
    }

    #[test]
    fn different_ips_have_independent_budgets() {
        let state = RateLimitState::new(&test_config());
        let ip1: IpAddr = "198.51.100.1".parse().unwrap();
        let ip2: IpAddr = "198.51.100.2".parse().unwrap();

        // Use different hostnames to avoid per-target interference.
        assert!(state.check_cost(ip1, "a.example.com", 10).is_ok());
        // ip2 has its own per-IP budget.
        assert!(state.check_cost(ip2, "b.example.com", 10).is_ok());
    }

    #[test]
    fn rejects_when_per_target_exhausted() {
        let state = RateLimitState::new(&test_config());
        let ip1: IpAddr = "198.51.100.1".parse().unwrap();
        let ip2: IpAddr = "198.51.100.2".parse().unwrap();

        // Per-target burst is 20. Use different IPs to avoid per-IP limits.
        assert!(state.check_cost(ip1, "example.com", 10).is_ok());
        assert!(state.check_cost(ip2, "example.com", 10).is_ok());
        // Third IP, same target — per-target budget should be exhausted.
        let ip3: IpAddr = "198.51.100.3".parse().unwrap();
        assert!(state.check_cost(ip3, "example.com", 1).is_err());
    }

    #[test]
    fn cost_exceeding_burst_is_rejected() {
        let state = RateLimitState::new(&test_config());
        let ip: IpAddr = "198.51.100.1".parse().unwrap();

        // Per-IP burst is 10 — cost 11 exceeds it entirely (InsufficientCapacity).
        assert!(state.check_cost(ip, "example.com", 11).is_err());
    }

    #[test]
    fn zero_cost_treated_as_one() {
        let state = RateLimitState::new(&test_config());
        let ip: IpAddr = "198.51.100.1".parse().unwrap();

        // Cost 0 is clamped to 1.
        assert!(state.check_cost(ip, "example.com", 0).is_ok());
    }

    #[test]
    fn hostname_is_case_insensitive() {
        let state = RateLimitState::new(&test_config());
        let ip: IpAddr = "198.51.100.1".parse().unwrap();

        // Both map to the same per-target key.
        assert!(state.check_cost(ip, "Example.COM", 10).is_ok());
        // Same target (lowercased), burst should be exhausted for per-IP.
        assert!(state.check_cost(ip, "example.com", 1).is_err());
    }

    #[test]
    fn rate_limited_error_has_scope() {
        let state = RateLimitState::new(&test_config());
        let ip: IpAddr = "198.51.100.1".parse().unwrap();

        // Exhaust per-IP budget.
        let _ = state.check_cost(ip, "example.com", 10);
        let err = state.check_cost(ip, "example.com", 1).unwrap_err();
        match err {
            AppError::RateLimited { scope, .. } => {
                assert_eq!(scope, "per_ip");
            }
            other => panic!("expected RateLimited, got: {other:?}"),
        }
    }
}
