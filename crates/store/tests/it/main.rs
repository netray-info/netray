//! store's integration tests, one module per file, linked as one test binary `it`
//! (v2-ci-speed: one binary links once; ~400 separate ones dominated CI). A new test file
//! goes here and gets its `mod` line; `tests/repo/test_test_binaries.sh` holds the layout.

mod duration;
mod findings_identical;
mod head;
mod record;
mod record_metrics;
mod schema;
mod seed;
mod sweep;
