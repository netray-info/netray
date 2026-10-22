//! Shared Rust of the netray workspace: what the modules, netray-web and the `netray` binary
//! use in common.
//!
//! - [`config`] -- strict layered configuration loading (TOML file + prefixed environment variables).
//! - [`domain`] -- the domain-name input rule (UTS #46 A-label).
//! - `fetch` (feature `fetch`) and [`target_policy`] -- outbound requests to user-supplied
//!   targets and the addresses they refuse.
//! - [`ip_extract`] -- the real client IP from proxy headers with trusted-proxy CIDR matching.
//! - [`error`] -- structured JSON error responses via the [`error::ApiError`] trait.
//! - [`rate_limit`] -- keyed and global rate limiting wrappers around `governor`.
//! - [`security_headers`] -- axum middleware for CSP, HSTS and the other security headers.
//! - `enrichment` (feature `enrichment`) -- in-process IP enrichment through the registered IP
//!   module.
//!
//! # Example
//!
//! ```rust
//! use netray_common::ip_extract::IpExtractor;
//!
//! let extractor = IpExtractor::new(&["10.0.0.0/8".to_string()]);
//! // extractor.extract(&headers, peer_addr) returns the real client IP
//! ```

pub mod config;
#[cfg(feature = "cors")]
pub mod cors;
pub mod domain;
pub mod ecosystem;
#[cfg(feature = "enrichment")]
pub mod enrichment;
pub mod error;
#[cfg(feature = "fetch")]
pub mod fetch;
pub mod ip_extract;
pub mod ip_filter;
pub mod metrics;
#[cfg(feature = "middleware")]
pub mod middleware;
pub mod prefetch;
pub mod rate_limit;
pub mod security_headers;
#[cfg(feature = "server")]
pub mod server;
pub mod target_policy;
#[cfg(feature = "telemetry")]
pub mod telemetry;
