// Library target for integration tests.
//
// Exposes the modules needed by tests in `tests/` without changing the binary
// entry point (`main.rs`). All items here are `pub` only for testing purposes.

pub mod api;
pub mod circuit_breaker;
pub mod config;
pub mod dns_dnssec;
pub mod dns_raw;
pub mod dns_trace;
pub mod error;
pub mod parser;
pub mod record_format;
pub mod reload;
pub mod result_cache;
pub mod security;

pub use netray_common::middleware::RequestId;
pub use netray_common::middleware::request_id as request_id_middleware;
