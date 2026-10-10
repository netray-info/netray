//! R18: `netray_engine::run` — one resolve stage, concurrent modules under section and hard
//! deadlines, sections sent as they finish, overrun -> TimedOut, failed resolve -> the sections
//! that need addresses are Incomplete. Stub modules and a stub FactsProvider only; real short
//! sleeps (the run's deadlines are `std::time::Instant`, which tokio's paused clock does not move).

use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::atomic::{AtomicUsize, Ordering};
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
    }
}

fn facts() -> Facts {
    Facts {
        a: vec![Ipv4Addr::new(192, 0, 2, 1)],
        aaaa: vec![Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)],
        mx: vec!["mx.example.com".into()],
        caa: vec!["0 issue \"example.com\"".into()],
        ns: vec!["ns.example.com".into()],
        https: vec!["1 . alpn=h2".into()],
    }
}

struct StubModule {
    protocol: Protocol,
    sleep: Duration,
    needs_addresses: bool,
    finished: Arc<Mutex<Option<Instant>>>,
    seen: Arc<Mutex<Vec<Facts>>>,
}

impl StubModule {
    fn new(protocol: Protocol, sleep: Duration) -> Self {
        StubModule {
            protocol,
            sleep,
            needs_addresses: false,
            finished: Arc::default(),
            seen: Arc::default(),
        }
    }
}

impl Module for StubModule {
    fn protocol(&self) -> Protocol {
        self.protocol
    }
    fn checks(&self) -> &'static [CheckId] {
        &[]
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }
    fn needs_addresses(&self) -> bool {
        self.needs_addresses
    }
    fn run<'a>(&'a self, _ctx: &'a RunContext, facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        self.seen.lock().unwrap().push(facts.clone());
        Box::pin(async move {
            tokio::time::sleep(self.sleep).await;
            *self.finished.lock().unwrap() = Some(Instant::now());
            measured()
        })
    }
}

struct StubProvider {
    result: Result<Facts, ResolveError>,
    calls: Arc<AtomicUsize>,
}

impl StubProvider {
    fn ok() -> (Self, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        (
            StubProvider {
                result: Ok(facts()),
                calls: calls.clone(),
            },
            calls,
        )
    }
}

impl FactsProvider for StubProvider {
    fn resolve<'a>(
        &'a self,
        _ctx: &'a RunContext,
        _domain: &'a Domain,
    ) -> BoxFuture<'a, Result<Facts, ResolveError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let result = self.result.clone();
        Box::pin(async move { result })
    }
}

fn base(hard: Duration) -> RunContext {
    RunContext {
        deadline: Instant::now() + hard,
        domain: Domain::new("example.com"),
        options: RunOptions::default(),
    }
}

/// Runs the engine and collects every event with its arrival instant.
async fn drive(
    registry: &Registry,
    base: RunContext,
    sections: &[(Protocol, Duration)],
) -> (netray_engine::RunReport, Vec<(SectionEvent, Instant)>) {
    let (tx, mut rx) = mpsc::channel(8);
    let collect = async {
        let mut events = Vec::new();
        while let Some(ev) = rx.recv().await {
            events.push((ev, Instant::now()));
        }
        events
    };
    tokio::join!(run(registry, base, sections, tx), collect)
}

fn outcome(events: &[(SectionEvent, Instant)], protocol: Protocol) -> &SectionOutcome {
    &events
        .iter()
        .find(|(e, _)| e.protocol == protocol)
        .unwrap_or_else(|| panic!("no event for {protocol:?}"))
        .0
        .outcome
}

#[tokio::test]
async fn c4_one_resolve_stage_and_every_module_sees_the_same_facts() {
    let (provider, calls) = StubProvider::ok();
    let mods =
        [Protocol::Http, Protocol::Tls, Protocol::Ip].map(|p| StubModule::new(p, Duration::ZERO));
    let seen: Vec<_> = mods.iter().map(|m| m.seen.clone()).collect();
    let mut registry = Registry::new().with_facts(Box::new(provider));
    for m in mods {
        registry = registry.with(Box::new(m));
    }
    let sections = [
        (Protocol::Http, MS(500)),
        (Protocol::Tls, MS(500)),
        (Protocol::Ip, MS(500)),
    ];

    let (report, events) = drive(&registry, base(MS(2000)), &sections).await;

    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "resolve called once per run"
    );
    assert_eq!(events.len(), 3);
    for s in &seen {
        assert_eq!(
            *s.lock().unwrap(),
            vec![facts()],
            "module saw the resolved Facts"
        );
    }
    assert_eq!(report.facts, facts());
    assert_eq!(report.resolve_error, None);
}

