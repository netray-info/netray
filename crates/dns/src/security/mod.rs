//! The DNS inspection's guards: query restrictions ([`QueryPolicy`]) and rate limiting
//! ([`RateLimitState`]).

pub mod query_policy;
pub mod rate_limit;

pub use query_policy::QueryPolicy;
pub use rate_limit::RateLimitState;
