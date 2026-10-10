//! C5: a stub Module reads the domain and DKIM selectors from `RunContext` and returns
//! `Measured { checks, presentation }`; the `Registry` hands modules and facts out by protocol.

use std::net::Ipv4Addr;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use netray_engine::{
    BoxFuture, Domain, EvidencePath, Facts, FactsProvider, Module, Registry, ResolveError,
    RunContext, RunOptions, SectionOutcome,
};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use serde_json::json;

fn http_check() -> CheckId {
    CheckId::parse("http.https_reachable").unwrap()
}

fn result() -> CheckResult {
    CheckResult {
        id: http_check(),
        status: Status::Pass,
        findings: vec![],
        evidence: vec![],
    }
}

static CHECKS: LazyLock<Vec<CheckId>> = LazyLock::new(|| vec![http_check()]);

type Seen = Arc<Mutex<Option<(String, Option<Vec<String>>)>>>;

struct RecordingModule {
    seen: Seen,
}

impl Module for RecordingModule {
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
        *self.seen.lock().unwrap() = Some((
            ctx.domain.as_str().to_string(),
            ctx.options.dkim_selectors.clone(),
        ));
        Box::pin(async {
            SectionOutcome::Measured {
                checks: vec![result()],
                presentation: json!({"headline": "x"}),
            }
        })
    }
}

struct StubProvider;

impl FactsProvider for StubProvider {
    fn resolve<'a>(
        &'a self,
        _ctx: &'a RunContext,
        _domain: &'a Domain,
    ) -> BoxFuture<'a, Result<Facts, ResolveError>> {
        Box::pin(async {
            Ok(Facts {
                a: vec![Ipv4Addr::new(192, 0, 2, 1)],
                ..Facts::default()
            })
        })
    }
}

#[tokio::test]
async fn registry_runs_module_with_domain_and_selectors_from_context() {
    let seen: Seen = Arc::default();
    let registry = Registry::new().with(Box::new(RecordingModule { seen: seen.clone() }));

    let module = registry
        .module(Protocol::Http)
        .expect("http module registered");
    let ctx = RunContext {
        deadline: Instant::now() + Duration::from_secs(5),
        domain: Domain::new("example.com"),
        options: RunOptions {
            dkim_selectors: Some(vec!["s1".into(), "s2".into()]),
        },
    };
    let outcome = module.run(&ctx, &Facts::default()).await;

    assert_eq!(
        seen.lock().unwrap().clone(),
        Some((
            "example.com".to_string(),
            Some(vec!["s1".to_string(), "s2".to_string()])
        ))
    );
    assert_eq!(
        outcome,
        SectionOutcome::Measured {
            checks: vec![result()],
            presentation: json!({"headline": "x"}),
        }
    );
}

#[test]
fn registry_returns_none_for_unregistered_protocol_and_facts_when_set() {
    let registry = Registry::new().with(Box::new(RecordingModule {
        seen: Seen::default(),
    }));
    assert!(registry.module(Protocol::Dns).is_none());
    assert!(registry.facts().is_none());

    let registry = registry.with_facts(Box::new(StubProvider));
    assert!(registry.facts().is_some());
}
