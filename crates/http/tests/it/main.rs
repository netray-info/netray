//! http's integration tests, one module per file, linked as one test binary `it`
//! (v2-ci-speed: one binary links once; ~400 separate ones dominated CI). A new test file
//! goes here and gets its `mod` line; `tests/repo/test_test_binaries.sh` holds the layout.

mod caching_directives;
mod caching_evidence;
mod cdn_isolation_evidence;
mod contract_golden;
mod contract_status;
mod coop_check;
mod cors_probe;
mod csp_evidence;
mod csp_structured_issues;
mod evidence;
mod evidence_probes;
mod evidence_reading;
mod history_volatile;
mod hsts_evidence;
mod isolation_unreadable;
mod model_status;
mod module_target_limit;
mod module_v3;
mod outbound_redirects;
mod parsed_evidence_register;
mod probe_error_kinds;
mod probe_outcomes;
mod probe_outcomes_landing;
mod redirect_results_table;
mod referrer_policy_list;
mod server_inspection_ms;
mod set_cookie_rows;
