//! Security middleware for the lens web service.
//!
//! - **Rate limiting** (governor GCRA) — via [`rate_limit`]
//!
//! Client IP extraction uses `netray_common::ip_extract::IpExtractor`
//! (held in `AppState`) with the real `ConnectInfo` peer.

pub mod rate_limit;

pub use rate_limit::{GlobalRateLimiter, PerIpRateLimiter, check_rate_limit};
