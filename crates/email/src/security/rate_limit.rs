use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use governor::Quota;
use governor::RateLimiter;
use netray_common::rate_limit::{KeyedLimiter, check_keyed_cost};

use crate::error::MailError;

/// Per-IP quota, per minute with a burst.
#[derive(Debug, Clone, Copy)]
pub struct Quotas {
    pub per_ip_per_minute: u32,
    pub per_ip_burst: u32,
}

/// The metric prefix and the scope label a limiter reports its rejections under.
#[derive(Debug, Clone, Copy)]
pub struct Scopes {
    pub metrics_prefix: &'static str,
    pub per_ip: &'static str,
}

const PRUNE_EVERY: Duration = Duration::from_secs(60);

pub struct RateLimitState {
    per_ip: KeyedLimiter<IpAddr>,
    scopes: Scopes,
    last_prune: Mutex<Instant>,
}

impl RateLimitState {
    /// Build the limiter from a validated, non-zero quota.
    pub fn scoped(quotas: &Quotas, scopes: Scopes) -> Self {
        let per_ip = RateLimiter::keyed(
            Quota::per_minute(
                NonZeroU32::new(quotas.per_ip_per_minute).expect("validated non-zero"),
            )
            .allow_burst(NonZeroU32::new(quotas.per_ip_burst).expect("validated non-zero")),
        );
        Self {
            per_ip,
            scopes,
            last_prune: Mutex::new(Instant::now()),
        }
    }

    /// Drop the keys that have refilled, at most once per minute.
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
    }

    pub fn check(&self, client_ip: IpAddr) -> Result<(), MailError> {
        let cost = NonZeroU32::new(1).expect("1 is non-zero");
        check_keyed_cost(
            &self.per_ip,
            &client_ip,
            cost,
            self.scopes.per_ip,
            self.scopes.metrics_prefix,
        )
        .map_err(|r| MailError::RateLimited {
            retry_after_secs: r.retry_after_secs,
            scope: r.scope,
        })?;
        Ok(())
    }
}
