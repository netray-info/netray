//! R3(d): `Module::wants_facts` — a module that reads Facts but does not need addresses waits for
//! the resolve stage inside its own window; on a resolve error it still runs, with empty Facts, and
//! keeps its own outcome. A default module starts at once with empty Facts. Stub modules and a stub
//! FactsProvider only; real short sleeps (the run's deadlines are `std::time::Instant`).

use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use netray_engine::{
    BoxFuture, Domain, EvidencePath, Facts, FactsProvider, Module, Registry, ResolveError,
    RunContext, RunOptions, SectionEvent, SectionOutcome, run,
};
use netray_model::{CheckId, Protocol};
use serde_json::json;
use tokio::sync::mpsc;

const MS: fn(u64) -> Duration = Duration::from_millis;

fn measured() -> SectionOutcome {
    SectionOutcome::Measured {
        checks: vec![],
        presentation: json!({}),
        evidence: vec![],
    }
}

fn facts() -> Facts {
    Facts {
        a: vec![Ipv4Addr::new(192, 0, 2, 1)],
        caa: vec!["0 issue \"example.com\"".into()],
        ..Facts::default()
    }
}

struct Stub {
    wants_facts: bool,
    seen: Arc<Mutex<Vec<Facts>>>,
}

impl Stub {
    fn new(wants_facts: bool) -> Self {
        Stub {
            wants_facts,
            seen: Arc::default(),
        }
    }
}

impl Module for Stub {
    fn protocol(&self) -> Protocol {
        Protocol::Tls
    }
    fn checks(&self) -> &'static [CheckId] {
        &[]
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }
    fn wants_facts(&self) -> bool {
        self.wants_facts
    }
    fn run<'a>(&'a self, _ctx: &'a RunContext, facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        self.seen.lock().unwrap().push(facts.clone());
        Box::pin(async { measured() })
    }
}

/// Does not override `wants_facts`: exercises the trait default.
struct DefaultModule;

impl Module for DefaultModule {
    fn protocol(&self) -> Protocol {
        Protocol::Tls
    }
    fn checks(&self) -> &'static [CheckId] {
        &[]
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }
    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async { measured() })
    }
}

struct StubProvider {
    result: Result<Facts, ResolveError>,
    delay: Duration,
}

impl FactsProvider for StubProvider {
    fn resolve<'a>(
        &'a self,
        _ctx: &'a RunContext,
        _domain: &'a Domain,
    ) -> BoxFuture<'a, Result<Facts, ResolveError>> {
        let result = self.result.clone();
        let delay = self.delay;
        Box::pin(async move {
            tokio::time::sleep(delay).await;
            result
        })
    }
}

async fn drive(registry: &Registry, delay_budget: Duration) -> Vec<SectionEvent> {
    let base = RunContext {
        deadline: Instant::now() + MS(2000),
        domain: Domain::new("example.com"),
        options: RunOptions::default(),
    };
    let sections = [(Protocol::Tls, MS(1000))];
    let (tx, mut rx) = mpsc::channel(8);
    let collect = async {
        let mut events = Vec::new();
        while let Some(ev) = rx.recv().await {
            events.push(ev);
        }
        events
    };
    tokio::join!(run(registry, base, &sections, delay_budget, tx), collect).1
}

#[test]
fn wants_facts_defaults_to_false() {
    assert!(!DefaultModule.wants_facts());
}

#[tokio::test]
async fn wants_facts_module_sees_resolved_facts_after_a_delayed_resolve() {
    let stub = Stub::new(true);
    let seen = stub.seen.clone();
    let registry = Registry::new()
        .with_facts(Box::new(StubProvider {
            result: Ok(facts()),
            delay: MS(40),
        }))
        .with(Box::new(stub));

    let events = drive(&registry, MS(500)).await;

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].outcome, measured());
    assert_eq!(*seen.lock().unwrap(), vec![facts()]);
}

#[tokio::test]
async fn wants_facts_module_runs_with_empty_facts_when_resolve_fails() {
    let stub = Stub::new(true);
    let seen = stub.seen.clone();
    let registry = Registry::new()
        .with_facts(Box::new(StubProvider {
            result: Err(ResolveError("nameserver unreachable".into())),
            delay: MS(20),
        }))
        .with(Box::new(stub));

    let events = drive(&registry, MS(500)).await;

    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].outcome,
        measured(),
        "its own outcome, not Incomplete"
    );
    assert_eq!(*seen.lock().unwrap(), vec![Facts::default()]);
}

#[tokio::test]
async fn default_module_starts_with_empty_facts_without_waiting_for_the_resolve() {
    let stub = Stub::new(false);
    let seen = stub.seen.clone();
    let registry = Registry::new()
        .with_facts(Box::new(StubProvider {
            result: Ok(facts()),
            delay: MS(100),
        }))
        .with(Box::new(stub));

    let events = drive(&registry, MS(500)).await;

    assert_eq!(events[0].outcome, measured());
    assert_eq!(*seen.lock().unwrap(), vec![Facts::default()]);
}