#[tokio::test]
async fn c5_sections_arrive_in_finish_order_each_before_the_slower_finish() {
    let (provider, _) = StubProvider::ok();
    let slow = StubModule::new(Protocol::Http, MS(100));
    let fast = StubModule::new(Protocol::Tls, MS(10));
    let mid = StubModule::new(Protocol::Ip, MS(50));
    let (slow_done, mid_done) = (slow.finished.clone(), mid.finished.clone());
    let registry = Registry::new()
        .with_facts(Box::new(provider))
        .with(Box::new(slow))
        .with(Box::new(fast))
        .with(Box::new(mid));
    let sections = [
        (Protocol::Http, MS(1000)),
        (Protocol::Tls, MS(1000)),
        (Protocol::Ip, MS(1000)),
    ];

    let (_, events) = drive(&registry, base(MS(2000)), &sections).await;

    let order: Vec<Protocol> = events.iter().map(|(e, _)| e.protocol).collect();
    assert_eq!(order, vec![Protocol::Tls, Protocol::Ip, Protocol::Http]);
    let fast_arrival = events[0].1;
    let mid_arrival = events[1].1;
    assert!(
        fast_arrival < mid_done.lock().unwrap().unwrap(),
        "the 10 ms section arrived before the 50 ms module finished"
    );
    assert!(
        mid_arrival < slow_done.lock().unwrap().unwrap(),
        "the 50 ms section arrived before the 100 ms module finished"
    );
}

#[tokio::test]
async fn c6_overrun_of_section_deadline_is_timed_out_others_keep_results() {
    let (provider, _) = StubProvider::ok();
    let registry = Registry::new()
        .with_facts(Box::new(provider))
        .with(Box::new(StubModule::new(Protocol::Http, MS(200))))
        .with(Box::new(StubModule::new(Protocol::Tls, MS(5))))
        .with(Box::new(StubModule::new(Protocol::Ip, MS(5))));
    let sections = [
        (Protocol::Http, MS(20)),
        (Protocol::Tls, MS(500)),
        (Protocol::Ip, MS(500)),
    ];

    let (_, events) = drive(&registry, base(MS(2000)), &sections).await;

    assert_eq!(events.len(), 3);
    assert_eq!(outcome(&events, Protocol::Http), &SectionOutcome::TimedOut);
    assert_eq!(outcome(&events, Protocol::Tls), &measured());
    assert_eq!(outcome(&events, Protocol::Ip), &measured());
}

#[tokio::test]
async fn failed_resolve_makes_address_sections_incomplete_and_others_still_run() {
    let provider = StubProvider {
        result: Err(ResolveError("nameserver unreachable".into())),
        calls: Arc::default(),
    };
    let mut http = StubModule::new(Protocol::Http, Duration::ZERO);
    http.needs_addresses = true;
    let mut tls = StubModule::new(Protocol::Tls, Duration::ZERO);
    tls.needs_addresses = true;
    let email = StubModule::new(Protocol::Email, Duration::ZERO);
    let (http_seen, email_seen) = (http.seen.clone(), email.seen.clone());
    let registry = Registry::new()
        .with_facts(Box::new(provider))
        .with(Box::new(http))
        .with(Box::new(tls))
        .with(Box::new(email));
    let sections = [
        (Protocol::Http, MS(500)),
        (Protocol::Tls, MS(500)),
        (Protocol::Email, MS(500)),
    ];

    let (report, events) = drive(&registry, base(MS(2000)), &sections).await;

    assert_eq!(events.len(), 3);
    for p in [Protocol::Http, Protocol::Tls] {
        match outcome(&events, p) {
            SectionOutcome::Incomplete { reason } => {
                assert!(reason.contains("address resolution failed"), "{reason}")
            }
            other => panic!("{p:?}: expected Incomplete, got {other:?}"),
        }
    }
    assert!(
        http_seen.lock().unwrap().is_empty(),
        "an address module is not run"
    );
    assert_eq!(outcome(&events, Protocol::Email), &measured());
    assert_eq!(
        email_seen.lock().unwrap().len(),
        1,
        "a module without addresses still runs"
    );
    assert_eq!(
        report.resolve_error,
        Some(ResolveError("nameserver unreachable".into()))
    );
}

#[tokio::test]
async fn hard_deadline_caps_a_longer_section_deadline() {
    let (provider, _) = StubProvider::ok();
    let registry = Registry::new()
        .with_facts(Box::new(provider))
        .with(Box::new(StubModule::new(Protocol::Http, MS(2000))));
    let sections = [(Protocol::Http, Duration::from_secs(30))];

    let start = Instant::now();
    let (_, events) = drive(&registry, base(MS(60)), &sections).await;

    assert_eq!(outcome(&events, Protocol::Http), &SectionOutcome::TimedOut);
    assert!(
        start.elapsed() < MS(1000),
        "the hard deadline, not the 30 s section deadline, ended the run: {:?}",
        start.elapsed()
    );
}
