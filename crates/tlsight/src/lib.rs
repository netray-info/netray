// Library target for integration tests.
//
// Exposes the modules needed by tests in `tests/` without changing the binary
// entry point (`main.rs`). All items here are `pub` only for testing purposes.

pub mod config;
pub mod dns;
pub mod error;
pub mod input;
pub mod quality;
pub mod reload;
pub mod routes;
pub mod security;
pub mod state;
pub mod tls;
pub mod validate;

pub use netray_common::middleware::RequestId;
