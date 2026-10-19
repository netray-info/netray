//! V2 Phase 1, requirement 5: lens takes the HTTP section from the engine registry. With a
//! stub HTTP module in the registry, lens's check path runs it in-process and the section's
//! checks are the module's; there is no `http_url` to call and no other section's module in the
//! registry, so the section cannot have come from a request.
//!
//! Copied from crates/lens/tests/http_in_process.rs (spec v2-cutover, Phase 6); nothing dropped
//! (the check runs through `netray_engine::domain_run::run_check_streaming` over the apex
//! state's registry and plan).

mod common;

use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use netray_engine::domain_run::{CheckInput, run_check_streaming};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, Registry, RunContext, SectionOutcome};
use netray_model::{CheckId, CheckResult, Protocol, Status};

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
                evidence: vec![],
            }
        })
    }
}

#[tokio::test]
async fn runs_the_registry_http_module_in_process() {
    let ran_for = Arc::new(Mutex::new(Vec::new()));
    let registry = Registry::new()
        .with(Box::new(RecordingHttp {
            ran_for: ran_for.clone(),
        }))
        .with_facts(Box::new(common::StubFacts(Facts::default())));
    let mut config = common::production_config();
    config.cache_enabled = false;
    let plan = common::production_plan(&registry);
    let state = common::state_with(config, registry, plan);

    let input = CheckInput {
        domain: "example.com".to_string(),
        dkim_selectors: None,
    };
    let (out, _) = tokio::time::timeout(
        Duration::from_secs(10),
        run_check_streaming(
            &state.registry,
            &state.plan,
            state.resolve_budget,
            &state.scoring_profile,
            input,
            Duration::from_secs(5),
            |_| {},
        ),
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
