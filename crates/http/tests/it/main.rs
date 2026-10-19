//! http's integration tests, one module per file, linked as one test binary `it`
//! (v2-ci-speed: one binary links once; ~400 separate ones dominated CI). A new test file
//! goes here and gets its `mod` line; `tests/repo/test_test_binaries.sh` holds the layout.

mod contract_golden;
mod contract_status;
mod cors_probe;
mod evidence;
mod evidence_probes;
mod evidence_reading;
mod history_volatile;
mod model_status;
mod module_target_limit;
mod module_v3;
mod outbound_redirects;
mod probe_error_kinds;
mod probe_outcomes;
mod probe_outcomes_landing;
mod redirect_results_table;
