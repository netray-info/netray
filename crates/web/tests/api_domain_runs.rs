//! spec v2-api, requirement 1: the `DomainRuns` seam between netray-web's API and the run machinery.
use std::net::IpAddr;
use std::sync::Arc;

use futures::future::BoxFuture;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::Run;
use netray_model::{Grade, Protocol};
use netray_web::api::{
    ApiConfig, ApiState, DomainRefusal, DomainRuns, RefusalReason, RunEvent, RunRefusal, RunStart,
    RunTrigger, router,
};

struct Stub;

impl DomainRuns for Stub {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        if domain.is_empty() {
            return Err(DomainRefusal {
                code: "DOMAIN_INVALID",
                message: "empty".into(),
            });
        }
        Ok(domain.to_lowercase())
    }

    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
    }
}

#[tokio::test]
async fn stub_is_usable_as_dyn_domain_runs_and_router_builds() {
    let runs: Arc<dyn DomainRuns> = Arc::new(Stub);
    assert_eq!(runs.validate("EXAMPLE.com"), Ok("example.com".to_string()));
    assert_eq!(runs.validate("").unwrap_err().code, "DOMAIN_INVALID");
    let client: IpAddr = [192, 0, 2, 1].into();
    let started = runs.start("example.com", client, RunTrigger::Api, false).await;
    assert!(matches!(started, RunStart::NotStarted));

    let state = ApiState::new(ApiConfig::default(), runs);
    let _router = router(state);
}

#[test]
fn run_vocabulary_is_constructible() {
    let triggers = [RunTrigger::Visit, RunTrigger::Recheck, RunTrigger::Api];
    assert_eq!(triggers.len(), 3);

    let reasons = [
        RefusalReason::Throttle,
        RefusalReason::Budget,
        RefusalReason::IpLimit,
        RefusalReason::GlobalLimit,
        RefusalReason::Slots,
    ];
    assert_eq!(reasons.len(), 5);

    let refusal = RunRefusal {
        reason: RefusalReason::Throttle,
        retry_after_s: 12,
        last_run_ago_s: Some(3),
        last_incomplete: true,
    };
    assert_eq!(refusal.retry_after_s, 12);
    assert_eq!(refusal.last_run_ago_s, Some(3));
    assert!(refusal.last_incomplete);

    let events: netray_web::api::RunEvents = Box::pin(futures::stream::empty::<RunEvent>());
    assert!(matches!(RunStart::Admitted(events), RunStart::Admitted(_)));
    assert!(matches!(
        RunStart::Refused(refusal),
        RunStart::Refused(RunRefusal {
            reason: RefusalReason::Throttle,
            ..
        })
    ));
    assert!(matches!(RunStart::NotStarted, RunStart::NotStarted));

    let section = RunEvent::Section(netray_engine::SectionEvent {
        protocol: Protocol::Dns,
        outcome: netray_engine::SectionOutcome::NotApplicable {
            reason: "none".into(),
        },
    });
    assert!(matches!(section, RunEvent::Section(_)));
    let run = Run {
        profile_version: 3,
        engine_version: "0.0.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![],
        checks: vec![],
        blocks: vec![],
    };
    assert!(matches!(RunEvent::Done(run), RunEvent::Done(_)));
    assert!(matches!(RunEvent::Recorded(None), RunEvent::Recorded(None)));
}
