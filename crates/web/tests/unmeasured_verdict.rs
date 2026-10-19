//! V2 `Status::Unmeasured` reaches lens's wire intact (criteria C2, C7, C8, C9).
//!
//! The email section is a stub module that emits `email.dnsbl` with the status under test next
//! to one passing check; the other sections run their goldens. `Unmeasured` is a check nobody
//! could measure: the verdict is unmeasured and the grade is `incomplete`. `NotApplicable` stays
//! skip and the grade a letter.
//!
//! Copied from crates/lens/tests/unmeasured_verdict.rs (spec v2-cutover, Phase 6); dropped as V1:
//! the `/api/check` SSE wire (the run is admitted through `netray_web::admission::admit` and its
//! published output read instead of the streamed `email` and `summary` events) and C9, the
//! stored-snapshot test (the snapshot store is gone).

mod common;

use std::net::{IpAddr, Ipv4Addr};
use std::sync::LazyLock;

use common::{dns_golden_raw, facts_golden_raw, http_module, ip_golden, registry_with, tls_module};
use netray_engine::domain_run::CheckVerdict;
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use netray_web::admission::{Admit, Trigger, admit, until_done};

static CHECKS: LazyLock<Vec<CheckId>> = LazyLock::new(|| {
    vec![
        CheckId::parse("email.spf").unwrap(),
        CheckId::parse("email.dnsbl").unwrap(),
    ]
});

/// An email module whose `email.dnsbl` check carries `dnsbl`.
struct StubEmail {
    dnsbl: Status,
}

impl Module for StubEmail {
    fn protocol(&self) -> Protocol {
        Protocol::Email
    }
    fn checks(&self) -> &'static [CheckId] {
        &CHECKS
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }
    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        let dnsbl = self.dnsbl;
        Box::pin(async move {
            let result = |id: &str, status| CheckResult {
                id: CheckId::parse(id).unwrap(),
                status,
                findings: vec![],
                evidence: vec![],
            };
            SectionOutcome::Measured {
                checks: vec![
                    result("email.spf", Status::Pass),
                    result("email.dnsbl", dnsbl),
                ],
                evidence: vec![],
            }
        })
    }
}

/// The `email.dnsbl` verdict and the grade of one run.
async fn run(dnsbl: Status) -> (CheckVerdict, String) {
    let mut config = common::production_config();
    config.cache_enabled = false;
    let registry = registry_with(
        dns_golden_raw("prism.sse"),
        facts_golden_raw("prism.sse"),
        http_module(Some("spectra-inspect.json")),
        Box::new(StubEmail { dnsbl }),
        ip_golden("ifconfig-json.json"),
        tls_module(Some("tlsight-inspect.json")),
    );
    let plan = common::production_plan(&registry);
    let state = common::state_with(config, registry, plan);
    let client = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let Admit::Admitted(mut stream) = admit(&state, "example.com", client, Trigger::Api, false)
    else {
        panic!("the run is not admitted");
    };
    let published = until_done(&mut stream).await.expect("the run finishes");
    let email = published.output.sections["email"]
        .as_ref()
        .unwrap_or_else(|e| panic!("email section errored: {e:?}"));
    let verdict = email
        .checks
        .iter()
        .find(|c| c.name == "dnsbl")
        .unwrap_or_else(|| panic!("no dnsbl check in {:?}", email.checks))
        .verdict
        .clone();
    (verdict, published.output.score.grade.clone())
}

/// C7 and C8, two rows of one table: the verdict and whether the grade is a letter.
#[tokio::test]
async fn dnsbl_status_reaches_the_wire_as_its_own_verdict() {
    let rows = [
        // (status, verdict, grade is incomplete)
        (Status::Unmeasured, CheckVerdict::Unmeasured, true),
        (Status::NotApplicable, CheckVerdict::Skip, false),
    ];
    for (status, want_verdict, want_incomplete) in rows {
        let (verdict, grade) = run(status).await;
        assert_eq!(verdict, want_verdict, "verdict for {status:?}");
        assert_eq!(
            grade == "incomplete",
            want_incomplete,
            "grade for {status:?} was {grade}"
        );
    }
}
