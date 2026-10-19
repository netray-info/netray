//! GCRA rate limiting with the `governor` crate: a per-IP limiter keyed by client address and one
//! global limiter shared by every client. Copied from lens's `security/rate_limit.rs`.

use std::net::IpAddr;
use std::num::NonZeroU32;

use governor::clock::DefaultClock;
use governor::state::InMemoryState;
use governor::state::NotKeyed;
use governor::{Quota, RateLimiter};
use netray_common::rate_limit::{
    KeyedLimiter, RateLimitRejection, check_direct_cost, check_keyed_cost,
};

/// Per-IP GCRA rate limiter keyed by `IpAddr`.
pub struct PerIpRateLimiter {
    inner: KeyedLimiter<IpAddr>,
}

impl PerIpRateLimiter {
    /// Panics on a zero rate or burst; the configuration refuses both.
    pub fn new(per_minute: u32, burst: u32) -> Self {
        let inner = RateLimiter::keyed(
            Quota::per_minute(NonZeroU32::new(per_minute).expect("validated non-zero"))
                .allow_burst(NonZeroU32::new(burst).expect("validated non-zero")),
        );
        Self { inner }
    }
}

/// Global (unkeyed) GCRA rate limiter shared across all clients.
pub struct GlobalRateLimiter {
    inner: RateLimiter<NotKeyed, InMemoryState, DefaultClock>,
}

impl GlobalRateLimiter {
    /// Panics on a zero rate or burst; the configuration refuses both.
    pub fn new(per_minute: u32, burst: u32) -> Self {
        let inner = RateLimiter::direct(
            Quota::per_minute(NonZeroU32::new(per_minute).expect("validated non-zero"))
                .allow_burst(NonZeroU32::new(burst).expect("validated non-zero")),
        );
        Self { inner }
    }
}

/// Checks the per-IP, then the global limiter for `ip`: the rejection of the first that refuses
/// (scope `per_ip` or `global`).
pub fn check_rate_limit(
    per_ip: &PerIpRateLimiter,
    global: &GlobalRateLimiter,
    ip: IpAddr,
) -> Result<(), RateLimitRejection> {
    let cost = NonZeroU32::new(1).unwrap();
    check_keyed_cost(&per_ip.inner, &ip, cost, "per_ip", "netray")?;
    check_direct_cost(&global.inner, cost, "netray")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limiters() -> (PerIpRateLimiter, GlobalRateLimiter) {
        (
            PerIpRateLimiter::new(10, 3),
            GlobalRateLimiter::new(100, 20),
        )
    }

    #[test]
    fn allows_request_within_budget() {
        let (per_ip, global) = limiters();
        let ip: IpAddr = "203.0.113.1".parse().unwrap();
        assert!(check_rate_limit(&per_ip, &global, ip).is_ok());
    }

    #[test]
    fn rejects_when_per_ip_burst_exhausted() {
        let (per_ip, global) = limiters();
        let ip: IpAddr = "203.0.113.1".parse().unwrap();
        for _ in 0..3 {
            assert!(check_rate_limit(&per_ip, &global, ip).is_ok());
        }
        let err = check_rate_limit(&per_ip, &global, ip).unwrap_err();
        assert_eq!(err.scope, "per_ip");
        assert!(err.retry_after_secs >= 1, "retry_after_secs must be >= 1");
    }

    #[test]
    fn different_ips_have_independent_per_ip_budgets() {
        let (per_ip, global) = limiters();
        let ip1: IpAddr = "203.0.113.1".parse().unwrap();
        let ip2: IpAddr = "203.0.113.2".parse().unwrap();
        for _ in 0..3 {
            assert!(check_rate_limit(&per_ip, &global, ip1).is_ok());
        }
        assert!(check_rate_limit(&per_ip, &global, ip2).is_ok());
    }
}
