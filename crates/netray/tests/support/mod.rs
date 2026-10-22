//! Startup helpers shared by the tests that spawn `netray serve` on ports picked by `free_port`.
//!
//! A picked port is free when it is picked, not when serve binds it: a parallel test can take it
//! in between. A test therefore accepts a server only once its own log shows both listeners
//! bound, and starts again on fresh ports when a bind failed (the CI flake on e2bb5899:
//! "failed to bind server address: Os { code: 98, kind: AddrInUse }").

use std::path::Path;

/// How many times a test starts serve before a bind failure fails the test.
pub const ATTEMPTS: u32 = 3;

fn log(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// The server and the metrics listener of this serve process are bound.
pub fn listening(path: &Path) -> bool {
    let log = log(path);
    log.contains("netray listening") && log.contains("metrics server listening")
}

/// A listener of this serve process failed to bind its port.
pub fn bind_failed(path: &Path) -> bool {
    let log = log(path);
    log.contains("AddrInUse")
        || log.contains("Address already in use")
        || log.contains("metrics server failed")
}
