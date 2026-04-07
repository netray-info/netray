//! Security middleware for the lens web service.
//!
//! - **Rate limiting** (governor GCRA) — via [`rate_limit`]
//! - **Client IP extraction** — via [`ip_extract`]
//! - **Target IP policy** — via [`target_policy`]

pub mod ip_extract;
pub mod rate_limit;
pub mod target_policy;

pub use ip_extract::extract_client_ip;
pub use rate_limit::{GlobalRateLimiter, PerIpRateLimiter, check_rate_limit};
