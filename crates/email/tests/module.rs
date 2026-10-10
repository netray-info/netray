// V2 Phase 2, requirement 7: the email module, its config and the pure translation moved from
// lens's `backends/email.rs`. Needs `[[test]] name = "module"` with `required-features =
// ["testing"]` on this test target, a `testing = []` feature on netray-email, and `netray-engine`
// and `toml` as dev-dependencies of netray-email.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use netray_email::quality::types::SseEvent;
use netray_email::{EmailModule, ModuleConfig, testing, translate};
use netray_engine::{Domain, Facts, Module, RunContext, RunOptions, SectionOutcome};
use netray_model::{Protocol, Status};
use serde_json::Value;

fn contracts_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts")
        .join(name)
}

fn contracts(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(contracts_path(name)).unwrap()).unwrap()
}

fn sse(name: &str) -> String {
    std::fs::read_to_string(contracts_path(name)).unwrap()
}

fn parse_events(sse: &str) -> Vec<SseEvent> {
    sse.lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(|d| serde_json::from_str(d.trim()).expect("SseEvent"))
        .collect()
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

/// (beacon golden, lens full-output golden, what lens rendered)
const FIXTURES: &[(&str, &str, Expect)] = &[
    ("beacon.sse", "lens-full-healthy.json", Expect::Measured),
    ("beacon-no-mx.sse", "lens-full-no-mx.json", Expect::Measured),
    (
        "beacon-mx-cname.sse",
        "lens-full-mx-cname.json",
        Expect::Measured,
    ),
    (
        "beacon-null-mx.sse",
        "lens-full-null-mx.json",
        Expect::Measured,
    ),
    (
        "beacon-sending-no-dkim.sse",
        "lens-full-beacon-sending-no-dkim.json",
        Expect::Measured,
    ),
    (
        "beacon-timeout.sse",
        "lens-full-beacon-timeout.json",
        Expect::TimedOut,
    ),
    // Lens rendered "backend error": a category that did not complete.
    (
        "beacon-partial.sse",
        "lens-full-beacon-partial.json",
        Expect::Incomplete,
    ),
];

#[derive(Clone, Copy)]
enum Expect {
    Measured,
    TimedOut,
    Incomplete,
}

#[test]
fn translate_equals_lens_email_section_for_every_fixture() {
    for (sse_name, lens_name, expect) in FIXTURES {
        let events = parse_events(&sse(sse_name));
        let outcome = translate(&events);
        let lens = contracts(lens_name);
        let email = lens["email"].as_object().expect("lens email section");

        match (expect, outcome) {
            (Expect::TimedOut, SectionOutcome::TimedOut) => {
                assert_eq!(email["headline"], "timeout", "{sse_name}");
            }
            (Expect::Incomplete, SectionOutcome::Incomplete { .. }) => {
                assert_eq!(email["headline"], "backend error", "{sse_name}");
            }
            (
                Expect::Measured,
                SectionOutcome::Measured {
                    checks,
                    presentation,
                },
            ) => {
                let lens_checks = email["checks"].as_array().unwrap();
                assert_eq!(checks.len(), lens_checks.len(), "{sse_name}: check count");
                for lc in lens_checks {
                    let name = lc["name"].as_str().unwrap();
                    let id = format!("email.{name}");
                    let got = checks
                        .iter()
                        .find(|c| c.id.to_string() == id)
                        .unwrap_or_else(|| panic!("{sse_name}: no CheckResult {id}"));
                    assert_eq!(
                        got.status,
                        status_of(lc["verdict"].as_str().unwrap()),
                        "{sse_name}: {id} status"
                    );
                    let want: Vec<String> = lc
                        .get("messages")
                        .and_then(Value::as_array)
                        .map(|m| m.iter().map(|s| s.as_str().unwrap().to_string()).collect())
                        .unwrap_or_default();
                    assert_eq!(got.findings, want, "{sse_name}: {id} findings");
                }

                // Translation-derived keys of lens's email object. Not asserted: `detail_url`
                // (built from lens's public URL and the domain, not from beacon's events) and
                // `status` (the section status lens computes from its scoring).
                for key in ["headline", "grade"] {
                    assert_eq!(
                        presentation[key], email[key],
                        "{sse_name}: presentation.{key}"
                    );
                }
            }
            (_, other) => panic!("{sse_name}: unexpected outcome {other:?}"),
        }
    }
}

#[test]
fn module_config_refuses_unknown_key_and_accepts_dkim_max_user_selectors() {
    let bad: toml::Table = toml::from_str("bogus = 1").unwrap();
    assert!(
        bad.try_into::<ModuleConfig>().is_err(),
        "unknown key refused"
    );

    let bad_nested: toml::Table = toml::from_str("[dkim]\nbogus = 1").unwrap();
    assert!(bad_nested.try_into::<ModuleConfig>().is_err());

    let good: toml::Table = toml::from_str("[dkim]\nmax_user_selectors = 3").unwrap();
    assert!(good.try_into::<ModuleConfig>().is_ok());
}

#[tokio::test]
async fn email_module_declares_email_checks() {
    let cfg: ModuleConfig = toml::from_str("[dkim]\nmax_user_selectors = 3").unwrap();
    let module = EmailModule::new(cfg).await.expect("module builds");
    assert_eq!(module.protocol(), Protocol::Email);
    assert!(!module.checks().is_empty());
    for id in module.checks() {
        assert!(id.to_string().starts_with("email."), "{id}");
        assert_eq!(id.protocol(), Protocol::Email);
    }
}

#[tokio::test]
async fn golden_module_run_equals_translate() {
    let golden = sse("beacon.sse");
    let want = translate(&parse_events(&golden));

    let module = testing::golden_module(&golden);
    assert_eq!(module.protocol(), Protocol::Email);
    let ctx = RunContext {
        deadline: Instant::now() + Duration::from_secs(5),
        domain: Domain::new("example.com"),
        options: RunOptions::default(),
    };
    let got = module.run(&ctx, &Facts::default()).await;
    assert_eq!(got, want);
}
