use std::net::IpAddr;
use std::num::NonZeroU32;

use governor::Quota;
use governor::RateLimiter;
use netray_common::rate_limit::{KeyedLimiter, check_keyed_cost};

use crate::config::RateLimitConfig;
use crate::error::MailError;

pub struct RateLimitState {
    per_ip: KeyedLimiter<IpAddr>,
}

impl RateLimitState {
    pub fn new(config: &RateLimitConfig) -> Self {
        let rate = parse_rate(&config.per_ip);
        let per_ip = RateLimiter::keyed(
            Quota::per_minute(NonZeroU32::new(rate).expect("rate must be > 0"))
                .allow_burst(NonZeroU32::new(rate).expect("rate must be > 0")),
        );
        Self { per_ip }
    }

    pub fn check(&self, client_ip: IpAddr) -> Result<(), MailError> {
        let cost = NonZeroU32::new(1).expect("1 is non-zero");
        check_keyed_cost(&self.per_ip, &client_ip, cost, "per_ip", "mail-inspector").map_err(
            |r| MailError::RateLimited {
                retry_after_secs: r.retry_after_secs,
                scope: r.scope,
            },
        )?;
        Ok(())
    }
}

/// Parse rate string like "10/min" into the numeric part.
fn parse_rate(s: &str) -> u32 {
    s.split('/')
        .next()
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or(10)
}
