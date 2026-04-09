use std::net::IpAddr;
use std::num::NonZeroU32;

use governor::Quota;
use governor::RateLimiter;
use netray_common::rate_limit::{KeyedLimiter, check_keyed_cost};

use crate::config::LimitsConfig;
use crate::error::AppError;

pub struct RateLimitState {
    per_ip: KeyedLimiter<IpAddr>,
    per_target: KeyedLimiter<String>,
}

impl RateLimitState {
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

    pub fn check(&self, client_ip: IpAddr, hostname: &str) -> Result<(), AppError> {
        let cost = NonZeroU32::new(1).expect("1 is non-zero");

        check_keyed_cost(&self.per_ip, &client_ip, cost, "per_ip", "spectra").map_err(|r| {
            AppError::RateLimited {
                retry_after_secs: r.retry_after_secs,
                scope: r.scope,
            }
        })?;

        check_keyed_cost(
            &self.per_target,
            &hostname.to_lowercase(),
            cost,
            "per_target",
            "spectra",
        )
        .map_err(|r| AppError::RateLimited {
            retry_after_secs: r.retry_after_secs,
            scope: r.scope,
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> LimitsConfig {
        LimitsConfig {
            per_ip_per_minute: 10,
            per_ip_burst: 5,
            per_target_per_minute: 30,
            per_target_burst: 10,
            max_concurrent_connections: 256,
        }
    }

    #[test]
    fn allows_request_within_budget() {
        let state = RateLimitState::new(&test_config());
        let ip: IpAddr = "198.51.100.1".parse().unwrap();
        assert!(state.check(ip, "example.com").is_ok());
    }

    #[test]
    fn rejects_when_per_ip_exhausted() {
        let state = RateLimitState::new(&test_config());
        let ip: IpAddr = "198.51.100.1".parse().unwrap();
        for _ in 0..5 {
            assert!(state.check(ip, "example.com").is_ok());
        }
        assert!(state.check(ip, "example.com").is_err());
    }

    #[test]
    fn per_target_exhausted_blocks_different_ip() {
        let state = RateLimitState::new(&test_config()); // per_target_burst = 10
        // Use 10 different source IPs (each within their own per-IP budget) to exhaust
        // the per-target burst for a single hostname.
        for i in 0u8..10 {
            let ip: IpAddr = format!("198.51.100.{}", 10 + i).parse().unwrap();
            let _ = state.check(ip, "target.example.com");
        }
        // A fresh IP — per-IP budget is untouched, but per-target is exhausted
        let new_ip: IpAddr = "198.51.101.1".parse().unwrap();
        assert!(state.check(new_ip, "target.example.com").is_err());
    }

    #[test]
    fn different_ips_independent() {
        let state = RateLimitState::new(&test_config());
        let ip1: IpAddr = "198.51.100.1".parse().unwrap();
        let ip2: IpAddr = "198.51.100.2".parse().unwrap();
        for _ in 0..5 {
            assert!(state.check(ip1, "a.example.com").is_ok());
        }
        assert!(state.check(ip2, "b.example.com").is_ok());
    }
}
