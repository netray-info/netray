//! GCRA-based rate limiting using the `governor` crate.
//!
//! Two independent rate limiters enforce the budget model:
//!
//! - **Per-IP**: Limits requests per client IP per minute.
//! - **Global**: Single shared limit across all clients.

use std::net::IpAddr;
use std::num::NonZeroU32;

use governor::clock::DefaultClock;
use governor::state::InMemoryState;
use governor::state::NotKeyed;
use governor::{Quota, RateLimiter};
use netray_common::rate_limit::{
    KeyedLimiter, RateLimitRejection, check_direct_cost, check_keyed_cost,
};

use crate::config::RateLimitConfig;

/// Per-IP GCRA rate limiter keyed by `IpAddr`.
pub struct PerIpRateLimiter {
    inner: KeyedLimiter<IpAddr>,
}

impl PerIpRateLimiter {
    pub fn new(config: &RateLimitConfig) -> Self {
        let inner = RateLimiter::keyed(
            Quota::per_minute(
                NonZeroU32::new(config.per_ip_per_minute).expect("validated non-zero"),
            )
            .allow_burst(NonZeroU32::new(config.per_ip_burst).expect("validated non-zero")),
        );
        Self { inner }
    }
}

/// Global (unkeyed) GCRA rate limiter shared across all clients.
pub struct GlobalRateLimiter {
    inner: RateLimiter<NotKeyed, InMemoryState, DefaultClock>,
}

impl GlobalRateLimiter {
    pub fn new(config: &RateLimitConfig) -> Self {
        let inner = RateLimiter::direct(
            Quota::per_minute(
                NonZeroU32::new(config.global_per_minute).expect("validated non-zero"),
            )
            .allow_burst(NonZeroU32::new(config.global_burst).expect("validated non-zero")),
        );
        Self { inner }
    }
}

/// Check both per-IP and global rate limiters for the given client IP.
///
/// Returns `Ok(())` if both allow the request, or the rejection of the first limiter that
/// refuses it (scope `per_ip` or `global`). Per-IP is checked first.
pub fn check_rate_limit(
    per_ip: &PerIpRateLimiter,
    global: &GlobalRateLimiter,
    ip: IpAddr,
) -> Result<(), RateLimitRejection> {
    let cost = NonZeroU32::new(1).unwrap();
    check_keyed_cost(&per_ip.inner, &ip, cost, "per_ip", "lens")?;
    check_direct_cost(&global.inner, cost, "lens")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> RateLimitConfig {
        RateLimitConfig {
            per_ip_per_minute: 10,
            per_ip_burst: 3,
            global_per_minute: 100,
            global_burst: 20,
        }
    }

    #[test]
    fn allows_request_within_budget() {
        let cfg = test_config();
        let per_ip = PerIpRateLimiter::new(&cfg);
        let global = GlobalRateLimiter::new(&cfg);
        let ip: IpAddr = "203.0.113.1".parse().unwrap();

        assert!(check_rate_limit(&per_ip, &global, ip).is_ok());
    }

    #[test]
    fn rejects_when_per_ip_burst_exhausted() {
        let cfg = test_config();
        let per_ip = PerIpRateLimiter::new(&cfg);
        let global = GlobalRateLimiter::new(&cfg);
        let ip: IpAddr = "203.0.113.1".parse().unwrap();

        // Exhaust per-IP burst (3).
        assert!(check_rate_limit(&per_ip, &global, ip).is_ok());
        assert!(check_rate_limit(&per_ip, &global, ip).is_ok());
        assert!(check_rate_limit(&per_ip, &global, ip).is_ok());
        // 4th request must be rejected.
        let err = check_rate_limit(&per_ip, &global, ip).unwrap_err();
        // v2-admission R12/R16: the rejection carries its scope.
        assert_eq!(err.scope, "per_ip");
    }

    #[test]
    fn different_ips_have_independent_per_ip_budgets() {
        let cfg = test_config();
        let per_ip = PerIpRateLimiter::new(&cfg);
        let global = GlobalRateLimiter::new(&cfg);
        let ip1: IpAddr = "203.0.113.1".parse().unwrap();
        let ip2: IpAddr = "203.0.113.2".parse().unwrap();

        for _ in 0..3 {
            assert!(check_rate_limit(&per_ip, &global, ip1).is_ok());
        }
        // ip2 has its own budget — should still be allowed.
        assert!(check_rate_limit(&per_ip, &global, ip2).is_ok());
    }

    #[test]
    fn rate_limited_error_has_retry_after() {
        let cfg = test_config();
        let per_ip = PerIpRateLimiter::new(&cfg);
        let global = GlobalRateLimiter::new(&cfg);
        let ip: IpAddr = "203.0.113.1".parse().unwrap();

        for _ in 0..3 {
            let _ = check_rate_limit(&per_ip, &global, ip);
        }
        let err = check_rate_limit(&per_ip, &global, ip).unwrap_err();
        // v2-admission R12/R16: a RateLimitRejection, not an AppError.
        assert!(err.retry_after_secs >= 1, "retry_after_secs must be >= 1");
    }
}
