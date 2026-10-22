//! engine's integration tests, one module per file, linked as one test binary `it`
//! (v2-ci-speed: one binary links once; ~400 separate ones dominated CI). A new test file
//! goes here and gets its `mod` line; `tests/repo/test_test_binaries.sh` holds the layout.

mod catalogue;
mod catalogue_for_protocol;
mod domain_run_ip_enrichment_errors;
mod domain_run_model_status;
mod domain_run_score;
mod domain_run_scoring_regression;
mod evidence_path;
mod facts_blocks;
mod facts_blocks_section_wins;
mod hard_fail_precedence;
mod history;
mod history_fixtures;
mod history_fixtures_story;
mod no_address_sections;
mod profile;
mod profile_v4;
mod registry;
mod run;
mod schema_derives;
mod score;
mod score_run;
mod score_v1_cases;
mod section_kind;
mod strong_signature_finding;
mod traits;
mod unmeasured_scoring;
mod wants_facts;
