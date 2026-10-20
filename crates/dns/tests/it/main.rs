//! dns's integration tests, one module per file, linked as one test binary `it`
//! (v2-ci-speed: one binary links once; ~400 separate ones dominated CI). A new test file
//! goes here and gets its `mod` line; `tests/repo/test_test_binaries.sh` holds the layout.

mod contract;
mod contract_golden;
mod evidence;
mod evidence_records;
mod facts_blocks;
mod facts_definite;
mod history_volatile;
mod integration;
mod job_page_blocks;
mod job_page_blocks_human;
mod job_summary;
mod lint_results_table;
mod lookup_errors_unmeasured;
mod model_status;
mod module_admitted;
mod module_input;
mod parse_cursor;
mod parse_system;
mod production_fixture;
mod result_cache;
mod system_resolvers;
