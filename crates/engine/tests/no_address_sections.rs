//! v2-unmeasured requirement 5: when A and AAAA are both definite negatives
//! (`Facts::no_address_definite`), the IP, HTTP and TLS sections are not applicable ("no A or AAAA
//! record") and are not asked to measure, so the run keeps a letter grade. Empty addresses without
//! the flag (SERVFAIL, timeout, referral) leave those sections incomplete and the grade
//! Incomplete. Stub modules and a stub FactsProvider only.

use std::time::{Duration, Instant};

use netray_engine::profile::Profiles;
use netray_engine::{
    BoxFuture, Domain, EvidencePath, Facts, FactsProvider, Module, Registry, ResolveError,
    RunContext, RunOptions, SectionEvent, SectionOutcome, run, score,
};
use netray_model::{CheckId, CheckResult, Grade, Protocol, Status};
use tokio::sync::mpsc;

const MS: fn(u64) -> Duration = Duration::from_millis;

fn passing(ids: &[&str]) -> SectionOutcome {
    SectionOutcome::Measured {
        checks: ids
            .iter()
            .map(|id| CheckResult {
                id: CheckId::parse(id).unwrap(),
                status: Status::Pass,
                findings: vec![],
                evidence: vec![],
            })
            .collect(),
        presentation: serde_json::Value::Null,
        evidence: vec![],
    }
}

/// Measures with one passing weighted check; with `needs_addresses` it mirrors the real modules
/// and is Incomplete when it sees no addresses.
struct Stub {
    protocol: Protocol,
    check: &'static str,
    needs_addresses: bool,
}

impl Module for Stub {
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
        let no_addresses = facts.a.is_empty() && facts.aaaa.is_empty();
        let outcome = if self.needs_addresses && no_addresses {
            SectionOutcome::Incomplete {
                reason: "no addresses".into(),
            }
        } else {
            passing(&[self.check])
        };
        Box::pin(async move { outcome })
    }
}

struct StubProvider(Facts);

impl FactsProvider for StubProvider {
    fn resolve<'a>(
        &'a self,
        _ctx: &'a RunContext,
        _domain: &'a Domain,
    ) -> BoxFuture<'a, Result<Facts, ResolveError>> {
        let facts = self.0.clone();
        Box::pin(async move { Ok(facts) })
    }
}

async fn run_with(facts: Facts) -> Vec<SectionEvent> {
    let stub = |protocol, check, needs_addresses| {
        Box::new(Stub {
            protocol,
            check,
            needs_addresses,
        })
    };
    let registry = Registry::new()
        .with_facts(Box::new(StubProvider(facts)))
        .with(stub(Protocol::Dns, "dns.dnssec", false))
        .with(stub(Protocol::Email, "email.spf", false))
        .with(stub(Protocol::Ip, "ip.reputation", true))
        .with(stub(Protocol::Http, "http.hsts", true))
        .with(stub(Protocol::Tls, "tls.tls_reachable", true));
    let base = RunContext {
        deadline: Instant::now() + MS(2000),
        domain: Domain::new("example.com"),
        options: RunOptions::default(),
    };
    let sections = [
        (Protocol::Dns, MS(1000)),
        (Protocol::Email, MS(1000)),
        (Protocol::Ip, MS(1000)),
        (Protocol::Http, MS(1000)),
        (Protocol::Tls, MS(1000)),
    ];
    let (tx, mut rx) = mpsc::channel(8);
    let collect = async {
        let mut events = Vec::new();
        while let Some(ev) = rx.recv().await {
            events.push(ev);
        }
        events
    };
    tokio::join!(run(&registry, base, &sections, MS(500), tx), collect).1
}

fn outcome(events: &[SectionEvent], protocol: Protocol) -> &SectionOutcome {
    &events
        .iter()
        .find(|e| e.protocol == protocol)
        .unwrap_or_else(|| panic!("no event for {protocol:?}"))
        .outcome
}

#[tokio::test]
async fn definite_no_address_makes_ip_http_tls_not_applicable_and_only_then() {
    // (no_address_definite, ip/http/tls are NotApplicable, run complete)
    let cases = [
        (true, true, true),    // C2, C5: both families definitely negative
        (false, false, false), // C6, C7, C8: SERVFAIL, timeout, referral
    ];
    for (definite, not_applicable, complete) in cases {
        let events = run_with(Facts {
            no_address_definite: definite,
            ..Facts::default()
        })
        .await;

        for protocol in [Protocol::Ip, Protocol::Http, Protocol::Tls] {
            let got = outcome(&events, protocol);
            if not_applicable {
                assert_eq!(
                    *got,
                    SectionOutcome::NotApplicable {
                        reason: "no A or AAAA record".into()
                    },
                    "{protocol:?} with definite={definite}"
                );
            } else {
                assert!(
                    !matches!(got, SectionOutcome::NotApplicable { .. }),
                    "{protocol:?} with definite={definite}: {got:?}"
                );
            }
        }
        for protocol in [Protocol::Dns, Protocol::Email] {
            assert!(
                matches!(outcome(&events, protocol), SectionOutcome::Measured { .. }),
                "{protocol:?} is measured with definite={definite}"
            );
        }

        let s = score(Profiles::get(3).unwrap(), &events);
        assert_eq!(s.complete, complete, "complete with definite={definite}");
        assert_eq!(
            s.grade != Grade::Incomplete,
            complete,
            "letter with definite={definite}: {:?}",
            s.grade
        );
    }
}
