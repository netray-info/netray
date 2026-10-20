//! tls's integration tests, one module per file, linked as one test binary `it`
//! (v2-ci-speed: one binary links once; ~400 separate ones dominated CI). A new test file
//! goes here and gets its `mod` line; `tests/repo/test_test_binaries.sh` holds the layout.

mod caa_critical;
mod caa_wildcard;
mod chain_completeness;
mod config_reload_tolerance;
mod contract;
mod contract_golden;
mod contract_status;
mod error_codes;
mod evidence;
mod evidence_cert_detail;
mod evidence_detail_register;
mod evidence_handshake_detail;
mod evidence_ocsp_live;
mod evidence_refs;
mod evidence_unmeasured;
mod integration;
mod lookup_errors_unmeasured;
mod model_status;
mod module_limits;
mod module_v3;
mod port_results_table;
mod probe_address;
mod probe_alerts;
mod probe_checks;
mod probe_classify;
mod probe_client_hello;
mod probe_close_evidence;
mod probe_evidence;
mod reachable_unmeasured;
mod signature_verdict;
mod translate_ports;
mod trust_anchor;
