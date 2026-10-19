// v2-http-checks C2, C11, C12: `http.server` carries the inspection's duration as
// `inspection_ms`, not `response_ms`, and the volatile list follows the rename.

use netray_engine::{Module, SectionOutcome};
use netray_http::inspect::assembler::InspectResponse;
use netray_http::module::{REGISTER, translate};
use netray_http::{HttpModule, ModuleConfig};
use netray_model::evidence::{Shape, Value, validate};

const SPECTRA_JSON: &str =
    include_str!("../../../../tests/fixtures/contracts/spectra-inspect.json");

#[test]
fn server_block_holds_inspection_ms_equal_to_duration_ms() {
    let resp: InspectResponse = serde_json::from_str(SPECTRA_JSON).unwrap();
    let SectionOutcome::Measured { evidence, .. } = translate(&resp) else {
        panic!("expected Measured");
    };
    validate("http", &REGISTER, &evidence).expect("blocks fit the register");
    let server = evidence
        .iter()
        .find(|b| b.id.to_string() == "http.server")
        .expect("http.server block");
    let Shape::Fields(fields) = &server.shape else {
        panic!("http.server is not Fields");
    };
    let get = |k: &str| fields.iter().find(|(n, _)| n == k).map(|(_, v)| v);
    assert_eq!(get("inspection_ms"), Some(&Value::Duration(87)));
    assert_eq!(get("response_ms"), None);
}

#[test]
fn volatile_list_names_inspection_ms_not_response_ms() {
    let cfg: ModuleConfig = toml::from_str("[inspect]\nrequest_timeout_secs = 5").unwrap();
    let m = HttpModule::new(cfg).expect("module builds");
    let paths: Vec<&str> = m.volatile().iter().map(|p| p.0).collect();
    assert!(paths.contains(&"http.server/inspection_ms"), "{paths:?}");
    assert!(!paths.contains(&"http.server/response_ms"), "{paths:?}");
}
