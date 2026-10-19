//! model's integration tests, one module per file, linked as one test binary `it`
//! (v2-ci-speed: one binary links once; ~400 separate ones dominated CI). A new test file
//! goes here and gets its `mod` line; `tests/repo/test_test_binaries.sh` holds the layout.

mod diff_evidence;
mod diff_presence;
mod diff_status;
mod evidence;
mod evidence_rules;
mod fingerprint;
mod model;
mod run;
mod run_check_fields;
mod volatile_block;
mod volatile_row_repeats;
