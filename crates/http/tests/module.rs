// V2 Phase 1, requirement 4: the HTTP module, its config and the pure translation moved from
// lens's `backends/http.rs`. Needs `required-features = ["testing"]` on this test target and
// `netray-engine` and `toml` as dev-dependencies of netray-http.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use netray_engine::{Domain, Facts, Module, RunContext, RunOptions, SectionOutcome};
use netray_http::inspect::assembler::InspectResponse;
use netray_http::{HttpModule, ModuleConfig, testing, translate};
use netray_model::evidence::{Block, Shape, Value as EvValue};
use netray_model::{Protocol, Status};
use serde_json::Value;

const SPECTRA_JSON: &str = include_str!("../../../tests/fixtures/contracts/spectra-inspect.json");

fn contracts(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn measured(outcome: SectionOutcome) -> (Vec<netray_model::CheckResult>, Vec<Block>) {
    match outcome {
        SectionOutcome::Measured { checks, evidence } => (checks, evidence),
        other => panic!("expected Measured, got {other:?}"),
    }
}

fn status_of(verdict: &str) -> Status {
    match verdict {
        "pass" => Status::Pass,
        "warn" => Status::Warn,
        "fail" => Status::Fail,
        "skip" => Status::NotApplicable,
        other => panic!("unknown lens verdict {other}"),
    }
}

#[test]
fn translate_equals_lens_http_section() {
    let resp: InspectResponse = serde_json::from_str(SPECTRA_JSON).unwrap();
    let (checks, evidence) = measured(translate(&resp));

    let lens = contracts("lens-full-healthy.json");
    let http = lens["http"].as_object().expect("lens http section");
    let lens_checks = http["checks"].as_array().unwrap();

    assert_eq!(checks.len(), lens_checks.len(), "check count");
    for lc in lens_checks {
        let name = lc["name"].as_str().unwrap();
        let id = format!("http.{name}");
        let got = checks
            .iter()
            .find(|c| c.id.to_string() == id)
            .unwrap_or_else(|| panic!("no CheckResult {id}"));
        assert_eq!(
            got.status,
            status_of(lc["verdict"].as_str().unwrap()),
            "{id} status"
        );
        let want: Vec<String> = lc
            .get("messages")
            .and_then(Value::as_array)
            .map(|m| m.iter().map(|s| s.as_str().unwrap().to_string()).collect())
            .unwrap_or_default();
        assert_eq!(got.findings, want, "{id} findings");
    }

    // Lens's http keys as lens derives them from the evidence. Not asserted: `detail_url`
    // (built from lens's ecosystem base URL and the domain, not from the spectra response) and
    // `status` (the section status lens computes from its scoring, not a translation output).
    let fields = |id: &str| -> Vec<(String, EvValue)> {
        match evidence
            .iter()
            .find(|b| b.id.to_string() == id)
            .map(|b| &b.shape)
        {
            Some(Shape::Fields(f)) => f.clone(),
            other => panic!("{id} is not a fields block: {other:?}"),
        }
    };
    let server = fields("http.server");
    let json_of = |key: &str| -> Value {
        match server.iter().find(|(k, _)| k == key).map(|(_, v)| v) {
            Some(EvValue::Text(t)) => Value::from(t.clone()),
            Some(EvValue::Int(i)) => Value::from(*i),
            Some(EvValue::Addr(a)) => Value::from(a.to_string()),
            other => panic!("http.server/{key}: {other:?}"),
        }
    };
    for (lens_key, key) in [
        ("http_version", "http_version"),
        ("status_code", "status_code"),
        ("server_ip", "address"),
        ("server_network_type", "network_type"),
        ("server_org", "org"),
    ] {
        assert_eq!(
            json_of(key),
            http[lens_key],
            "http.server/{key} as {lens_key}"
        );
    }
    let https = if fields("http.port80_probe")
        .contains(&("redirects_to_https".into(), EvValue::Bool(true)))
    {
        "✓"
    } else {
        "✗"
    };
    let symbol = |name: &str| match checks
        .iter()
        .find(|c| c.id.to_string() == format!("http.{name}"))
        .map(|c| c.status)
    {
        Some(Status::Pass) => "✓",
        Some(Status::Fail) => "✗",
        Some(Status::Warn) => "~",
        _ => "-",
    };
    let headline = format!(
        "HTTPS {https}  HSTS {}  CSP {}  CORS {}",
        symbol("hsts"),
        symbol("csp"),
        symbol("cors")
    );
    assert_eq!(Value::from(headline), http["headline"], "headline");
}

#[test]
fn module_config_refuses_unknown_key_and_accepts_inspect_timeout() {
    let bad: toml::Table = toml::from_str("bogus = 1").unwrap();
    assert!(
        bad.try_into::<ModuleConfig>().is_err(),
        "unknown key refused"
    );

    let bad_nested: toml::Table = toml::from_str("[inspect]\nbogus = 1").unwrap();
    assert!(bad_nested.try_into::<ModuleConfig>().is_err());

    let good: toml::Table = toml::from_str("[inspect]\nrequest_timeout_secs = 5").unwrap();
    assert!(good.try_into::<ModuleConfig>().is_ok());
}

#[test]
fn http_module_declares_http_checks() {
    let cfg: ModuleConfig = toml::from_str("[inspect]\nrequest_timeout_secs = 5").unwrap();
    let module = HttpModule::new(cfg).expect("module builds");
    assert_eq!(module.protocol(), Protocol::Http);
    assert!(!module.checks().is_empty());
    for id in module.checks() {
        assert!(id.to_string().starts_with("http."), "{id}");
        assert_eq!(id.protocol(), Protocol::Http);
    }
}

#[tokio::test]
async fn golden_module_run_equals_translate() {
    let resp: InspectResponse = serde_json::from_str(SPECTRA_JSON).unwrap();
    let want = translate(&resp);

    let module = testing::golden_module(SPECTRA_JSON);
    assert_eq!(module.protocol(), Protocol::Http);
    let ctx = RunContext {
        deadline: Instant::now() + Duration::from_secs(5),
        domain: Domain::new("example.com"),
        options: RunOptions::default(),
    };
    let got = module.run(&ctx, &Facts::default()).await;
    assert_eq!(got, want);
}
