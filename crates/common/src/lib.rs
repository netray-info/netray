//! Shared utilities for the netray.info service ecosystem.
//!
//! This crate provides cross-cutting concerns used by multiple backend services:
//!
//! - [`config`] -- Strict layered configuration loading (TOML file + prefixed environment variables).
//! - [`ip_extract`] -- Extract real client IP from proxy headers with trusted-proxy CIDR matching.
//! - [`error`] -- Structured JSON error responses via the [`error::ApiError`] trait.
//! - [`rate_limit`] -- Keyed and global rate limiting wrappers around `governor`.
//! - [`security_headers`] -- Axum middleware for CSP, HSTS, and other security headers.
//!
//! # Example
//!
//! ```rust
//! use netray_common::ip_extract::IpExtractor;
//!
//! let extractor = IpExtractor::new(&["10.0.0.0/8".to_string()]);
//! // extractor.extract(&headers, peer_addr) returns the real client IP
//! ```

#[cfg(feature = "backend")]
pub mod backend;
pub mod config;
#[cfg(feature = "cors")]
pub mod cors;
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
pub mod rate_limit;
pub mod security_headers;
#[cfg(feature = "server")]
pub mod server;
pub mod target_policy;
#[cfg(feature = "telemetry")]
pub mod telemetry;
