use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use governor::Quota;
use governor::RateLimiter;
use netray_common::rate_limit::{KeyedLimiter, check_keyed_cost};

use crate::error::AppError;

/// Per-IP and per-target quotas, per minute with a burst.
#[derive(Debug, Clone, Copy)]
pub struct Quotas {
    pub per_ip_per_minute: u32,
    pub per_ip_burst: u32,
    pub per_target_per_minute: u32,
    pub per_target_burst: u32,
}

/// The metric prefix and the two scope labels a limiter reports its rejections under.
#[derive(Debug, Clone, Copy)]
pub struct Scopes {
    pub metrics_prefix: &'static str,
    pub per_ip: &'static str,
    pub per_target: &'static str,
}

const PRUNE_EVERY: Duration = Duration::from_secs(60);

pub struct RateLimitState {
    per_ip: KeyedLimiter<IpAddr>,
    per_target: KeyedLimiter<String>,
    scopes: Scopes,
    last_prune: Mutex<Instant>,
}

impl RateLimitState {
    /// Build the limiters from validated, non-zero quotas.
    pub fn scoped(quotas: &Quotas, scopes: Scopes) -> Self {
        let per_ip = RateLimiter::keyed(
            Quota::per_minute(
                NonZeroU32::new(quotas.per_ip_per_minute).expect("validated non-zero"),
            )
            .allow_burst(NonZeroU32::new(quotas.per_ip_burst).expect("validated non-zero")),
        );

        let per_target = RateLimiter::keyed(
            Quota::per_minute(
                NonZeroU32::new(quotas.per_target_per_minute).expect("validated non-zero"),
            )
            .allow_burst(NonZeroU32::new(quotas.per_target_burst).expect("validated non-zero")),
        );

        Self {
            per_ip,
            per_target,
            scopes,
            last_prune: Mutex::new(Instant::now()),
        }
    }

    /// Drop the keys of both maps that have refilled, at most once per minute.
    pub fn prune_if_due(&self) {
        let Ok(mut last) = self.last_prune.try_lock() else {
            return;
        };
        if last.elapsed() < PRUNE_EVERY {
            return;
        }
        *last = Instant::now();
        self.per_ip.retain_recent();
        self.per_ip.shrink_to_fit();
        self.per_target.retain_recent();
        self.per_target.shrink_to_fit();
    }

    pub fn check(&self, client_ip: IpAddr, hostname: &str) -> Result<(), AppError> {
        let cost = NonZeroU32::new(1).expect("1 is non-zero");

        check_keyed_cost(
            &self.per_ip,
            &client_ip,
            cost,
            self.scopes.per_ip,
            self.scopes.metrics_prefix,
        )
        .map_err(|r| AppError::RateLimited {
            retry_after_secs: r.retry_after_secs,
            scope: r.scope,
        })?;

        check_keyed_cost(
            &self.per_target,
            &hostname.to_lowercase(),
            cost,
            self.scopes.per_target,
            self.scopes.metrics_prefix,
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

    fn test_state() -> RateLimitState {
        RateLimitState::scoped(
            &Quotas {
                per_ip_per_minute: 10,
                per_ip_burst: 5,
                per_target_per_minute: 30,
                per_target_burst: 10,
            },
            Scopes {
                metrics_prefix: "spectra",
                per_ip: "per_ip",
                per_target: "per_target",
            },
        )
    }

    #[test]
    fn allows_request_within_budget() {
        let state = test_state();
        let ip: IpAddr = "198.51.100.1".parse().unwrap();
        assert!(state.check(ip, "example.com").is_ok());
    }

    #[test]
    fn rejects_when_per_ip_exhausted() {
        let state = test_state();
        let ip: IpAddr = "198.51.100.1".parse().unwrap();
        for _ in 0..5 {
            assert!(state.check(ip, "example.com").is_ok());
        }
        assert!(state.check(ip, "example.com").is_err());
    }

    #[test]
    fn per_target_exhausted_blocks_different_ip() {
        let state = test_state(); // per_target_burst = 10
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
        let state = test_state();
        let ip1: IpAddr = "198.51.100.1".parse().unwrap();
        let ip2: IpAddr = "198.51.100.2".parse().unwrap();
        for _ in 0..5 {
            assert!(state.check(ip1, "a.example.com").is_ok());
        }
        assert!(state.check(ip2, "b.example.com").is_ok());
    }
}
