//! The TLS inspection's guards: target restrictions ([`target_policy`]) and rate limiting
//! ([`RateLimitState`]).

pub mod rate_limit;
pub mod target_policy;

pub use rate_limit::RateLimitState;
