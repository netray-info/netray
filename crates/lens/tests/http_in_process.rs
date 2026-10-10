//! V2 Phase 1, requirement 5: lens takes the HTTP section from the engine registry. With a
//! stub HTTP module in the registry, lens's check path runs it in-process and the section's
//! checks are the module's; there is no `http_url` to call and no other section's module in the
//! registry, so the section cannot have come from a request.

mod common;

use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use lens::check::{CheckInput, run_check_with_deadline};
use lens::config::Config;
use lens::state::AppState;
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, Registry, RunContext, SectionOutcome};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use serde_json::json;

static CHECKS: LazyLock<Vec<CheckId>> =
    LazyLock::new(|| vec![CheckId::parse("http.https_redirect").unwrap()]);

/// An HTTP module that records the domain it ran for and answers one passing check.
struct RecordingHttp {
    ran_for: Arc<Mutex<Vec<String>>>,
}

impl Module for RecordingHttp {
    fn protocol(&self) -> Protocol {
        Protocol::Http
    }
    fn checks(&self) -> &'static [CheckId] {
        &CHECKS
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }
    fn run<'a>(&'a self, ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        self.ran_for
            .lock()
            .unwrap()
            .push(ctx.domain.as_str().to_string());
        Box::pin(async {
            SectionOutcome::Measured {
                checks: vec![CheckResult {
                    id: CheckId::parse("http.https_redirect").unwrap(),
                    status: Status::Pass,
                    findings: vec![],
                    evidence: vec![],
                }],
                presentation: json!({"headline": "stub module"}),
            }
        })
    }
}

#[tokio::test]
async fn lens_runs_the_registry_http_module_in_process() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    config.cache.enabled = false;
    config.snapshots.enabled = false;

    let ran_for = Arc::new(Mutex::new(Vec::new()));
    let registry = Registry::new()
        .with(Box::new(RecordingHttp {
            ran_for: ran_for.clone(),
        }))
        .with_facts(Box::new(common::StubFacts(Facts::default())));
    let state = AppState::with_registry(config, registry).unwrap();

    let input = CheckInput {
        domain: "example.com".to_string(),
        dkim_selectors: None,
        client_ip: None,
        request_id: None,
    };
    let out = tokio::time::timeout(
        Duration::from_secs(10),
        run_check_with_deadline(&state, input, Duration::from_secs(5)),
    )
    .await
    .expect("check finished");

    assert_eq!(
        *ran_for.lock().unwrap(),
        vec!["example.com".to_string()],
        "the registry's HTTP module runs once, for the checked domain"
    );
    let http = out.sections["http"]
        .as_ref()
        .unwrap_or_else(|e| panic!("http section errored: {e:?}"));
    let names: Vec<&str> = http.checks.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["https_redirect"],
        "the HTTP section's checks are the module's"
    );
}
