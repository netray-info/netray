// V2 Phase 4, requirements 13 and C4: the TLS module, its config and the pure translation moved
// from lens's `backends/tls.rs`. Needs `required-features = ["testing"]` on this test target,
// `netray-engine` and `toml` as dev-dependencies of netray-tls, and `Deserialize` on
// `routes::InspectResponse` and everything it contains (it is Serialize-only today; `input_mode`
// is `&'static str` and must become a deserializable type, e.g. `String`).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use netray_engine::{Domain, Facts, Module, RunContext, RunOptions, SectionOutcome};
use netray_model::{Protocol, Status};
use netray_tls::routes::InspectResponse;
use netray_tls::{ModuleConfig, TlsModule, testing, translate};
use serde_json::Value;

fn read(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts")
        .join(name);
    std::fs::read_to_string(path).unwrap()
}

fn contracts(name: &str) -> Value {
    serde_json::from_str(&read(name)).unwrap()
}

fn measured(outcome: SectionOutcome) -> (Vec<netray_model::CheckResult>, Value) {
    match outcome {
        SectionOutcome::Measured {
            checks,
            presentation,
        } => (checks, presentation),
        other => panic!("expected Measured, got {other:?}"),
    }
}

/// lens's verdict word for a status, as lens's module adapter maps it.
fn verdict_of(status: Status) -> &'static str {
    match status {
        Status::Pass => "pass",
        Status::Warn => "warn",
        Status::Fail => "fail",
        Status::NotApplicable | Status::NotTested | Status::Unmeasured => "skip",
        other => panic!("unmapped status {other:?}"),
    }
}

/// (tlsight golden, lens-full golden that serves it, whether tls_reachable must be NotTested).
const CASES: [(&str, &str, bool); 3] = [
    ("tlsight-inspect.json", "lens-full-healthy.json", false),
    (
        "tlsight-unreachable.json",
        "lens-full-http-only.json",
        false,
    ),
    (
        "tlsight-not-tested.json",
        "lens-full-no-weighted-tls.json",
        true,
    ),
];

#[test]
fn translate_equals_lens_tls_section() {
    for (golden, lens_file, not_tested) in CASES {
        let resp: InspectResponse = serde_json::from_str(&read(golden)).unwrap();
        let (checks, presentation) = measured(translate(&resp));

        let lens = contracts(lens_file);
        let tls = lens["tls"].as_object().expect("lens tls section");
        let lens_checks = tls["checks"].as_array().unwrap();

        assert_eq!(checks.len(), lens_checks.len(), "{golden}: check count");
        for lc in lens_checks {
            let name = lc["name"].as_str().unwrap();
            let id = format!("tls.{name}");
            let got = checks
                .iter()
                .find(|c| c.id.to_string() == id)
                .unwrap_or_else(|| panic!("{golden}: no CheckResult {id}"));
            assert_eq!(
                verdict_of(got.status),
                lc["verdict"].as_str().unwrap(),
                "{golden}: {id} verdict"
            );
            let want: Vec<String> = lc
                .get("messages")
                .and_then(Value::as_array)
                .map(|m| m.iter().map(|s| s.as_str().unwrap().to_string()).collect())
                .unwrap_or_default();
            assert_eq!(got.findings, want, "{golden}: {id} findings");
            if not_tested && name == "tls_reachable" {
                assert_eq!(got.status, Status::NotTested, "{golden}: {id} status");
            }
        }

        // Translation-derived key of lens's tls object. Not asserted: `detail_url` (lens's
        // ecosystem base URL and the domain) and `status` (lens's scoring, not a translation
        // output).
        assert_eq!(
            presentation["headline"], tls["headline"],
            "{golden}: headline"
        );
    }
}

#[test]
fn module_config_refuses_unknown_key_and_allow_blocked_targets() {
    let bad: toml::Table = toml::from_str("bogus = 1").unwrap();
    assert!(
        bad.try_into::<ModuleConfig>().is_err(),
        "unknown key refused"
    );

    let bad_nested: toml::Table = toml::from_str("[limits]\nbogus = 1").unwrap();
    assert!(bad_nested.try_into::<ModuleConfig>().is_err());

    for text in [
        "allow_blocked_targets = true",
        "[limits]\nallow_blocked_targets = true",
    ] {
        let t: toml::Table = toml::from_str(text).unwrap();
        assert!(
            t.try_into::<ModuleConfig>().is_err(),
            "allow_blocked_targets refused: {text}"
        );
    }

    let good: toml::Table =
        toml::from_str("[limits]\nmax_ports = 3\nper_target_per_minute = 2\nper_target_burst = 2")
            .unwrap();
    assert!(good.try_into::<ModuleConfig>().is_ok());
}

#[tokio::test]
async fn tls_module_declares_tls_checks() {
    let cfg: ModuleConfig = toml::from_str("[limits]\nmax_ports = 3").unwrap();
    let module = TlsModule::new(cfg).await.expect("module builds");
    assert_eq!(module.protocol(), Protocol::Tls);
    assert!(!module.checks().is_empty());
    for id in module.checks() {
        assert!(id.to_string().starts_with("tls."), "{id}");
        assert_eq!(id.protocol(), Protocol::Tls);
    }
}

#[tokio::test]
async fn golden_module_run_equals_translate() {
    for (golden, _, _) in CASES {
        let json = read(golden);
        let resp: InspectResponse = serde_json::from_str(&json).unwrap();
        let want = translate(&resp);

        let module = testing::golden_module(&json);
        assert_eq!(module.protocol(), Protocol::Tls);
        let ctx = RunContext {
            deadline: Instant::now() + Duration::from_secs(5),
            domain: Domain::new("example.com"),
            options: RunOptions::default(),
        };
        let got = module.run(&ctx, &Facts::default()).await;
        assert_eq!(got, want, "{golden}");
    }
}
