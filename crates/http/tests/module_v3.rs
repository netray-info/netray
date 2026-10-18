// V2 catalogue Phase 3, requirement 3(b) (criterion C3): the HTTP module emits exactly the 12
// `http.*` catalogue IDs in every run. Same fixture and entry points as `module.rs`.

use std::collections::BTreeSet;

use netray_engine::{Module, SectionOutcome, catalogue};
use netray_http::inspect::assembler::InspectResponse;
use netray_http::{HttpModule, ModuleConfig, translate};
use netray_model::{CheckResult, Protocol, Status};
use serde_json::{Value, json};

const SPECTRA_JSON: &str = include_str!("../../../tests/fixtures/contracts/spectra-inspect.json");

fn catalogue_http_ids() -> Vec<String> {
    catalogue()
        .entries()
        .iter()
        .filter(|e| e.protocol == Protocol::Http)
        .map(|e| e.id.to_string())
        .collect()
}

fn measured(outcome: SectionOutcome) -> Vec<CheckResult> {
    match outcome {
        SectionOutcome::Measured { checks, .. } => checks,
        other => panic!("expected Measured, got {other:?}"),
    }
}

fn find<'a>(checks: &'a [CheckResult], id: &str) -> &'a CheckResult {
    checks
        .iter()
        .find(|c| c.id.to_string() == id)
        .unwrap_or_else(|| panic!("no CheckResult {id}"))
}

fn variant(mutate: impl FnOnce(&mut Value)) -> InspectResponse {
    let mut v: Value = serde_json::from_str(SPECTRA_JSON).unwrap();
    mutate(&mut v);
    serde_json::from_value(v).unwrap()
}

#[test]
fn http_module_emits_exactly_the_twelve_catalogue_ids() {
    let want = catalogue_http_ids();
    assert_eq!(want.len(), 12, "catalogue http.* IDs");

    let resp: InspectResponse = serde_json::from_str(SPECTRA_JSON).unwrap();
    let checks = measured(translate(&resp));
    let got: Vec<String> = checks.iter().map(|c| c.id.to_string()).collect();
    assert_eq!(got.len(), 12, "each ID once: {got:?}");
    assert_eq!(
        got.iter().collect::<BTreeSet<_>>(),
        want.iter().collect::<BTreeSet<_>>(),
        "Measured IDs"
    );

    let cfg: ModuleConfig = toml::from_str("[inspect]\nrequest_timeout_secs = 5").unwrap();
    let module = HttpModule::new(cfg, None).expect("module builds");
    let declared: Vec<String> = module.checks().iter().map(|c| c.to_string()).collect();
    assert_eq!(declared.len(), 12, "checks() each ID once: {declared:?}");
    assert_eq!(
        declared.iter().collect::<BTreeSet<_>>(),
        want.iter().collect::<BTreeSet<_>>(),
        "checks()"
    );
}

#[test]
fn http_golden_reported_checks_carry_spectra_status_others_not_reported() {
    let resp: InspectResponse = serde_json::from_str(SPECTRA_JSON).unwrap();
    let checks = measured(translate(&resp));

    // Spectra's quality checks in the golden: hsts pass, csp warn, x_frame_options pass,
    // cors pass, info_leakage pass; http_upgrade redirects_to_https = true.
    for (id, status) in [
        ("http.https_redirect", Status::Pass),
        ("http.hsts", Status::Pass),
        ("http.csp", Status::Warn),
        ("http.x_frame_options", Status::Pass),
        ("http.cors", Status::Pass),
        ("http.info_leakage", Status::Pass),
    ] {
        assert_eq!(find(&checks, id).status, status, "{id} status");
    }

    for id in [
        "http.cookie_secure",
        "http.x_content_type_options",
        "http.referrer_policy",
        "http.permissions_policy",
        "http.deprecated_headers",
        "http.caching",
    ] {
        let c = find(&checks, id);
        assert_eq!(c.status, Status::NotApplicable, "{id} status");
        assert_eq!(
            c.findings,
            vec!["not reported".to_string()],
            "{id} findings"
        );
    }
}

#[test]
fn http_https_redirect_derives_from_http_upgrade() {
    let resp = variant(|v| v["http_upgrade"]["redirects_to_https"] = json!(false));
    let checks = measured(translate(&resp));
    assert_eq!(find(&checks, "http.https_redirect").status, Status::Fail);
}

#[test]
fn http_redirect_limit_is_a_finding_on_https_redirect() {
    let mut resp = variant(|v| {
        v["quality"]["checks"].as_array_mut().unwrap().push(json!({
            "label": "Redirect Limit",
            "message": "Redirect limit reached (10)",
            "name": "redirect_limit",
            "status": "warn"
        }));
    });
    resp.redirect_limit_reached = Some(10);

    let checks = measured(translate(&resp));
    let ids: Vec<String> = checks.iter().map(|c| c.id.to_string()).collect();
    assert!(
        !ids.iter().any(|i| i == "http.redirect_limit"),
        "no http.redirect_limit: {ids:?}"
    );
    assert_eq!(ids.len(), 12, "still the 12 catalogue IDs: {ids:?}");

    let c = find(&checks, "http.https_redirect");
    assert!(
        c.findings
            .iter()
            .any(|f| f.to_lowercase().contains("redirect limit")),
        "https_redirect findings: {:?}",
        c.findings
    );
}
