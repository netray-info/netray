//! The engine's two seams: `Module` (one protocol's checks) and `FactsProvider` (shared DNS facts),
//! exercised through stubs behind trait objects, using only `netray_engine` and `netray_model`.

use std::net::Ipv4Addr;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use netray_engine::{
    BoxFuture, Domain, EvidencePath, Facts, FactsProvider, Module, ResolveError, RunContext,
    RunOptions, SectionOutcome,
};
use netray_model::{CheckId, CheckResult, Protocol, Status};

static VOLATILE: [EvidencePath; 1] = [EvidencePath("tls.handshake_ms")];

fn tls_reachable() -> CheckId {
    CheckId::parse("tls.tls_reachable").unwrap()
}

fn result() -> CheckResult {
    CheckResult {
        id: tls_reachable(),
        status: Status::Pass,
        findings: vec![],
        evidence: vec![],
    }
}

static CHECKS: LazyLock<Vec<CheckId>> = LazyLock::new(|| vec![tls_reachable()]);

struct StubModule;

impl Module for StubModule {
    fn protocol(&self) -> Protocol {
        Protocol::Tls
    }
    fn checks(&self) -> &'static [CheckId] {
        &CHECKS
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        &VOLATILE
    }
    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async {
            SectionOutcome::Measured {
                checks: vec![result()],
                presentation: serde_json::Value::Null,
            }
        })
    }
}

struct StubProvider;

impl FactsProvider for StubProvider {
    fn resolve<'a>(
        &'a self,
        _ctx: &'a RunContext,
        domain: &'a Domain,
    ) -> BoxFuture<'a, Result<Facts, ResolveError>> {
        Box::pin(async move {
            if domain.as_str() == "example.com" {
                Ok(Facts {
                    a: vec![Ipv4Addr::new(192, 0, 2, 1)],
                    ..Facts::default()
                })
            } else {
                Err(ResolveError(format!("cannot resolve {}", domain.as_str())))
            }
        })
    }
}

fn ctx() -> RunContext {
    RunContext {
        deadline: Instant::now() + Duration::from_secs(5),
        domain: Domain::new("example.com"),
        options: RunOptions::default(),
    }
}

#[tokio::test]
async fn boxed_module_declares_and_returns_its_outcome() {
    let module: Box<dyn Module> = Box::new(StubModule);
    assert_eq!(module.protocol(), Protocol::Tls);
    assert_eq!(module.checks(), [tls_reachable()].as_slice());
    assert_eq!(module.volatile(), &[EvidencePath("tls.handshake_ms")]);

    let outcome = module.run(&ctx(), &Facts::default()).await;
    assert_eq!(
        outcome,
        SectionOutcome::Measured {
            checks: vec![result()],
            presentation: serde_json::Value::Null,
        }
    );
}

#[tokio::test]
async fn facts_provider_resolves_one_domain_and_refuses_another() {
    let provider: Box<dyn FactsProvider> = Box::new(StubProvider);
    let ctx = ctx();

    let facts = provider
        .resolve(&ctx, &Domain::new("example.com"))
        .await
        .unwrap();
    assert_eq!(facts.a, vec![Ipv4Addr::new(192, 0, 2, 1)]);
    assert!(facts.aaaa.is_empty());

    let err = provider
        .resolve(&ctx, &Domain::new("missing.example.com"))
        .await
        .unwrap_err();
    assert_eq!(
        err,
        ResolveError("cannot resolve missing.example.com".into())
    );
}
