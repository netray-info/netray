//! email's integration tests, one module per file, linked as one test binary `it`
//! (v2-ci-speed: one binary links once; ~400 separate ones dominated CI). A new test file
//! goes here and gets its `mod` line; `tests/repo/test_test_binaries.sh` holds the layout.

mod contract_golden;
mod enrichment_integration;
mod history_volatile;
mod inspect_route;
mod inspect_selectors;
mod model_status;
mod module_input;
mod module_v3;
mod resolver_tcp_retry;
