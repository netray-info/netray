//! The domain run's grade is `netray_engine::score` with profile v3, a warn earning half (the
//! scoring half of lens's `tests/v3_render.rs`, V2 catalogue Phase 3 C8; copied for spec
//! v2-cutover Phase 6, assertions unchanged).
//!
//! lens ran the module goldens; the engine names no module crate, so stub modules answer every
//! catalogue check of their protocol as passing, and the HTTP module answers the thirteen v4
//! HTTP checks with known statuses (a weight-5 warn, a failing `csp`). The expected grade is
//! `netray_engine::score` over the events of a second run on the same registry.

use std::time::{Duration, Instant};

use netray_engine::domain_run::{CheckInput, HARD_DEADLINE, parse_grade, run_domain};
use netray_engine::profile::Profiles;
use netray_engine::{
    BoxFuture, Domain, EvidencePath, Facts, FactsProvider, Module, Registry, ResolveError,
    RunContext, RunOptions, SectionOutcome,
};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use tokio::sync::mpsc;

const HTTP_IDS: [&str; 13] = [
    "https_redirect",
    "hsts",
    "cors",
    "cookie_secure",
    "csp",
    "x_frame_options",
    "x_content_type_options",
    "referrer_policy",
    "permissions_policy",
    "coop",
    "deprecated_headers",
    "info_leakage",
    "caching",
];

const PROTOCOLS: [Protocol; 5] = [
    Protocol::Dns,
    Protocol::Tls,
    Protocol::Http,
    Protocol::Email,
    Protocol::Ip,
];

/// A module answering a measured section: the thirteen v4 HTTP checks for HTTP
/// (`https_redirect`, weight 5, warns; `csp` fails; the rest pass), every catalogue check of its
/// protocol passing otherwise.
struct Stub(Protocol);

impl Module for Stub {
    fn protocol(&self) -> Protocol {
        self.0
    }
    fn checks(&self) -> &'static [CheckId] {
        &[]
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }
    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        let check = |id: CheckId, status| CheckResult {
            id,
            status,
            findings: vec![],
            evidence: vec![],
        };
        let checks = match self.0 {
            Protocol::Http => HTTP_IDS
                .iter()
                .map(|n| {
                    let status = match *n {
                        "https_redirect" => Status::Warn,
                        "csp" => Status::Fail,
                        _ => Status::Pass,
                    };
                    check(CheckId::parse(&format!("http.{n}")).unwrap(), status)
                })
                .collect(),
            p => netray_engine::catalogue()
                .entries()
                .iter()
                .filter(|e| e.protocol == p)
                .map(|e| check(e.id.clone(), Status::Pass))
                .collect(),
        };
        Box::pin(async move {
            SectionOutcome::Measured {
                checks,
                evidence: vec![],
            }
        })
    }
}

/// A resolve stage answering one public address, so the IP section is scored.
struct OneAddress;

impl FactsProvider for OneAddress {
    fn resolve<'a>(
        &'a self,
        _ctx: &'a RunContext,
        _domain: &'a Domain,
    ) -> BoxFuture<'a, Result<Facts, ResolveError>> {
        Box::pin(async {
            Ok(Facts {
                a: vec!["1.1.1.1".parse().unwrap()],
                ..Facts::default()
            })
        })
    }
}

fn registry() -> Registry {
    PROTOCOLS
        .into_iter()
        .fold(Registry::new(), |r, p| r.with(Box::new(Stub(p))))
        .with_facts(Box::new(OneAddress))
}

#[tokio::test]
async fn the_grade_is_the_engine_score_with_a_warn_earning_half() {
    let registry = registry();
    let plan: Vec<(Protocol, Duration)> = PROTOCOLS
        .iter()
        .map(|p| (*p, Duration::from_secs(15)))
        .collect();
    let got = run_domain(
        &registry,
        &plan,
        Duration::from_secs(5),
        Profiles::current(),
        CheckInput {
            domain: "example.com".to_string(),
            dkim_selectors: None,
        },
        HARD_DEADLINE,
        |_| {},
    )
    .await;

    // The same registry, run directly: the events `netray_engine::score` takes.
    let base = RunContext {
        deadline: Instant::now() + Duration::from_secs(20),
        domain: Domain::new("example.com"),
        options: RunOptions::default(),
    };
    let (tx, mut rx) = mpsc::channel(8);
    let (_, events) = tokio::join!(
        netray_engine::run(&registry, base, &plan, Duration::from_secs(5), tx),
        async {
            let mut events = Vec::new();
            while let Some(e) = rx.recv().await {
                events.push(e);
            }
            events
        }
    );
    let want = netray_engine::score(Profiles::current(), &events);

    let summary = &got.output.score;
    assert_eq!(parse_grade(&summary.grade), Some(want.grade), "{summary:?}");
    assert_eq!(
        got.run.grade, want.grade,
        "the stored run carries the grade"
    );
    assert_eq!(
        summary.overall_percentage, want.overall_percentage,
        "{summary:?}"
    );
    // The weight-5 warn earns 2.5 of 5, not V1's integer 2.
    let http = &want.sections[&Protocol::Http];
    let weights = &Profiles::current().sections[&Protocol::Http].checks;
    let possible: u32 = weights.values().sum();
    assert_eq!(
        http.possible, possible,
        "control: every http check is weighted"
    );
    // Everything passes but the warning (5, earns 2.5) and csp (1, fails).
    assert_eq!(http.earned, (possible - 5 - 1) as f64 + 2.5);
}
