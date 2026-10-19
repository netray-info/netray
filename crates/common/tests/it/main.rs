//! common's integration tests, one module per file, linked as one test binary `it`
//! (v2-ci-speed: one binary links once; ~400 separate ones dominated CI). A new test file
//! goes here and gets its `mod` line; `tests/repo/test_test_binaries.sh` holds the layout.

mod config_loader;
mod ip_extract_no_cf;
mod prefetch;
mod target_policy_results_table;
