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
use netray_common::rate_limit::{check_keyed_cost, KeyedLimiter};

use crate::config::LimitsConfig;
use crate::error::AppError;

/// Rate limiting state shared across all request handlers.
pub struct RateLimitState {
    per_ip: KeyedLimiter<IpAddr>,
    per_target: KeyedLimiter<String>,
    per_ip_burst: u32,
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

        Self {
            per_ip,
            per_target,
            per_ip_burst: config.per_ip_burst,
        }
    }

    /// Return the per-IP burst size as an upper bound for cap-and-warn budget calculation.
    ///
    /// Governor doesn't expose remaining capacity, so we use the burst size as a
    /// conservative estimate. The actual `check_cost` call will reject if insufficient.
    pub fn remaining_budget(&self, _client_ip: IpAddr) -> u32 {
        self.per_ip_burst
    }

    /// Check whether a request with the given cost is allowed.
    ///
    /// Returns `Ok(())` if allowed, or `Err(AppError::RateLimited)` if rejected.
    /// Checks per-IP first, then per-target.
    pub fn check_cost(&self, client_ip: IpAddr, hostname: &str, cost: u32) -> Result<(), AppError> {
        let cost_nz = NonZeroU32::new(cost.max(1)).expect("max(1) is non-zero");

        // Per-IP check
        check_keyed_cost(&self.per_ip, &client_ip, cost_nz, "per_ip", "tlsight")
            .map_err(|r| AppError::RateLimited {
                retry_after_secs: r.retry_after_secs,
                scope: r.scope,
            })?;

        // Per-target check
        check_keyed_cost(
            &self.per_target,
            &hostname.to_lowercase(),
            cost_nz,
            "per_target",
            "tlsight",
        )
        .map_err(|r| AppError::RateLimited {
            retry_after_secs: r.retry_after_secs,
            scope: r.scope,
        })?;

        Ok(())
    }
}

/// Select representative IPs when the full set exceeds the rate budget.
///
/// Prefers one IPv4 + one IPv6, fills remaining slots in DNS order.
/// Returns `(selected, skipped)`.
pub fn select_representative_ips(ips: &[IpAddr], budget: usize) -> (Vec<IpAddr>, Vec<IpAddr>) {
    if ips.len() <= budget {
        return (ips.to_vec(), Vec::new());
    }

    if budget == 0 {
        return (Vec::new(), ips.to_vec());
    }

    let mut selected = Vec::with_capacity(budget);
    let mut remaining: Vec<IpAddr> = ips.to_vec();

    // Pick first v4
    if let Some(pos) = remaining.iter().position(|ip| ip.is_ipv4()) {
        selected.push(remaining.remove(pos));
    }

    // Pick first v6 (if budget allows)
    if selected.len() < budget
        && let Some(pos) = remaining.iter().position(|ip| ip.is_ipv6())
    {
        selected.push(remaining.remove(pos));
    }

    // Fill remaining budget in DNS order
    while selected.len() < budget && !remaining.is_empty() {
        selected.push(remaining.remove(0));
    }

    (selected, remaining)
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
            max_ports: 7,
            max_ips_per_hostname: 10,
            max_domain_length: 253,
            allow_blocked_targets: false,
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
    fn select_representative_all_fit() {
        let ips: Vec<IpAddr> = vec![
            "198.51.100.1".parse().unwrap(),
            "198.51.100.2".parse().unwrap(),
        ];
        let (selected, skipped) = select_representative_ips(&ips, 5);
        assert_eq!(selected.len(), 2);
        assert!(skipped.is_empty());
    }

    #[test]
    fn select_representative_prefers_v4_and_v6() {
        let ips: Vec<IpAddr> = vec![
            "198.51.100.1".parse().unwrap(),
            "198.51.100.2".parse().unwrap(),
            "2001:db8::1".parse().unwrap(),
            "2001:db8::2".parse().unwrap(),
        ];
        let (selected, skipped) = select_representative_ips(&ips, 2);
        assert_eq!(selected.len(), 2);
        assert!(selected[0].is_ipv4());
        assert!(selected[1].is_ipv6());
        assert_eq!(skipped.len(), 2);
    }

    #[test]
    fn select_representative_budget_one() {
        let ips: Vec<IpAddr> = vec![
            "198.51.100.1".parse().unwrap(),
            "2001:db8::1".parse().unwrap(),
        ];
        let (selected, skipped) = select_representative_ips(&ips, 1);
        assert_eq!(selected.len(), 1);
        assert!(selected[0].is_ipv4());
        assert_eq!(skipped.len(), 1);
    }

    #[test]
    fn select_representative_budget_zero() {
        let ips: Vec<IpAddr> = vec!["198.51.100.1".parse().unwrap()];
        let (selected, skipped) = select_representative_ips(&ips, 0);
        assert!(selected.is_empty());
        assert_eq!(skipped.len(), 1);
    }

    #[test]
    fn select_representative_all_ipv6_budget_one() {
        let ips: Vec<IpAddr> = vec![
            "2001:db8::1".parse().unwrap(),
            "2001:db8::2".parse().unwrap(),
            "2001:db8::3".parse().unwrap(),
        ];
        let (selected, skipped) = select_representative_ips(&ips, 1);
        assert_eq!(selected.len(), 1);
        assert_eq!(skipped.len(), 2);
        // The selected IP must be an IPv6 address
        assert!(matches!(selected[0], IpAddr::V6(_)));
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
