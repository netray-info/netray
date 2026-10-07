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
    pub fn new(config: &RateLimitConfig) -> Result<Self, String> {
        let rate = parse_rate(&config.per_ip)?;
        let quota = NonZeroU32::new(rate).ok_or_else(|| format!("rate must be > 0, got {rate}"))?;
        let per_ip = RateLimiter::keyed(Quota::per_minute(quota).allow_burst(quota));
        Ok(Self { per_ip })
    }

    pub fn check(&self, client_ip: IpAddr) -> Result<(), MailError> {
        let cost = NonZeroU32::new(1).expect("1 is non-zero");
        check_keyed_cost(&self.per_ip, &client_ip, cost, "per_ip", "beacon").map_err(|r| {
            MailError::RateLimited {
                retry_after_secs: r.retry_after_secs,
                scope: r.scope,
            }
        })?;
        Ok(())
    }
}

/// Parse a rate string like `"10/min"` into its numeric component.
///
/// The unit suffix (`/min`, `/hour`, etc.) is currently informational — the
/// rate limiter always applies the returned number as a per-minute quota.
/// Returns `Err` on empty/non-numeric input and on a zero numerator (zero
/// would panic inside `NonZeroU32::new`).
fn parse_rate(s: &str) -> Result<u32, String> {
    let numeric = s
        .split('/')
        .next()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or_else(|| format!("rate string '{s}' has no numeric component"))?;

    let value: u32 = numeric
        .parse()
        .map_err(|_| format!("rate string '{s}' is not a valid unsigned integer"))?;

    if value == 0 {
        return Err(format!("rate string '{s}' resolves to zero"));
    }

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_per_minute_rate() {
        assert_eq!(parse_rate("10/min").unwrap(), 10);
    }

    #[test]
    fn parses_hourly_rate_as_raw_number() {
        // Unit suffix is informational — parse_rate returns the numerator as-is.
        assert_eq!(parse_rate("100/hour").unwrap(), 100);
    }

    #[test]
    fn parses_bare_number() {
        assert_eq!(parse_rate("42").unwrap(), 42);
    }

    #[test]
    fn trims_whitespace_around_numerator() {
        assert_eq!(parse_rate("  7  /min").unwrap(), 7);
    }

    #[test]
    fn rejects_non_numeric_numerator() {
        let err = parse_rate("abc/min").unwrap_err();
        assert!(err.contains("valid unsigned integer"), "got: {err}");
    }

    #[test]
    fn rejects_zero_numerator() {
        let err = parse_rate("0/min").unwrap_err();
        assert!(err.contains("zero"), "got: {err}");
    }

    #[test]
    fn rejects_empty_string() {
        assert!(parse_rate("").is_err());
    }

    #[test]
    fn rejects_empty_numerator() {
        assert!(parse_rate("/min").is_err());
    }

    #[test]
    fn rate_limit_state_propagates_parse_error() {
        let cfg = RateLimitConfig {
            per_ip: "abc/min".to_string(),
        };
        assert!(RateLimitState::new(&cfg).is_err());
    }

    #[test]
    fn rate_limit_state_rejects_zero() {
        let cfg = RateLimitConfig {
            per_ip: "0/min".to_string(),
        };
        assert!(RateLimitState::new(&cfg).is_err());
    }

    #[test]
    fn rate_limit_state_accepts_valid() {
        let cfg = RateLimitConfig {
            per_ip: "10/min".to_string(),
        };
        assert!(RateLimitState::new(&cfg).is_ok());
    }
}
